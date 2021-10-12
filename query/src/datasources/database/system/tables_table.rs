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

use std::any::Any;
use std::sync::Arc;

use common_context::IOContext;
use common_context::TableIOContext;
use common_datablocks::DataBlock;
use common_datavalues::prelude::*;
use common_exception::Result;
use common_meta_types::TableInfo;
use common_planners::Extras;
use common_planners::Part;
use common_planners::ReadDataSourcePlan;
use common_planners::Statistics;
use common_streams::DataBlockStream;
use common_streams::SendableDataBlockStream;

use crate::catalogs::Catalog;
use crate::catalogs::Table;
use crate::sessions::DatabendQueryContext;

pub struct TablesTable {
    table_info: TableInfo,
}

impl TablesTable {
    pub fn create(table_id: u64) -> Self {
        let schema = DataSchemaRefExt::create(vec![
            DataField::new("database", DataType::String, false),
            DataField::new("name", DataType::String, false),
            DataField::new("engine", DataType::String, false),
        ]);

        let table_info = TableInfo {
            db: "system".to_string(),
            name: "tables".to_string(),
            table_id,
            schema,
            engine: "SystemTables".to_string(),

            ..Default::default()
        };

        TablesTable { table_info }
    }
}

#[async_trait::async_trait]
impl Table for TablesTable {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_table_info(&self) -> &TableInfo {
        &self.table_info
    }

    fn read_plan(
        &self,
        _io_ctx: Arc<TableIOContext>,
        _push_downs: Option<Extras>,
        _partition_num_hint: Option<usize>,
    ) -> Result<ReadDataSourcePlan> {
        Ok(ReadDataSourcePlan {
            table_info: self.table_info.clone(),
            parts: vec![Part {
                name: "".to_string(),
                version: 0,
            }],
            statistics: Statistics::default(),
            description: "(Read from system.functions table)".to_string(),
            scan_plan: Default::default(), // scan_plan will be removed form ReadSourcePlan soon
            tbl_args: None,
            push_downs: None,
        })
    }

    async fn read(
        &self,
        io_ctx: Arc<TableIOContext>,
        _push_downs: &Option<Extras>,
    ) -> Result<SendableDataBlockStream> {
        let ctx: Arc<DatabendQueryContext> = io_ctx
            .get_user_data()?
            .expect("DatabendQueryContext should not be None");

        let databases = ctx.get_catalog().get_databases()?;

        let mut database_tables = vec![];
        for database in databases {
            for table in database.get_tables()? {
                let name = database.name().to_string();
                database_tables.push((name, table));
            }
        }

        let databases: Vec<&[u8]> = database_tables.iter().map(|(d, _)| d.as_bytes()).collect();
        let names: Vec<&[u8]> = database_tables
            .iter()
            .map(|(_, v)| v.raw().name().as_bytes())
            .collect();
        let engines: Vec<&[u8]> = database_tables
            .iter()
            .map(|(_, v)| v.raw().engine().as_bytes())
            .collect();

        let block = DataBlock::create_by_array(self.table_info.schema.clone(), vec![
            Series::new(databases),
            Series::new(names),
            Series::new(engines),
        ]);

        Ok(Box::pin(DataBlockStream::create(
            self.table_info.schema.clone(),
            None,
            vec![block],
        )))
    }
}
