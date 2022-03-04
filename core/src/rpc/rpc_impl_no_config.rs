use jsonrpc_core::Result;
use mmb_rpc::rest_api::MmbRpc;
use mmb_utils::send_expected::SendExpectedByRef;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use std::sync::Arc;

use crate::lifecycle::app_lifetime_manager::ActionAfterGracefulShutdown;

use super::common::send_stop;
use super::common::set_config;

static CONFIG_IS_NOT_SET: &str = "Config isn't set";

pub struct RpcImplNoConfig {
    server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<ActionAfterGracefulShutdown>>>>,
    wait_config_tx: mpsc::Sender<()>,
}

impl RpcImplNoConfig {
    pub fn new(
        server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<ActionAfterGracefulShutdown>>>>,
        wait_config_tx: mpsc::Sender<()>,
    ) -> Self {
        Self {
            server_stopper_tx,
            wait_config_tx,
        }
    }
}

impl MmbRpc for RpcImplNoConfig {
    fn health(&self) -> Result<String> {
        Ok(CONFIG_IS_NOT_SET.into())
    }

    fn stop(&self) -> Result<String> {
        send_stop(self.server_stopper_tx.clone())
    }

    fn get_config(&self) -> Result<String> {
        Ok(CONFIG_IS_NOT_SET.into())
    }

    fn set_config(&self, settings: String) -> Result<String> {
        set_config(settings)?;
        self.wait_config_tx.send_expected(());
        Ok("Config was successfully set. Trading engine will be launched".into())
    }

    fn stats(&self) -> Result<String> {
        Ok(CONFIG_IS_NOT_SET.into())
    }
}
