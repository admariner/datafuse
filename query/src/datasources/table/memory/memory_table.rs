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

use std::any::Any;
use std::sync::Arc;

use common_context::IOContext;
use common_context::TableIOContext;
use common_datablocks::DataBlock;
use common_exception::ErrorCode;
use common_exception::Result;
use common_infallible::RwLock;
use common_meta_types::TableInfo;
use common_planners::Extras;
use common_planners::InsertIntoPlan;
use common_planners::ReadDataSourcePlan;
use common_planners::Statistics;
use common_planners::TruncateTablePlan;
use common_streams::SendableDataBlockStream;
use futures::stream::StreamExt;

use crate::catalogs::Table;
use crate::datasources::common::generate_parts;
use crate::datasources::table::memory::memory_table_stream::MemoryTableStream;
use crate::sessions::DatabendQueryContext;

pub struct MemoryTable {
    table_info: TableInfo,
    blocks: Arc<RwLock<Vec<DataBlock>>>,
}

impl MemoryTable {
    pub fn try_create(table_info: TableInfo) -> Result<Box<dyn Table>> {
        let table = Self {
            table_info,
            blocks: Arc::new(RwLock::new(vec![])),
        };
        Ok(Box::new(table))
    }
}

#[async_trait::async_trait]
impl Table for MemoryTable {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn is_stateful(&self) -> bool {
        true
    }

    fn get_table_info(&self) -> &TableInfo {
        &self.table_info
    }

    fn read_plan(
        &self,
        io_ctx: Arc<TableIOContext>,
        push_downs: Option<Extras>,
        _partition_num_hint: Option<usize>,
    ) -> Result<ReadDataSourcePlan> {
        let blocks = self.blocks.read();
        let rows = blocks.iter().map(|block| block.num_rows()).sum();
        let bytes = blocks.iter().map(|block| block.memory_size()).sum();

        let table_info = &self.table_info;
        let db = &table_info.db;
        Ok(ReadDataSourcePlan {
            table_info: self.table_info.clone(),
            parts: generate_parts(0, io_ctx.get_max_threads() as u64, blocks.len() as u64),
            statistics: Statistics::new_exact(rows, bytes),
            description: format!("(Read from Memory Engine table  {}.{})", db, self.name()),
            scan_plan: Default::default(),
            tbl_args: None,
            push_downs,
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

        let blocks = self.blocks.read();
        Ok(Box::pin(MemoryTableStream::try_create(
            ctx,
            blocks.clone(),
        )?))
    }

    async fn append_data(
        &self,
        _io_ctx: Arc<TableIOContext>,
        _insert_plan: InsertIntoPlan,
    ) -> Result<()> {
        let mut s = {
            let mut inner = _insert_plan.input_stream.lock();
            (*inner).take()
        }
        .ok_or_else(|| ErrorCode::EmptyData("input stream consumed"))?;

        if _insert_plan.schema().as_ref().fields() != self.table_info.schema.as_ref().fields() {
            return Err(ErrorCode::BadArguments("DataBlock schema mismatch"));
        }

        while let Some(block) = s.next().await {
            let mut blocks = self.blocks.write();
            blocks.push(block);
        }
        Ok(())
    }

    async fn truncate(
        &self,
        _io_ctx: Arc<TableIOContext>,
        _truncate_plan: TruncateTablePlan,
    ) -> Result<()> {
        let mut blocks = self.blocks.write();
        blocks.clear();
        Ok(())
    }
}
