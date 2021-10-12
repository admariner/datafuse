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

use common_base::tokio;
use common_datavalues::prelude::*;
use common_exception::Result;
use common_planners::*;
use futures::TryStreamExt;

use super::NumbersTable;

#[tokio::test]
async fn test_number_table() -> Result<()> {
    let tbl_args = Some(vec![Expression::create_literal(DataValue::UInt64(Some(8)))]);
    let ctx = crate::tests::try_create_context()?;
    let table = NumbersTable::create("system", "numbers_mt", 1, tbl_args)?;

    let scan = &ScanPlan {
        schema_name: "scan_test".to_string(),
        table_id: 0,
        table_version: None,
        table_schema: DataSchemaRefExt::create(vec![]),
        table_args: Some(Expression::create_literal(DataValue::UInt64(Some(8)))),
        projected_schema: DataSchemaRefExt::create(vec![DataField::new(
            "number",
            DataType::UInt64,
            false,
        )]),
        push_downs: Extras::default(),
    };
    let partitions = ctx.get_settings().get_max_threads()? as usize;
    let io_ctx = ctx.get_single_node_table_io_context()?;
    let io_ctx = Arc::new(io_ctx);
    let source_plan = table.read_plan(
        io_ctx.clone(),
        Some(scan.push_downs.clone()),
        Some(partitions),
    )?;
    ctx.try_set_partitions(source_plan.parts.clone())?;

    let stream = table.read(io_ctx, &source_plan.push_downs).await?;
    let result = stream.try_collect::<Vec<_>>().await?;
    let block = &result[0];
    assert_eq!(block.num_columns(), 1);

    let expected = vec![
        "+--------+",
        "| number |",
        "+--------+",
        "| 0      |",
        "| 1      |",
        "| 2      |",
        "| 3      |",
        "| 4      |",
        "| 5      |",
        "| 6      |",
        "| 7      |",
        "+--------+",
    ];
    common_datablocks::assert_blocks_sorted_eq(expected, result.as_slice());

    Ok(())
}
