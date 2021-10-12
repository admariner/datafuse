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

//! This crate defines data types used in meta data storage service.

use std::fmt::Debug;

pub type MetaVersion = u64;
pub type MetaId = u64;

/// An operation that updates a field, delete it, or leave it as is.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum Operation<T> {
    Update(T),
    Delete,
    AsIs,
}

impl<T> From<Option<T>> for Operation<T>
where for<'x> T: serde::Serialize + serde::Deserialize<'x> + Debug + Clone
{
    fn from(v: Option<T>) -> Self {
        match v {
            None => Operation::Delete,
            Some(x) => Operation::Update(x),
        }
    }
}
