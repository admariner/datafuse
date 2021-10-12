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

// consider remove these, read_util seems to be enough (type could be inferred)
mod segment_reader;
// end

mod block_appender;
#[cfg(test)]
mod block_appender_test;
mod block_reader;

pub(crate) use block_appender::*;
pub use block_reader::*;
pub use segment_reader::*;
