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

use common_exception::Result;
use common_meta_api::KVApi;
use common_meta_api::MetaApi;
use common_meta_flight::MetaFlightClient;
use common_meta_flight::MetaFlightClientConf;

// Since there is a pending dependency issue,
// StoreApiProvider is temporarily moved from store-api-sdk
//
// @see https://github.com/datafuselabs/databend/issues/1929

#[derive(Clone)]
pub struct MetaClientProvider {
    // do not depend on query::configs::Config in case of moving back to sdk
    // also @see config_converter.rs
    conf: MetaFlightClientConf,
}

impl MetaClientProvider {
    pub fn new(conf: impl Into<MetaFlightClientConf>) -> Self {
        MetaClientProvider { conf: conf.into() }
    }

    /// Get meta async client, trait is defined in MetaApi.
    pub async fn try_get_meta_client(&self) -> Result<Arc<dyn MetaApi>> {
        let client = MetaFlightClient::try_new(&self.conf).await?;
        Ok(Arc::new(client))
    }

    /// Get kv async client, operations trait defined in KVApi.
    pub async fn try_get_kv_client(&self) -> Result<Arc<dyn KVApi>> {
        let local = self.conf.kv_service_config.address.is_empty();
        if local {
            let client = common_meta_local_store::KV::new_temp().await?;
            Ok(Arc::new(client))
        } else {
            let client = MetaFlightClient::try_new(&self.conf).await?;
            Ok(Arc::new(client))
        }
    }
}
