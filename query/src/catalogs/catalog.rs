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

use std::sync::Arc;

use common_exception::Result;
use common_meta_types::CreateDatabaseReply;
use common_meta_types::MetaId;
use common_meta_types::MetaVersion;
use common_planners::CreateDatabasePlan;
use common_planners::DropDatabasePlan;

use crate::catalogs::Database;
use crate::catalogs::TableFunctionMeta;
use crate::catalogs::TableMeta;
use crate::datasources::database_engine::DatabaseEngine;
use crate::datasources::database_engine_registry::EngineDescription;
use crate::datasources::table_func_engine::TableArgs;

/// Catalog is the global view of all the databases of the user.
/// The global view has many engine type: Local-Database(engine=Local), Remote-Database(engine=Remote)
/// or others(like MySQL-Database, engine=MySQL)
/// When we create a new database, we first to get the engine from the registered engines,
/// and use the engine to create them.
pub trait Catalog {
    fn register_db_engine(
        &self,
        engine_type: &str,
        database_engine: Arc<dyn DatabaseEngine>,
    ) -> Result<()>;

    // Get all the databases.
    fn get_databases(&self) -> Result<Vec<Arc<dyn Database>>>;

    // Get the database by name.
    fn get_database(&self, db_name: &str) -> Result<Arc<dyn Database>>;

    // Get one table by db and table name.
    fn get_table(&self, db_name: &str, table_name: &str) -> Result<Arc<TableMeta>>;

    fn get_table_by_id(
        &self,
        table_id: MetaId,
        table_version: Option<MetaVersion>,
    ) -> Result<Arc<TableMeta>>;

    // Get function by name.
    fn get_table_function(
        &self,
        _func_name: &str,
        _tbl_args: TableArgs,
    ) -> Result<Arc<TableFunctionMeta>> {
        unimplemented!()
    }

    // Operation with database.
    fn create_database(&self, plan: CreateDatabasePlan) -> Result<CreateDatabaseReply>;

    fn drop_database(&self, plan: DropDatabasePlan) -> Result<()>;

    // Get all db engines.
    fn get_db_engines(&self) -> Result<Vec<EngineDescription>>;
}
