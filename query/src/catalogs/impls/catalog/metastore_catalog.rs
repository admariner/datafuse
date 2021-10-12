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
use common_exception::Result;
use common_infallible::RwLock;
use common_meta_types::CreateDatabaseReply;
use common_meta_types::DatabaseInfo;
use common_meta_types::MetaId;
use common_meta_types::MetaVersion;
use common_planners::CreateDatabasePlan;
use common_planners::DropDatabasePlan;

use crate::catalogs::backends::CatalogBackend;
use crate::catalogs::backends::EmbeddedCatalogBackend;
use crate::catalogs::backends::RemoteCatalogBackend;
use crate::catalogs::catalog::Catalog;
use crate::catalogs::Database;
use crate::catalogs::TableMeta;
use crate::common::MetaClientProvider;
use crate::configs::Config;
use crate::datasources::database::prelude::register_prelude_db_engines;
use crate::datasources::database_engine::DatabaseEngine;
use crate::datasources::database_engine_registry::DatabaseEngineRegistry;
use crate::datasources::database_engine_registry::EngineDescription;
use crate::datasources::table::register_prelude_tbl_engines;
use crate::datasources::table_engine_registry::TableEngineRegistry;

pub const DEFAULT_DB_ENGINE: &str = "Default";

/// Catalog based on MetaStore
/// - System Database NOT included
/// - Meta data of databases are saved in meta store
/// - Instances of `Database` are created by using database factories according to the engine
/// - Database engines are free to save table meta in metastore or not
pub struct MetaStoreCatalog {
    conf: Config,
    db_engine_registry: Arc<DatabaseEngineRegistry>,
    catalog_backend: Arc<dyn CatalogBackend>,

    // this is not for performance:
    // some tables are stateful, cached in database, thus, instances of Database have to be kept as well.
    //
    // if we drop Database Trait, and create tables by using catalog directly, things may be easier
    db_instances: RwLock<HashMap<String, Arc<dyn Database>>>,
}

impl MetaStoreCatalog {
    pub fn try_create_with_config(conf: Config) -> Result<Self> {
        let local_mode = conf.meta.meta_address.is_empty();
        let catalog_backend: Arc<dyn CatalogBackend> = if local_mode {
            Arc::new(EmbeddedCatalogBackend::create())
        } else {
            let store_client_provider = Arc::new(MetaClientProvider::new(&conf));
            Arc::new(RemoteCatalogBackend::create(store_client_provider))
        };

        let plan = CreateDatabasePlan {
            if_not_exists: true,
            db: "default".to_string(),
            engine: DEFAULT_DB_ENGINE.to_string(),
            options: Default::default(),
        };
        catalog_backend.create_database(plan)?;

        let db_engine_registry = Arc::new(DatabaseEngineRegistry::new());
        let table_engine_registry = Arc::new(TableEngineRegistry::new());

        register_prelude_tbl_engines(&table_engine_registry)?;
        register_prelude_db_engines(
            &db_engine_registry,
            catalog_backend.clone(),
            table_engine_registry,
        )?;

        let cat = MetaStoreCatalog {
            conf,
            db_engine_registry,
            catalog_backend,
            db_instances: RwLock::new(HashMap::new()),
        };

        Ok(cat)
    }

    // Get all the engines name.
    #[allow(dead_code)]
    pub fn engines(&self) -> Vec<String> {
        todo!()
    }

    fn build_db_instance(&self, db_info: &Arc<DatabaseInfo>) -> Result<Arc<dyn Database>> {
        let engine = if db_info.engine.is_empty() {
            "default" // TODO user default table constant
        } else {
            &db_info.engine
        };

        let provider = self
            .db_engine_registry
            .engine_provider(engine)
            .ok_or_else(|| {
                ErrorCode::UnknownDatabaseEngine(format!(
                    "unknown database engine [{}]",
                    db_info.engine
                ))
            })?;

        let name = db_info.db.clone();
        let db = provider.create(&self.conf, db_info)?;
        self.db_instances.write().insert(name, db.clone());
        Ok(db)
    }
}

impl Catalog for MetaStoreCatalog {
    fn register_db_engine(
        &self,
        engine_type: &str,
        backend: Arc<dyn DatabaseEngine>,
    ) -> Result<()> {
        self.db_engine_registry.register(engine_type, backend)
    }

    fn get_databases(&self) -> Result<Vec<Arc<dyn Database>>> {
        let dbs = self.catalog_backend.get_databases()?;
        dbs.iter().try_fold(vec![], |mut acc, item| {
            let db = self.build_db_instance(item)?;
            acc.push(db);
            Ok(acc)
        })
    }

    fn get_database(&self, db_name: &str) -> Result<Arc<dyn Database>> {
        {
            if let Some(db) = self.db_instances.read().get(db_name) {
                return Ok(db.clone());
            }
        }
        let db_info = self.catalog_backend.get_database(db_name)?;
        self.build_db_instance(&db_info)
    }

    fn get_table(&self, db_name: &str, table_name: &str) -> Result<Arc<TableMeta>> {
        let db = self.get_database(db_name)?;
        db.get_table(table_name)
    }

    fn get_table_by_id(
        &self,
        table_id: MetaId,
        table_version: Option<MetaVersion>,
    ) -> Result<Arc<TableMeta>> {
        let table_info = self
            .catalog_backend
            .get_table_by_id(table_id, table_version)?;
        // table factories are insides Database, tobe optimized latter
        let db = self.get_database(&table_info.db)?;
        db.get_table_by_id(table_id, table_version)
    }

    fn create_database(&self, plan: CreateDatabasePlan) -> Result<CreateDatabaseReply> {
        if self.db_engine_registry.contains(&plan.engine) {
            // TODO check if plan is valid (add validate method to database_factory)
            self.catalog_backend.create_database(plan)
        } else {
            Err(ErrorCode::UnknownDatabaseEngine(format!(
                "unknown database engine {}, supported database engines: {}",
                plan.engine,
                self.db_engine_registry.engine_names().join(",")
            )))
        }
    }

    fn drop_database(&self, plan: DropDatabasePlan) -> Result<()> {
        let name = plan.db.clone();
        self.catalog_backend.drop_database(plan)?;
        self.db_instances.write().remove(&name);
        Ok(())
    }

    fn get_db_engines(&self) -> Result<Vec<EngineDescription>> {
        let descriptions = self.db_engine_registry.descriptions();
        Ok(descriptions)
    }
}
