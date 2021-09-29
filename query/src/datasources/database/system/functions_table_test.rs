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

use common_base::tokio;
use common_exception::Result;
use futures::TryStreamExt;

use crate::catalogs::Table;
use crate::datasources::database::system::FunctionsTable;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_functions_table() -> Result<()> {
    let ctx = crate::tests::try_create_context()?;
    let table = FunctionsTable::create(1, "system");
    let source_plan = table.read_plan(
        ctx.clone(),
        None,
        Some(ctx.get_settings().get_max_threads()? as usize),
    )?;

    let stream = table.read(ctx, &source_plan).await?;
    let result = stream.try_collect::<Vec<_>>().await?;
    let block = &result[0];
    assert_eq!(block.num_columns(), 2);
    Ok(())
}
