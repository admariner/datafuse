// Copyright 2020 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use async_raft::raft::Entry;
use async_raft::raft::EntryPayload;
use async_raft::raft::MembershipConfig;
use common_exception::prelude::ErrorCode;
use common_exception::ToErrorCode;
use common_meta_sled_store::get_sled_db;
use common_meta_sled_store::sled;
use common_meta_sled_store::AsKeySpace;
use common_meta_sled_store::SledTree;
use common_meta_types::Cmd;
use common_meta_types::Database;
use common_meta_types::KVMeta;
use common_meta_types::KVValue;
use common_meta_types::LogEntry;
use common_meta_types::LogId;
use common_meta_types::MatchSeqExt;
use common_meta_types::Node;
use common_meta_types::NodeId;
use common_meta_types::Operation;
use common_meta_types::SeqValue;
use common_meta_types::Slot;
use common_meta_types::Table;
use common_tracing::tracing;
use serde::Deserialize;
use serde::Serialize;
use sled::IVec;

use crate::config::RaftConfig;
use crate::sled_key_spaces::Files;
use crate::sled_key_spaces::GenericKV;
use crate::sled_key_spaces::Nodes;
use crate::sled_key_spaces::Sequences;
use crate::sled_key_spaces::StateMachineMeta;
use crate::state_machine::placement::rand_n_from_m;
use crate::state_machine::AppliedState;
use crate::state_machine::Placement;
use crate::state_machine::StateMachineMetaKey;
use crate::state_machine::StateMachineMetaKey::Initialized;
use crate::state_machine::StateMachineMetaKey::LastApplied;
use crate::state_machine::StateMachineMetaKey::LastMembership;
use crate::state_machine::StateMachineMetaValue;

/// seq number key to generate seq for the value of a `generic_kv` record.
const SEQ_GENERIC_KV: &str = "generic_kv";
/// seq number key to generate database id
const SEQ_DATABASE_ID: &str = "database_id";
/// seq number key to generate table id
const SEQ_TABLE_ID: &str = "table_id";
/// seq number key to database meta version
const SEQ_DATABASE_META_ID: &str = "database_meta_id";

/// sled db tree name for nodes
// const TREE_NODES: &str = "nodes";
// const TREE_META: &str = "meta";
const TREE_STATE_MACHINE: &str = "state_machine";

/// Replication defines the replication strategy.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Replication {
    /// n-copies mode.
    Mirror(u64),
}

impl Default for Replication {
    fn default() -> Self {
        Replication::Mirror(1)
    }
}

/// The state machine of the `MemStore`.
/// It includes user data and two raft-related informations:
/// `last_applied_logs` and `client_serial_responses` to achieve idempotence.
#[derive(Debug)]
pub struct StateMachine {
    // TODO(xp): config is not required. Remove it after snapshot is done.
    _config: RaftConfig,

    /// The dedicated sled db to store everything about a state machine.
    /// A state machine has several trees opened on this db.
    _db: sled::Db,

    /// The internal sled::Tree to store everything about a state machine:
    /// - Store initialization state and last applied in keyspace `StateMachineMeta`.
    /// - Every other state is store in its own keyspace such as `Nodes`.
    ///
    /// TODO(xp): migrate other in-memory fields to `sm_tree`.
    pub sm_tree: SledTree,

    /// raft state: A mapping of client IDs to their state info:
    /// (serial, RaftResponse)
    /// This is used to de-dup client request, to impl idempotent operations.
    pub client_last_resp: HashMap<String, (u64, AppliedState)>,

    // cluster nodes, key distribution etc.
    pub slots: Vec<Slot>,

    pub replication: Replication,

    /// db name to database mapping
    pub databases: BTreeMap<String, Database>,

    /// table id to table mapping
    pub tables: BTreeMap<u64, Table>,
}

/// Initialize state machine for the first time it is brought online.
#[derive(Debug, Default, Clone)]
pub struct StateMachineInitializer {
    /// The number of slots to allocated.
    initial_slots: Option<u64>,
    /// The replication strategy.
    replication: Option<Replication>,
}

impl StateMachineInitializer {
    /// Set the number of slots to boot up a cluster.
    pub fn slots(mut self, n: u64) -> Self {
        self.initial_slots = Some(n);
        self
    }

    /// Specifies the cluster to replicate by mirror `n` copies of every file.
    pub fn mirror_replication(mut self, n: u64) -> Self {
        self.replication = Some(Replication::Mirror(n));
        self
    }

    /// Initialized the state machine for when it is created.
    pub fn init(&self, mut sm: StateMachine) -> StateMachine {
        let initial_slots = self.initial_slots.unwrap_or(3);
        let replication = self.replication.clone().unwrap_or(Replication::Mirror(1));

        sm.replication = replication;

        sm.slots.clear();

        for _i in 0..initial_slots {
            sm.slots.push(Slot::default());
        }

        sm
    }
}

/// A key-value pair in a snapshot is a vec of two `Vec<u8>`.
pub type SnapshotKeyValue = Vec<Vec<u8>>;

/// Snapshot data for serialization and for transport.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct SerializableSnapshot {
    /// A list of kv pairs.
    pub kvs: Vec<SnapshotKeyValue>,
}

impl SerializableSnapshot {
    /// Convert the snapshot to a `Vec<(type, name, iter)>` format for sled to import.
    pub fn sled_importable(self) -> Vec<(Vec<u8>, Vec<u8>, impl Iterator<Item = Vec<Vec<u8>>>)> {
        vec![(
            "tree".as_bytes().to_vec(),
            TREE_STATE_MACHINE.as_bytes().to_vec(),
            self.kvs.into_iter(),
        )]
    }
}

impl StateMachine {
    pub fn initializer() -> StateMachineInitializer {
        StateMachineInitializer {
            ..Default::default()
        }
    }

    #[tracing::instrument(level = "debug", skip(config), fields(config_id=%config.config_id, prefix=%config.sled_tree_prefix))]
    pub fn tree_name(config: &RaftConfig, sm_id: u64) -> String {
        config.tree_name(format!("{}/{}", TREE_STATE_MACHINE, sm_id))
    }

    #[tracing::instrument(level = "debug", skip(config), fields(config_id=config.config_id.as_str()))]
    pub fn clean(config: &RaftConfig, sm_id: u64) -> common_exception::Result<()> {
        let tree_name = StateMachine::tree_name(config, sm_id);

        let db = get_sled_db();

        // it blocks and slow
        db.drop_tree(tree_name)
            .map_err_to_code(ErrorCode::MetaStoreDamaged, || "drop prev state machine")?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(config), fields(config_id=config.config_id.as_str()))]
    pub async fn open(config: &RaftConfig, sm_id: u64) -> common_exception::Result<StateMachine> {
        let db = get_sled_db();

        let tree_name = StateMachine::tree_name(config, sm_id);

        let sm_tree = SledTree::open(&db, &tree_name, config.is_sync())?;

        let sm = StateMachine {
            _config: config.clone(),
            _db: db,

            sm_tree,

            client_last_resp: Default::default(),
            slots: Vec::new(),

            replication: Replication::Mirror(1),
            databases: BTreeMap::new(),
            tables: BTreeMap::new(),
        };

        let inited = {
            let sm_meta = sm.sm_meta();
            sm_meta.get(&Initialized)?
        };

        if inited.is_some() {
            Ok(sm)
        } else {
            // Run the default init on a new state machine.
            // TODO(xp): initialization should be customizable.
            let sm = StateMachine::initializer().init(sm);
            let sm_meta = sm.sm_meta();
            sm_meta
                .insert(&Initialized, &StateMachineMetaValue::Bool(true))
                .await?;
            Ok(sm)
        }
    }

    /// Create a snapshot.
    /// Returns:
    /// - an consistent iterator of all kvs;
    /// - the last applied log id
    /// - the last applied membership config
    /// - and a snapshot id
    pub fn snapshot(
        &self,
    ) -> common_exception::Result<(
        impl Iterator<Item = sled::Result<(IVec, IVec)>>,
        LogId,
        MembershipConfig,
        String,
    )> {
        let last_applied = self.get_last_applied()?;
        let mem = self.get_membership()?;

        // NOTE: An initialize node/cluster always has the first log contains membership config.
        let mem = mem.unwrap_or_default();

        let snapshot_idx = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let snapshot_id = format!(
            "{}-{}-{}",
            last_applied.term, last_applied.index, snapshot_idx
        );

        Ok((self.sm_tree.tree.iter(), last_applied, mem, snapshot_id))
    }

    /// Serialize a snapshot for transport.
    /// This step does not require a lock, since sled::Tree::iter() creates a consistent view on a tree
    /// no matter if there are other writes applied to the tree.
    pub fn serialize_snapshot(
        view: impl Iterator<Item = sled::Result<(IVec, IVec)>>,
    ) -> common_exception::Result<Vec<u8>> {
        let mut kvs = Vec::new();
        for rkv in view {
            let (k, v) = rkv.map_err_to_code(ErrorCode::MetaStoreDamaged, || "taking snapshot")?;
            kvs.push(vec![k.to_vec(), v.to_vec()]);
        }
        let snap = SerializableSnapshot { kvs };
        let snap = serde_json::to_vec(&snap)?;
        Ok(snap)
    }

    /// Internal func to get an auto-incr seq number.
    /// It is just what Cmd::IncrSeq does and is also used by Cmd that requires
    /// a unique id such as Cmd::AddDatabase which needs make a new database id.
    ///
    /// Note: this can only be called inside apply().
    async fn incr_seq(&self, key: &str) -> common_exception::Result<u64> {
        let sequences = self.sequences();

        let curr = sequences
            .update_and_fetch(&key.to_string(), |old| Some(old.unwrap_or_default() + 1))
            .await?;

        let curr = curr.unwrap();

        tracing::debug!("applied IncrSeq: {}={}", key, curr);

        Ok(curr.0)
    }

    /// Apply an log entry to state machine.
    ///
    /// If a duplicated log entry is detected by checking data.txid, no update
    /// will be made and the previous resp is returned. In this way a client is able to re-send a
    /// command safely in case of network failure etc.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn apply(
        &mut self,
        entry: &Entry<LogEntry>,
    ) -> common_exception::Result<AppliedState> {
        // TODO(xp): all update need to be done in a tx.

        let log_id = &entry.log_id;

        let sm_meta = self.sm_meta();
        sm_meta
            .insert(&LastApplied, &StateMachineMetaValue::LogId(*log_id))
            .await?;

        match entry.payload {
            EntryPayload::Blank => {}
            EntryPayload::Normal(ref norm) => {
                let data = &norm.data;
                if let Some(ref txid) = data.txid {
                    if let Some((serial, resp)) = self.client_last_resp.get(&txid.client) {
                        if serial == &txid.serial {
                            return Ok(resp.clone());
                        }
                    }
                }

                let resp = self.apply_cmd(&data.cmd).await?;

                if let Some(ref txid) = data.txid {
                    self.client_last_resp
                        .insert(txid.client.clone(), (txid.serial, resp.clone()));
                }
                return Ok(resp);
            }
            EntryPayload::ConfigChange(ref mem) => {
                sm_meta
                    .insert(
                        &LastMembership,
                        &StateMachineMetaValue::Membership(mem.membership.clone()),
                    )
                    .await?;
                return Ok(AppliedState::None);
            }
            EntryPayload::SnapshotPointer(_) => {}
        };

        Ok(AppliedState::None)
    }

    /// Apply a `Cmd` to state machine.
    ///
    /// Already applied log should be filtered out before passing into this function.
    /// This is the only entry to modify state machine.
    /// The `cmd` is always committed by raft before applying.
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn apply_cmd(&mut self, cmd: &Cmd) -> common_exception::Result<AppliedState> {
        match cmd {
            Cmd::AddFile { ref key, ref value } => {
                // TODO(xp): put it in a transaction
                let files = self.files();

                let prev = files.get(key)?;
                if prev.is_none() {
                    files.insert(key, value).await?;
                    tracing::info!("applied AddFile: {}={}", key, value);
                    Ok((prev, Some(value.clone())).into())
                } else {
                    // TODO(xp): failure to add should returns `prev` as `result`
                    Ok((prev, None).into())
                }
            }

            Cmd::SetFile { ref key, ref value } => {
                let files = self.files();

                let prev = files.insert(key, value).await?;
                tracing::info!("applied SetFile: {}={}", key, value);
                Ok((prev, Some(value.clone())).into())
            }

            Cmd::IncrSeq { ref key } => Ok(self.incr_seq(key).await?.into()),

            Cmd::AddNode {
                ref node_id,
                ref node,
            } => {
                let sm_nodes = self.nodes();

                let prev = sm_nodes.get(node_id)?;

                if prev.is_some() {
                    Ok((prev, None).into())
                } else {
                    sm_nodes.insert(node_id, node).await?;
                    tracing::info!("applied AddNode: {}={:?}", node_id, node);
                    Ok((prev, Some(node.clone())).into())
                }
            }

            Cmd::CreateDatabase {
                ref name, ref db, ..
            } => {
                // - If the db present, return it.
                // - Otherwise, create a new one with next seq number as database id, and add it in to store.
                if self.databases.contains_key(name) {
                    let prev = self.databases.get(name);
                    Ok((prev.cloned(), prev.cloned()).into())
                } else {
                    let db = Database {
                        database_id: self.incr_seq(SEQ_DATABASE_ID).await?,
                        database_engine: db.database_engine.clone(),
                        tables: Default::default(),
                    };
                    self.incr_seq(SEQ_DATABASE_META_ID).await?;

                    self.databases.insert(name.clone(), db.clone());
                    tracing::debug!("applied CreateDatabase: {}={:?}", name, db);

                    Ok((None, Some(db)).into())
                }
            }

            Cmd::DropDatabase { ref name } => {
                let prev = self.databases.get(name).cloned();
                if prev.is_some() {
                    self.databases.remove(name);
                    self.incr_seq(SEQ_DATABASE_META_ID).await?;
                    tracing::debug!("applied DropDatabase: {}", name);
                    Ok((prev, None).into())
                } else {
                    Ok((None::<Database>, None::<Database>).into())
                }
            }

            Cmd::CreateTable {
                ref db_name,
                ref table_name,
                if_not_exists: _,
                ref table,
            } => {
                let db = self.databases.get(db_name);
                let mut db = db.unwrap().to_owned();

                if db.tables.contains_key(table_name) {
                    let table_id = db.tables.get(table_name).unwrap();
                    let prev = self.tables.get(table_id);
                    Ok((prev.cloned(), prev.cloned()).into())
                } else {
                    let table = Table {
                        table_id: self.incr_seq(SEQ_TABLE_ID).await?,
                        table_name: table_name.to_string(),
                        database_id: db.database_id,
                        db_name: db_name.to_string(),
                        schema: table.schema.clone(),
                        table_engine: table.table_engine.clone(),
                        table_options: table.table_options.clone(),
                        parts: table.parts.clone(),
                    };
                    self.incr_seq(SEQ_DATABASE_META_ID).await?;
                    db.tables.insert(table_name.clone(), table.table_id);
                    self.databases.insert(db_name.clone(), db);
                    self.tables.insert(table.table_id, table.clone());
                    tracing::debug!("applied CreateTable: {}={:?}", table_name, table);

                    Ok((None, Some(table)).into())
                }
            }

            Cmd::DropTable {
                ref db_name,
                ref table_name,
                if_exists: _,
            } => {
                let db = self.databases.get_mut(db_name).unwrap();
                let tbl_id = db.tables.get(table_name);
                if let Some(tbl_id) = tbl_id {
                    let tbl_id = tbl_id.to_owned();
                    db.tables.remove(table_name);
                    let prev = self.tables.remove(&tbl_id);
                    self.incr_seq(SEQ_DATABASE_META_ID).await?;

                    Ok((prev, None).into())
                } else {
                    Ok((None::<Table>, None::<Table>).into())
                }
            }

            Cmd::UpsertKV {
                ref key,
                ref seq,
                value: ref value_op,
                ref value_meta,
            } => {
                // TODO(xp): need to be done all in a tx
                // TODO(xp): now must be a timestamp extracted from raft log.
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                let kvs = self.kvs();
                let prev = kvs.get(key)?;

                // If prev is timed out, treat it as a None.
                let prev = match prev {
                    None => None,
                    Some(ref p) => {
                        if p.1 < now {
                            None
                        } else {
                            prev
                        }
                    }
                };

                if seq.match_seq(&prev).is_err() {
                    return Ok((prev.clone(), prev).into());
                }

                // result is the state after applying an operation.
                let result;

                match value_op {
                    Operation::Update(v) => {
                        result = self.kv_update(key, value_meta, v).await?;
                    }
                    Operation::Delete => {
                        kvs.remove(key, true).await?;
                        result = None;
                    }
                    Operation::AsIs => {
                        result = match prev {
                            None => None,
                            Some((_, ref curr_kv_value)) => {
                                self.kv_update(key, value_meta, &curr_kv_value.value)
                                    .await?
                            }
                        };
                    }
                }

                tracing::debug!("applied UpsertKV: {} {:?}", key, result);
                Ok((prev, result).into())
            }
        }
    }

    /// Update a generic-kv record, without seq checking
    async fn kv_update(
        &self,
        key: &str,
        value_meta: &Option<KVMeta>,
        v: &[u8],
    ) -> common_exception::Result<Option<SeqValue<KVValue>>> {
        let new_seq = self.incr_seq(SEQ_GENERIC_KV).await?;

        let kv_value = KVValue {
            meta: value_meta.clone(),
            value: v.to_vec(),
        };
        let seq_kv_value = (new_seq, kv_value);

        let kvs = self.kvs();
        kvs.insert(&key.to_string(), &seq_kv_value).await?;

        Ok(Some(seq_kv_value))
    }

    pub fn get_membership(&self) -> common_exception::Result<Option<MembershipConfig>> {
        let sm_meta = self.sm_meta();
        let mem = sm_meta
            .get(&StateMachineMetaKey::LastMembership)?
            .map(MembershipConfig::from);

        Ok(mem)
    }

    pub fn get_last_applied(&self) -> common_exception::Result<LogId> {
        let sm_meta = self.sm_meta();
        let last_applied = sm_meta
            .get(&LastApplied)?
            .map(LogId::from)
            .unwrap_or_default();

        Ok(last_applied)
    }

    /// Initialize slots by assign nodes to everyone of them randomly, according to replicationn config.
    pub fn init_slots(&mut self) -> common_exception::Result<()> {
        for i in 0..self.slots.len() {
            self.assign_rand_nodes_to_slot(i)?;
        }

        Ok(())
    }

    /// Assign `n` random nodes to a slot thus the files associated to this slot are replicated to the corresponding nodes.
    /// This func does not cnosider nodes load and should only be used when a Dfs cluster is initiated.
    /// TODO(xp): add another func for load based assignment
    pub fn assign_rand_nodes_to_slot(&mut self, slot_index: usize) -> common_exception::Result<()> {
        let n = match self.replication {
            Replication::Mirror(x) => x,
        } as usize;

        let mut node_ids = self.list_node_ids();
        node_ids.sort_unstable();
        let total = node_ids.len();
        let node_indexes = rand_n_from_m(total, n)?;

        let mut slot = self
            .slots
            .get_mut(slot_index)
            .ok_or_else(|| ErrorCode::InvalidConfig(format!("slot not found: {}", slot_index)))?;

        slot.node_ids = node_indexes.iter().map(|i| node_ids[*i]).collect();

        Ok(())
    }

    fn list_node_ids(&self) -> Vec<NodeId> {
        let sm_nodes = self.nodes();
        sm_nodes.range_keys(..).expect("fail to list nodes")
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub fn get_file(&self, key: &str) -> common_exception::Result<Option<String>> {
        tracing::debug!("SM::get_file: {}", key);

        let files = self.files();
        let x = files.get(&key.to_string())?;

        tracing::debug!("SM::get_file: {}={:?}", key, x);
        Ok(x)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub fn list_files(&self, prefix: &str) -> common_exception::Result<Vec<String>> {
        let files = self.files();
        let fns = files.scan_prefix(&prefix.to_string())?;
        Ok(fns.into_iter().map(|(k, _v)| k).collect())
    }

    pub fn get_node(&self, node_id: &NodeId) -> common_exception::Result<Option<Node>> {
        let sm_nodes = self.nodes();
        sm_nodes.get(node_id)
    }

    pub fn get_database(&self, name: &str) -> Option<Database> {
        let x = self.databases.get(name);
        x.cloned()
    }

    pub fn get_databases(&self) -> &BTreeMap<String, Database> {
        &self.databases
    }

    pub fn get_database_meta_ver(&self) -> common_exception::Result<Option<u64>> {
        let sequences = self.sequences();
        let res = sequences.get(&SEQ_DATABASE_META_ID.to_string())?;
        Ok(res.map(|x| x.0))
    }

    pub fn get_table(&self, tid: &u64) -> Option<Table> {
        let x = self.tables.get(tid);
        x.cloned()
    }

    pub fn get_kv(&self, key: &str) -> common_exception::Result<Option<SeqValue<KVValue>>> {
        // TODO(xp) refine get(): a &str is enough for key
        let sv = self.kvs().get(&key.to_string())?;
        tracing::debug!("get_kv sv:{:?}", sv);
        let sv = match sv {
            None => return Ok(None),
            Some(sv) => sv,
        };

        Ok(Self::unexpired(sv))
    }

    pub fn mget_kv(
        &self,
        keys: &[impl AsRef<str>],
    ) -> common_exception::Result<Vec<Option<SeqValue<KVValue>>>> {
        let kvs = self.kvs();
        let mut res = vec![];
        for x in keys.iter() {
            let v = kvs.get(&x.as_ref().to_string())?;
            let v = Self::unexpired_opt(v);
            res.push(v)
        }

        Ok(res)
    }

    pub fn prefix_list_kv(
        &self,
        prefix: &str,
    ) -> common_exception::Result<Vec<(String, SeqValue<KVValue>)>> {
        let kvs = self.kvs();
        let kv_pairs = kvs.scan_prefix(&prefix.to_string())?;

        let x = kv_pairs.into_iter();

        // Convert expired to None
        let x = x.map(|(k, v)| (k, Self::unexpired(v)));
        // Remove None
        let x = x.filter(|(_k, v)| v.is_some());
        // Extract from an Option
        let x = x.map(|(k, v)| (k, v.unwrap()));

        Ok(x.collect())
    }

    fn unexpired_opt(seq_value: Option<SeqValue<KVValue>>) -> Option<SeqValue<KVValue>> {
        match seq_value {
            None => None,
            Some(sv) => Self::unexpired(sv),
        }
    }
    fn unexpired(seq_value: SeqValue<KVValue>) -> Option<SeqValue<KVValue>> {
        // TODO(xp): log must be assigned with a ts.

        // TODO(xp): background task to clean expired

        // TODO(xp): Caveat: The cleanup must be consistent across raft nodes:
        //           A conditional update, e.g. an upsert_kv() with MatchSeq::Eq(some_value),
        //           must be applied with the same timestamp on every raft node.
        //           Otherwise: node-1 could have applied a log with a ts that is smaller than value.expire_at,
        //           while node-2 may fail to apply the same log if it use a greater ts > value.expire_at.
        //           Thus:
        //           1. A raft log must have a field ts assigned by the leader. When applying, use this ts to
        //              check against expire_at to decide whether to purge it.
        //           2. A GET operation must not purge any expired entry. Since a GET is only applied to a node itself.
        //           3. The background task can only be triggered by the raft leader, by submit a "clean expired" log.

        // TODO(xp): maybe it needs a expiration queue for efficient cleaning up.

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        tracing::debug!("seq_value: {:?} now: {}", seq_value, now);

        if seq_value.1 < now {
            None
        } else {
            Some(seq_value)
        }
    }
}

/// Key space support
impl StateMachine {
    pub fn sm_meta(&self) -> AsKeySpace<StateMachineMeta> {
        self.sm_tree.key_space()
    }

    pub fn nodes(&self) -> AsKeySpace<Nodes> {
        self.sm_tree.key_space()
    }

    /// The file names stored in this cluster
    pub fn files(&self) -> AsKeySpace<Files> {
        self.sm_tree.key_space()
    }

    /// A kv store of all other general purpose information.
    /// The value is tuple of a monotonic sequence number and userdata value in string.
    /// The sequence number is guaranteed to increment(by some value greater than 0) everytime the record changes.
    pub fn kvs(&self) -> AsKeySpace<GenericKV> {
        self.sm_tree.key_space()
    }

    /// storage of auto-incremental number.
    pub fn sequences(&self) -> AsKeySpace<Sequences> {
        self.sm_tree.key_space()
    }
}

impl Placement for StateMachine {
    fn get_slots(&self) -> &[Slot] {
        &self.slots
    }

    fn get_placement_node(&self, node_id: &NodeId) -> common_exception::Result<Option<Node>> {
        self.get_node(node_id)
    }
}
