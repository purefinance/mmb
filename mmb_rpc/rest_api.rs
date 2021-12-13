use jsonrpc_core::{Error, Params, Result, Value};
use jsonrpc_derive::rpc;

pub static IPC_ADDRESS: &str = "/tmp/mmb.ipc";

/// Rpc trait
#[rpc]
pub trait MmbRpc {
    #[rpc(name = "health")]
    fn health(&self) -> Result<Value>;

    #[rpc(name = "stop")]
    fn stop(&self) -> Result<Value>;

    #[rpc(name = "get_config")]
    fn get_config(&self) -> Result<Value>;

    #[rpc(name = "set_config")]
    fn set_config(&self, params: Params) -> Result<Value>;

    #[rpc(name = "stats")]
    fn stats(&self) -> Result<Value>;
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
