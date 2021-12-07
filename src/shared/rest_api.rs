use jsonrpc_core::{Error, Params, Result, Value};
use jsonrpc_derive::rpc;

pub static IPC_ADDRESS: &str = "/tmp/mmb.ipc"; // TODO: get this path from .toml file

/// Rpc trait
#[rpc]
pub trait Rpc {
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
    Error::new(jsonrpc_core::ErrorCode::ServerError(code as i64))
}
