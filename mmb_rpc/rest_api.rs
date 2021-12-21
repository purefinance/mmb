use jsonrpc_core::{Error, Result};
use jsonrpc_derive::rpc;

#[cfg(unix)]
pub static IPC_ADDRESS: &str = "/tmp/mmb_core.ipc";
#[cfg(windows)]
pub static IPC_ADDRESS: &str = r#"\\.\pipe\mmb_core"#;

#[rpc]
pub trait MmbRpc {
    #[rpc(name = "health")]
    fn health(&self) -> Result<String>;

    #[rpc(name = "stop")]
    fn stop(&self) -> Result<String>;

    #[rpc(name = "get_config")]
    fn get_config(&self) -> Result<String>;

    #[rpc(name = "set_config")]
    fn set_config(&self, params: String) -> Result<String>;

    #[rpc(name = "stats")]
    fn stats(&self) -> Result<String>;
}

pub enum ErrorCode {
    StopperIsNone = 1,
    UnableToSendSignal = 2,
    FailedToSaveNewConfig = 3,
}

pub fn server_side_error(code: ErrorCode) -> Error {
    let reason = match code {
        ErrorCode::StopperIsNone => "Server stopper is none",
        ErrorCode::UnableToSendSignal => "Unable to send signal",
        ErrorCode::FailedToSaveNewConfig => "Failed to save new config",
    };
    log::error!("Rest API error: {}", reason);
    Error::new(jsonrpc_core::ErrorCode::ServerError(code as i64))
}
