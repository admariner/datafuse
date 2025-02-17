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

use common_base::TrySpawn;
use common_catalog::TableSnapshot;
use common_dal::DataAccessor;
use common_dal::ObjectAccessor;
use common_exception::Result;

pub fn read_table_snapshot<S: TrySpawn>(
    da: Arc<dyn DataAccessor>,
    ctx: &S,
    loc: &str,
) -> Result<TableSnapshot> {
    ObjectAccessor::new(da).blocking_read_obj(ctx, loc)
}

#[allow(dead_code)]
pub async fn read_table_snapshot_async(
    da: Arc<dyn DataAccessor>,
    loc: &str,
) -> Result<TableSnapshot> {
    ObjectAccessor::new(da).read_obj(loc).await
}
