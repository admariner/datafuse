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

pub trait CatalogBackend: Send + Sync {
    // database

    fn create_database(&self, plan: CreateDatabasePlan) -> Result<CreateDatabaseReply>;

    fn drop_database(&self, plan: DropDatabasePlan) -> Result<()>;

    fn get_database(&self, db_name: &str) -> Result<Arc<DatabaseInfo>>;

    fn get_databases(&self) -> Result<Vec<Arc<DatabaseInfo>>>;

    // table

    fn create_table(&self, plan: CreateTablePlan) -> Result<CreateTableReply>;

    fn drop_table(&self, plan: DropTablePlan) -> Result<()>;

    fn get_table(&self, db_name: &str, table_name: &str) -> Result<Arc<TableInfo>>;

    fn get_tables(&self, db_name: &str) -> Result<Vec<Arc<TableInfo>>>;

    fn get_table_by_id(
        &self,
        table_id: MetaId,
        table_version: Option<MetaVersion>,
    ) -> Result<Arc<TableInfo>>;

    fn name(&self) -> String;
}
