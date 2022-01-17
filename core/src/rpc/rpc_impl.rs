use jsonrpc_core::Result;
use mmb_rpc::rest_api::server_side_error;
use mmb_rpc::rest_api::MmbRpc;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use std::sync::Arc;

use crate::statistic_service::StatisticService;
use mmb_rpc::rest_api::ErrorCode;

use super::common::send_stop;
use super::common::set_config;

pub struct RpcImpl {
    server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    statistics: Arc<StatisticService>,
    engine_settings: String,
}

impl RpcImpl {
    pub fn new(
        server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
        statistics: Arc<StatisticService>,
        engine_settings: String,
    ) -> Self {
        Self {
            server_stopper_tx,
            statistics,
            engine_settings,
        }
    }
}

impl MmbRpc for RpcImpl {
    fn health(&self) -> Result<String> {
        Ok("Engine is working".into())
    }

    fn stop(&self) -> Result<String> {
        send_stop(self.server_stopper_tx.clone())
    }

    fn get_config(&self) -> Result<String> {
        Ok(self.engine_settings.clone())
    }

    fn set_config(&self, settings: String) -> Result<String> {
        set_config(settings)?;
        send_stop(self.server_stopper_tx.clone())?; // TODO: need restart here #337
        Ok("Config was successfully updated. Trading engine will stopped".into())
    }

    fn stats(&self) -> Result<String> {
        let json_statistic = serde_json::to_string(&self.statistics.statistic_service_state)
            .map_err(|err| {
                log::warn!(
                    "Failed to convert {:?} to string: {}",
                    self.statistics,
                    err.to_string()
                );
                server_side_error(ErrorCode::FailedToSaveNewConfig)
            })?;

        Ok(json_statistic)
    }
}
