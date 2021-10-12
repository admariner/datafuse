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

use std::net::Shutdown;

use common_base::tokio::net::TcpStream;
use common_exception::exception::ABORT_SESSION;
use common_exception::ErrorCode;
use common_exception::Result;
use common_exception::ToErrorCode;
use msql_srv::MysqlIntermediary;

use crate::servers::mysql::mysql_interactive_worker::InteractiveWorker;
use crate::sessions::SessionRef;

pub struct MySQLConnection;

impl MySQLConnection {
    pub fn run_on_stream(session: SessionRef, stream: TcpStream) -> Result<()> {
        let blocking_stream = Self::convert_stream(stream)?;
        MySQLConnection::attach_session(&session, &blocking_stream)?;
        std::thread::spawn(move || {
            MySQLConnection::session_executor(session, blocking_stream);
        });

        Ok(())
    }

    fn session_executor(session: SessionRef, blocking_stream: std::net::TcpStream) {
        let client_addr = blocking_stream.peer_addr().unwrap().to_string();
        let interactive_worker = InteractiveWorker::create(session, client_addr);
        if let Err(error) = MysqlIntermediary::run_on_tcp(interactive_worker, blocking_stream) {
            if error.code() != ABORT_SESSION {
                log::error!(
                    "Unexpected error occurred during query execution: {:?}",
                    error
                );
            }
        };
    }

    fn attach_session(session: &SessionRef, blocking_stream: &std::net::TcpStream) -> Result<()> {
        let host = blocking_stream.peer_addr().ok();
        let blocking_stream_ref = blocking_stream.try_clone()?;
        session.attach(host, move || {
            if let Err(error) = blocking_stream_ref.shutdown(Shutdown::Both) {
                log::error!("Cannot shutdown MySQL session io {}", error);
            }
        });

        Ok(())
    }

    // TODO: move to ToBlockingStream trait
    fn convert_stream(stream: TcpStream) -> Result<std::net::TcpStream> {
        let stream = stream
            .into_std()
            .map_err_to_code(ErrorCode::TokioError, || {
                "Cannot to convert Tokio TcpStream to Std TcpStream"
            })?;
        stream
            .set_nonblocking(false)
            .map_err_to_code(ErrorCode::TokioError, || {
                "Cannot to convert Tokio TcpStream to Std TcpStream"
            })?;

        Ok(stream)
    }
}
