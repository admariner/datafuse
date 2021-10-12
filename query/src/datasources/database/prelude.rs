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

use common_exception::Result;

use crate::catalogs::backends::CatalogBackend;
use crate::datasources::database::default::default_database_factory::DefaultDatabaseFactory;
use crate::datasources::database_engine_registry::DatabaseEngineRegistry;
use crate::datasources::table_engine_registry::TableEngineRegistry;

pub const DB_ENGINE_DEFAULT: &str = "Default";

pub fn register_prelude_db_engines(
    registry: &DatabaseEngineRegistry,
    catalog_backend: Arc<dyn CatalogBackend>,
    table_factory_registry: Arc<TableEngineRegistry>,
) -> Result<()> {
    let default = DefaultDatabaseFactory::new(catalog_backend, table_factory_registry);
    registry.register(DB_ENGINE_DEFAULT, Arc::new(default))?;
    Ok(())
}
