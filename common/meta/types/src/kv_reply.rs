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

use crate::KVValue;
use crate::SeqValue;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct UpsertKVActionReply {
    /// prev is the value before upsert.
    pub prev: Option<SeqValue<KVValue>>,
    /// result is the value after upsert.
    pub result: Option<SeqValue<KVValue>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct GetKVActionReply {
    pub result: Option<SeqValue<KVValue>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MGetKVActionReply {
    pub result: Vec<Option<SeqValue<KVValue>>>,
}

pub type PrefixListReply = Vec<(String, SeqValue<KVValue>)>;
