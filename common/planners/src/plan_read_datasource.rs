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

use common_datavalues::DataSchemaRef;
use common_meta_types::TableInfo;

use crate::Expression;
use crate::Extras;
use crate::Partitions;
use crate::ScanPlan;
use crate::Statistics;

// TODO: Delete the scan plan field, but it depends on plan_parser:L394
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ReadDataSourcePlan {
    pub table_info: TableInfo,

    pub parts: Partitions,
    pub statistics: Statistics,
    pub description: String,
    pub scan_plan: Arc<ScanPlan>,

    pub tbl_args: Option<Vec<Expression>>,
    pub push_downs: Option<Extras>,
}

impl ReadDataSourcePlan {
    pub fn schema(&self) -> DataSchemaRef {
        self.table_info.schema.clone()
    }
}
