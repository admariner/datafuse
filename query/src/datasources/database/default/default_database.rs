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

use std::sync::Arc;

use common_exception::ErrorCode;
use common_infallible::RwLock;
use common_meta_types::MetaId;
use common_meta_types::MetaVersion;
use common_meta_types::TableInfo;
use common_planners::CreateTablePlan;
use common_planners::DropTablePlan;

use crate::catalogs::backends::CatalogBackend;
use crate::catalogs::Database;
use crate::catalogs::InMemoryMetas;
use crate::catalogs::TableMeta;
use crate::common::MetaClientProvider;
use crate::datasources::table_engine_registry::TableEngineRegistry;

pub struct DefaultDatabase {
    db_name: String,
    engine_name: String,
    catalog_backend: Arc<dyn CatalogBackend>,
    table_factory_registry: Arc<TableEngineRegistry>,
    store_api_provider: MetaClientProvider,
    stateful_table_cache: RwLock<InMemoryMetas>,
}

impl DefaultDatabase {
    pub fn new(
        db_name: impl Into<String>,
        engine_name: impl Into<String>,
        catalog_backend: Arc<dyn CatalogBackend>,
        table_factory_registry: Arc<TableEngineRegistry>,
        store_api_provider: MetaClientProvider,
    ) -> Self {
        Self {
            db_name: db_name.into(),
            engine_name: engine_name.into(),
            catalog_backend,
            table_factory_registry,
            store_api_provider,
            stateful_table_cache: RwLock::new(InMemoryMetas::create()),
        }
    }

    fn build_table_instance(
        &self,
        table_info: &TableInfo,
    ) -> common_exception::Result<Arc<TableMeta>> {
        let engine = &table_info.engine;
        let provider = self
            .table_factory_registry
            .engine_provider(engine)
            .ok_or_else(|| {
                ErrorCode::UnknownTableEngine(format!("unknown table engine {}", engine))
            })?;
        let tbl = provider.try_create(table_info.clone(), self.store_api_provider.clone())?;
        let stateful = tbl.is_stateful();
        let tbl_meta = TableMeta::create(tbl.into(), table_info.table_id);
        if stateful {
            self.stateful_table_cache.write().insert(tbl_meta.clone());
        }

        Ok(Arc::new(tbl_meta))
    }
}

impl Database for DefaultDatabase {
    fn name(&self) -> &str {
        &self.db_name
    }

    fn engine(&self) -> &str {
        &self.engine_name
    }

    fn is_local(&self) -> bool {
        false
    }

    fn get_table(&self, table_name: &str) -> common_exception::Result<Arc<TableMeta>> {
        {
            if let Some(meta) = self.stateful_table_cache.read().get_by_name(table_name) {
                return Ok(meta);
            }
        }
        let db_name = self.name();
        let table_info = self.catalog_backend.get_table(db_name, table_name)?;
        self.build_table_instance(table_info.as_ref())
    }

    fn get_table_by_id(
        &self,
        table_id: MetaId,
        table_version: Option<MetaVersion>,
    ) -> common_exception::Result<Arc<TableMeta>> {
        {
            if let Some(tbl) = self.stateful_table_cache.read().get_by_id(&table_id) {
                return Ok(tbl.clone());
            }
        }

        let table_info = self
            .catalog_backend
            .get_table_by_id(table_id, table_version)?;

        self.build_table_instance(table_info.as_ref())
    }

    fn get_tables(&self) -> common_exception::Result<Vec<Arc<TableMeta>>> {
        let table_infos = self.catalog_backend.get_tables(self.name())?;
        table_infos.iter().try_fold(vec![], |mut acc, item| {
            let tbl = self.build_table_instance(item)?;
            acc.push(tbl);
            Ok(acc)
        })
    }

    fn create_table(&self, plan: CreateTablePlan) -> common_exception::Result<()> {
        // TODO validate table parameters by using TableFactory
        self.catalog_backend.create_table(plan)?;
        Ok(())
    }

    fn drop_table(&self, plan: DropTablePlan) -> common_exception::Result<()> {
        self.catalog_backend.drop_table(plan)
    }
}
