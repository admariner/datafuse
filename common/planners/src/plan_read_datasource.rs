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

use common_datavalues::DataSchema;
use common_datavalues::DataSchemaRef;
use common_metatypes::MetaId;
use common_metatypes::MetaVersion;

use crate::Expression;
use crate::Extras;
use crate::Partitions;
use crate::ScanPlan;
use crate::Statistics;

// TODO: Delete the scan plan field, but it depends on plan_parser:L394
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ReadDataSourcePlan {
    // TODO encapsulate these 5 fields into TableInfo
    pub db: String,
    pub table: String,
    pub table_id: MetaId,
    pub table_version: Option<MetaVersion>,
    pub schema: DataSchemaRef,

    pub parts: Partitions,
    pub statistics: Statistics,
    pub description: String,
    pub scan_plan: Arc<ScanPlan>,
    pub remote: bool,

    pub tbl_args: Option<Vec<Expression>>,
    pub push_downs: Option<Extras>,
}

impl ReadDataSourcePlan {
    pub fn empty(table_id: u64, table_version: Option<u64>) -> ReadDataSourcePlan {
        ReadDataSourcePlan {
            db: "".to_string(),
            table: "".to_string(),
            table_id,
            table_version,
            schema: Arc::from(DataSchema::empty()),
            parts: vec![],
            statistics: Statistics::default(),
            description: "".to_string(),
            scan_plan: Arc::new(ScanPlan::with_table_id(table_id, table_version)),
            remote: false,
            tbl_args: None,
            push_downs: None,
        }
    }

    pub fn schema(&self) -> DataSchemaRef {
        self.schema.clone()
    }
}
