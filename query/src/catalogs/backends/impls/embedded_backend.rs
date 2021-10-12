//  Copyright 2021 Datafuse Labs.
//
//  Licensed under the Apache License, Version 2.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
//

use std::collections::HashMap;
use std::sync::Arc;

use common_exception::ErrorCode;
use common_infallible::RwLock;
use common_meta_types::CreateDatabaseReply;
use common_meta_types::CreateTableReply;
use common_meta_types::DatabaseInfo;
use common_meta_types::MetaId;
use common_meta_types::MetaVersion;
use common_meta_types::TableInfo;
use common_planners::CreateDatabasePlan;
use common_planners::CreateTablePlan;
use common_planners::DropDatabasePlan;
use common_planners::DropTablePlan;

use crate::catalogs::backends::CatalogBackend;
use crate::catalogs::table_id_ranges::LOCAL_TBL_ID_BEGIN;

/// This catalog backend used for test only.
struct InMemoryTableInfo {
    pub(crate) name2meta: HashMap<String, Arc<TableInfo>>,
    pub(crate) id2meta: HashMap<MetaId, Arc<TableInfo>>,
}

impl InMemoryTableInfo {
    pub fn create() -> Self {
        Self {
            name2meta: HashMap::default(),
            id2meta: HashMap::default(),
        }
    }

    pub fn insert(&mut self, table_info: TableInfo) {
        let met_ref = Arc::new(table_info);
        self.name2meta
            .insert(met_ref.name.to_owned(), met_ref.clone());
        self.id2meta.insert(met_ref.table_id, met_ref);
    }
}

type Databases = Arc<RwLock<HashMap<String, (Arc<DatabaseInfo>, InMemoryTableInfo)>>>;

pub struct EmbeddedCatalogBackend {
    databases: Databases,
    tbl_id_seq: Arc<RwLock<u64>>,
}

impl EmbeddedCatalogBackend {
    pub fn create() -> Self {
        let tbl_id_seq = Arc::new(RwLock::new(LOCAL_TBL_ID_BEGIN));
        Self {
            databases: Default::default(),
            tbl_id_seq,
        }
    }

    fn next_db_id(&self) -> u64 {
        *self.tbl_id_seq.write() += 1;
        let r = self.tbl_id_seq.read();
        *r
    }
}

impl CatalogBackend for EmbeddedCatalogBackend {
    fn create_database(
        &self,
        plan: CreateDatabasePlan,
    ) -> common_exception::Result<CreateDatabaseReply> {
        let db_name = plan.db.as_str();

        let mut db = self.databases.write();

        if db.get(db_name).is_some() {
            return if plan.if_not_exists {
                // TODO(xp): just let it pass. This file will be removed as soon as common/kv provides full meta-APIs.
                Ok(CreateDatabaseReply { database_id: 0 })
            } else {
                Err(ErrorCode::DatabaseAlreadyExists(format!(
                    "Database: '{}' already exists.",
                    db_name
                )))
            };
        }

        let database_info = DatabaseInfo {
            // TODO(xp): just let it pass. This file will be removed as soon as common/kv provides full meta-APIs.
            database_id: 0,
            db: db_name.to_string(),
            engine: plan.engine.clone(),
        };

        db.insert(
            plan.db,
            (Arc::new(database_info), InMemoryTableInfo::create()),
        );

        // TODO(xp): just let it pass. This file will be removed as soon as common/kv provides full meta-APIs.
        Ok(CreateDatabaseReply { database_id: 0 })
    }

    fn drop_database(&self, plan: DropDatabasePlan) -> common_exception::Result<()> {
        let db_name = plan.db.as_str();

        let removed = {
            let mut dbs = self.databases.write();
            dbs.remove(db_name)
        };

        if removed.is_some() {
            return Ok(());
        }

        // removed.is_none()

        if plan.if_exists {
            Ok(())
        } else {
            Err(ErrorCode::UnknownDatabase(format!(
                "Unknown database: '{}'",
                db_name
            )))
        }
    }

    fn get_database(&self, db_name: &str) -> common_exception::Result<Arc<DatabaseInfo>> {
        let lock = self.databases.read();
        let db = lock.get(db_name);
        match db {
            None => Err(ErrorCode::UnknownDatabase(format!(
                "Unknown database: '{}'",
                db_name
            ))),
            Some((v, _)) => Ok(v.clone()),
        }
    }

    fn get_databases(&self) -> common_exception::Result<Vec<Arc<DatabaseInfo>>> {
        let mut res = vec![];
        let lock = self.databases.read();
        let values = lock.values();
        for (db, _) in values {
            res.push(db.clone());
        }
        Ok(res)
    }

    fn create_table(&self, plan: CreateTablePlan) -> common_exception::Result<CreateTableReply> {
        let clone = plan.clone();
        let db_name = clone.db.as_str();
        let table_name = clone.table.as_str();

        let table_info = TableInfo {
            database_id: 0, // TODO tobe assigned to some real value
            db: plan.db,
            table_id: self.next_db_id(),
            version: 0,
            name: plan.table,
            schema: plan.schema,
            options: plan.options,
            engine: plan.engine,
        };

        let mut lock = self.databases.write();
        let v = lock.get_mut(db_name);
        match v {
            None => {
                return Err(ErrorCode::UnknownDatabase(format!(
                    "Unknown database: {}",
                    db_name
                )));
            }
            Some((_db_info, metas)) => {
                if metas.name2meta.get(table_name).is_some() {
                    if plan.if_not_exists {
                        // TODO(xp): just let it passed, gonna be removed
                        return Ok(CreateTableReply { table_id: 0 });
                    } else {
                        return Err(ErrorCode::TableAlreadyExists(format!(
                            "Table: '{}.{}' already exists.",
                            db_name, table_name,
                        )));
                    };
                }
                metas.insert(table_info);
            }
        }

        // TODO(xp): just let it passed, gonna be removed
        Ok(CreateTableReply { table_id: 0 })
    }

    fn drop_table(&self, plan: DropTablePlan) -> common_exception::Result<()> {
        let db_name = plan.db.as_str();
        let table_name = plan.table.as_str();

        let mut lock = self.databases.write();
        let v = lock.get(db_name);
        let tbl_id = match v {
            None => {
                return Err(ErrorCode::UnknownDatabase(format!(
                    "Unknown database: {}",
                    db_name
                )))
            }
            Some((_, metas)) => {
                let by_name = metas.name2meta.get(table_name);
                match by_name {
                    None => {
                        if plan.if_exists {
                            return Ok(());
                        } else {
                            return Err(ErrorCode::UnknownTable(format!(
                                "Unknown table: '{}.{}'",
                                db_name, table_name
                            )));
                        }
                    }
                    Some(tbl) => tbl.table_id,
                }
            }
        };

        let v = lock.get_mut(db_name);
        match v {
            None => {
                return Err(ErrorCode::UnknownDatabase(format!(
                    "Unknown database: {}",
                    db_name
                )))
            }
            Some((_, metas)) => {
                metas.name2meta.remove(table_name);
                metas.id2meta.remove(&tbl_id);
            }
        }

        Ok(())
    }

    fn get_table(
        &self,
        db_name: &str,
        table_name: &str,
    ) -> common_exception::Result<Arc<TableInfo>> {
        let lock = self.databases.read();
        let v = lock.get(db_name);
        match v {
            None => Err(ErrorCode::UnknownDatabase(format!(
                "Unknown database: {}",
                db_name
            ))),
            Some((_, metas)) => {
                let table = metas.name2meta.get(table_name).ok_or_else(|| {
                    ErrorCode::UnknownTable(format!("Unknown table: '{}'", table_name))
                })?;
                Ok(table.clone())
            }
        }
    }

    fn get_tables(&self, db_name: &str) -> common_exception::Result<Vec<Arc<TableInfo>>> {
        let mut res = vec![];
        let lock = self.databases.read();
        let v = lock.get(db_name);
        match v {
            None => {
                return Err(ErrorCode::UnknownDatabase(format!(
                    "Unknown database: {}",
                    db_name
                )));
            }
            Some((_, metas)) => {
                for meta in metas.name2meta.values() {
                    res.push(meta.clone());
                }
            }
        }
        Ok(res)
    }

    fn get_table_by_id(
        &self,
        table_id: MetaId,
        _table_version: Option<MetaVersion>,
    ) -> common_exception::Result<Arc<TableInfo>> {
        let map = self.databases.read();
        for (_, tbl_idx) in map.values() {
            match tbl_idx.id2meta.get(&table_id) {
                None => {
                    continue;
                }
                Some(tbl) => {
                    return Ok(tbl.clone());
                }
            }
        }

        Err(ErrorCode::UnknownTable(format!(
            "Unknown table of id: {}",
            table_id
        )))
    }

    fn name(&self) -> String {
        "embedded metastore backend".to_owned()
    }
}
