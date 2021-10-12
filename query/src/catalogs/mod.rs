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
//

mod catalog;
mod database;
mod table;
mod table_function;
mod table_id_ranges;
mod table_meta;
mod table_metas;

pub mod backends;
pub mod impls;

pub use catalog::Catalog;
pub use database::Database;
pub use table::Table;
pub use table::TablePtr;
pub use table_function::TableFunction;
pub use table_id_ranges::*;
pub use table_meta::Meta;
pub use table_meta::TableFunctionMeta;
pub use table_meta::TableMeta;
pub use table_metas::InMemoryMetas;

pub use crate::datasources::database_engine::DatabaseEngine;
