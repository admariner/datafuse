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

use common_context::IOContext;
use common_context::TableIOContext;
use common_exception::ErrorCode;
use common_exception::Result;
use common_planners::Extras;
use common_streams::ProgressStream;
use common_streams::SendableDataBlockStream;
use tokio_stream::wrappers::ReceiverStream;

use super::io;
use crate::datasources::table::fuse::FuseTable;
use crate::sessions::DatabendQueryContext;

impl FuseTable {
    #[inline]
    pub async fn do_read(
        &self,
        io_ctx: Arc<TableIOContext>,
        push_downs: &Option<Extras>,
    ) -> Result<SendableDataBlockStream> {
        let ctx: Arc<DatabendQueryContext> = io_ctx
            .get_user_data()?
            .expect("DatabendQueryContext should not be None");

        let default_proj = || {
            (0..self.table_info.schema.fields().len())
                .into_iter()
                .collect::<Vec<usize>>()
        };

        let projection = if let Some(push_down) = push_downs {
            if let Some(prj) = &push_down.projection {
                prj.clone()
            } else {
                default_proj()
            }
        } else {
            default_proj()
        };

        let (tx, rx) = common_base::tokio::sync::mpsc::channel(1024);

        // TODO we need a configuration to specify the unit of dequeue operation
        let bite_size = 1;
        let mut iter = {
            let ctx = ctx.clone();
            std::iter::from_fn(move || match ctx.clone().try_get_partitions(bite_size) {
                Err(_) => None,
                Ok(parts) if parts.is_empty() => None,
                Ok(parts) => Some(parts),
            })
            .flatten()
        };
        let da = io_ctx.get_data_accessor()?;
        let arrow_schema = self.table_info.schema.to_arrow();

        // BlockReader::read_part is !Send (since parquet2::read::page_stream::get_page_stream is !Send)
        // we have to use spawn_local here
        let _h = common_base::tokio::task::spawn_local(async move {
            for part in &mut iter {
                io::BlockReader::read_part(
                    part,
                    da.clone(),
                    projection.clone(),
                    tx.clone(),
                    &arrow_schema,
                )
                .await?;
            }
            Ok::<(), ErrorCode>(())
        });

        let progress_callback = ctx.progress_callback()?;
        let receiver = ReceiverStream::new(rx);
        let stream = ProgressStream::try_create(Box::pin(receiver), progress_callback)?;
        Ok(Box::pin(stream))
    }
}
