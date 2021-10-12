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
use std::sync::Arc;

use common_arrow::arrow::datatypes::Schema as ArrowSchema;
use common_arrow::arrow::io::parquet::write::WriteOptions;
use common_arrow::arrow::io::parquet::write::*;
use common_arrow::arrow::record_batch::RecordBatch;
use common_dal::DataAccessor;
use common_datablocks::DataBlock;
use common_exception::ErrorCode;
use common_exception::Result;
use futures::StreamExt;
use rusoto_core::ByteStream;

use crate::datasources::table::fuse::util;
use crate::datasources::table::fuse::SegmentInfo;
use crate::datasources::table::fuse::Stats;

pub type BlockStream =
    std::pin::Pin<Box<dyn futures::stream::Stream<Item = DataBlock> + Sync + Send + 'static>>;

/// dummy struct, namespace placeholder
pub struct BlockAppender;

impl BlockAppender {
    pub async fn append_blocks(
        data_accessor: Arc<dyn DataAccessor>,
        mut stream: BlockStream,
    ) -> Result<SegmentInfo> {
        let mut stats_acc = util::StatisticsAccumulator::new();
        let mut block_meta_acc = util::BlockMetaAccumulator::new();

        // accumulates the stats and save the blocks
        while let Some(block) = stream.next().await {
            stats_acc.acc(&block)?;
            let schema = block.schema().to_arrow();
            let location = util::gen_unique_block_location();
            let file_size = Self::save_block(&schema, block, &data_accessor, &location).await?;
            block_meta_acc.acc(file_size, location, &mut stats_acc);
        }

        // summary and gives back a segment_info
        // we need to send back a stream of segment latter
        let block_metas = block_meta_acc.blocks_metas;
        let summary = util::column_stats_reduce(stats_acc.blocks_stats)?;
        let segment_info = SegmentInfo {
            blocks: block_metas,
            summary: Stats {
                row_count: stats_acc.summary_row_count,
                block_count: stats_acc.summary_block_count,
                uncompressed_byte_size: stats_acc.in_memory_size,
                compressed_byte_size: stats_acc.file_size,
                col_stats: summary,
            },
        };
        Ok(segment_info)
    }

    async fn save_block(
        arrow_schema: &ArrowSchema,
        block: DataBlock,
        data_accessor: impl AsRef<dyn DataAccessor>,
        location: &str,
    ) -> Result<u64> {
        let data_accessor = data_accessor.as_ref();
        let options = WriteOptions {
            write_statistics: true,
            compression: Compression::Lz4, // let's begin with lz4
            version: Version::V2,
        };
        let batch = RecordBatch::try_from(block)?;
        let encodings: Vec<_> = arrow_schema
            .fields()
            .iter()
            .map(|f| util::col_encoding(&f.data_type))
            .collect();

        let iter = vec![Ok(batch)];
        let row_groups =
            RowGroupIterator::try_new(iter.into_iter(), arrow_schema, options, encodings)?;
        let parquet_schema = row_groups.parquet_schema().clone();

        // PutObject in S3 need to know the content-length in advance
        // multipart upload may intimidate this, but let's fit things together first
        // see issue #xxx

        use bytes::BufMut;
        // we need a configuration of block size threshold here
        let mut writer = Vec::with_capacity(10 * 1024 * 1024).writer();

        let len = common_arrow::parquet::write::write_file(
            &mut writer,
            row_groups,
            parquet_schema,
            options,
            None,
            None,
        )
        .map_err(|e| ErrorCode::ParquetError(e.to_string()))?;

        let parquet = writer.into_inner();
        let stream_len = parquet.len();
        let stream = ByteStream::from(parquet);
        data_accessor
            .put_stream(location, Box::new(stream), stream_len)
            .await?;

        Ok(len)
    }
}
