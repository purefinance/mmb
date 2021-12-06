use jsonrpc_core::Result;
use jsonrpc_derive::rpc;

/// Rpc trait
#[rpc]
pub trait RestApiRpc {
    #[rpc(name = "health")]
    fn health(&self) -> Result<String>;

    #[rpc(name = "stop")]
    fn stop(&self) -> Result<()>;

    #[rpc(name = "get_config")]
    fn get_config(&self) -> Result<String>;

    #[rpc(name = "set_config")]
    fn set_config(&self, data: String) -> Result<()>;

    #[rpc(name = "stats")]
    fn stats(&self) -> Result<String>;
}
