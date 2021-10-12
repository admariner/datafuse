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
use std::sync::Mutex;

use common_exception::ErrorCode;
use common_exception::Result;
use tokio::time::sleep_until;
use tokio::time::Duration;
use tokio::time::Instant;

use crate::runtime::BlockingWait;
use crate::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_runtime() -> Result<()> {
    let counter = Arc::new(Mutex::new(0));

    let runtime = Runtime::with_default_worker_threads()?;
    let runtime_counter = Arc::clone(&counter);
    let runtime_header = runtime.spawn(async move {
        let rt1 = Runtime::with_default_worker_threads().unwrap();
        let rt1_counter = Arc::clone(&runtime_counter);
        let rt1_header = rt1.spawn(async move {
            let rt2 = Runtime::with_worker_threads(1).unwrap();
            let rt2_counter = Arc::clone(&rt1_counter);
            let rt2_header = rt2.spawn(async move {
                let rt3 = Runtime::with_default_worker_threads().unwrap();
                let rt3_counter = Arc::clone(&rt2_counter);
                let rt3_header = rt3.spawn(async move {
                    let mut num = rt3_counter.lock().unwrap();
                    *num += 1;
                });
                rt3_header.await.unwrap();

                let mut num = rt2_counter.lock().unwrap();
                *num += 1;
            });
            rt2_header.await.unwrap();

            let mut num = rt1_counter.lock().unwrap();
            *num += 1;
        });
        rt1_header.await.unwrap();

        let mut num = runtime_counter.lock().unwrap();
        *num += 1;
    });
    runtime_header.await.unwrap();

    let result = *counter.lock().unwrap();
    assert_eq!(result, 4);

    Ok(())
}

#[test]
fn test_block_on() -> Result<()> {
    async fn five() -> Result<u8> {
        Ok(5)
    }

    // Ok.
    {
        let rt = Runtime::with_default_worker_threads().unwrap();
        let r = rt.block_on(five(), None)??;
        assert_eq!(r, 5);
    }

    // Ok.
    {
        let rt = Runtime::with_default_worker_threads().unwrap();
        let r = rt.block_on(five(), Some(Duration::from_secs(10)))??;
        assert_eq!(r, 5);
    }

    // Timeout error.
    {
        async fn sleep() -> Result<()> {
            let deadline = Instant::now() + Duration::from_millis(10_000);
            sleep_until(deadline).await;
            Ok(())
        }

        let rt = Runtime::with_default_worker_threads().unwrap();
        let r = rt.block_on(sleep(), Some(Duration::from_millis(500)));

        assert!(r.is_err());
        if let Err(e) = r {
            let expect = "Code: 40, displayText = timed out waiting on channel.";
            let actual = format!("{:}", e);
            assert_eq!(expect, actual);
        }
    }

    Ok(())
}

#[test]
fn test_blocking_wait() -> Result<()> {
    async fn five() -> Result<u8> {
        Ok(5)
    }

    let res = five().wait(None)?;
    assert!(res.is_ok());
    assert_eq!(5, res.unwrap());

    let rt = Runtime::with_default_worker_threads().unwrap();

    let res = five().wait_in(&rt, None)?;
    assert!(res.is_ok());
    assert_eq!(5, res.unwrap());

    Ok(())
}

#[test]
fn test_blocking_wait_timeout() -> Result<()> {
    async fn sleep_5_sec() -> Result<()> {
        tokio::time::sleep(Duration::from_millis(5000)).await;
        Ok(())
    }

    let res = sleep_5_sec().wait(Some(Duration::from_millis(1000)));
    assert!(res.is_err());
    assert_eq!(ErrorCode::Timeout("").code(), res.unwrap_err().code());

    let rt = Runtime::with_default_worker_threads().unwrap();

    let res = sleep_5_sec().wait_in(&rt, Some(Duration::from_millis(1000)));
    assert!(res.is_err());
    assert_eq!(ErrorCode::Timeout("").code(), res.unwrap_err().code());

    Ok(())
}

#[test]
fn test_blocking_wait_no_timeout() -> Result<()> {
    async fn sleep_1_sec() -> Result<()> {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        Ok(())
    }

    let res = sleep_1_sec().wait(Some(Duration::from_millis(5000)))?;
    assert!(res.is_ok());

    let rt = Runtime::with_default_worker_threads().unwrap();

    let res = sleep_1_sec().wait_in(&rt, Some(Duration::from_millis(5000)))?;
    assert!(res.is_ok());

    Ok(())
}
