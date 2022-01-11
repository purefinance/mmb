use jsonrpc_core::Result;
use mmb_rpc::rest_api::server_side_error;
use mmb_rpc::rest_api::MmbRpc;
use mmb_utils::send_expected::SendExpectedByRef;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use std::sync::Arc;

use crate::core::config::save_settings;
use crate::core::config::CONFIG_PATH;
use crate::core::config::CREDENTIALS_PATH;
use crate::core::statistic_service::StatisticService;
use crate::rpc::control_panel::FAILED_TO_SEND_STOP_NOTIFICATION;
use mmb_rpc::rest_api::ErrorCode;

pub struct RpcImpl {
    server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    statistics: Arc<StatisticService>,
    engine_settings: String,
    wait_config_tx: Option<mpsc::Sender<()>>,
}

impl RpcImpl {
    pub fn new(
        server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
        statistics: Arc<StatisticService>,
        engine_settings: String,
        wait_config_tx: Option<mpsc::Sender<()>>,
    ) -> Self {
        Self {
            server_stopper_tx,
            statistics,
            engine_settings,
            wait_config_tx,
        }
    }

    fn send_stop(&self) -> Result<String> {
        match self.server_stopper_tx.lock().take() {
            Some(sender) => {
                if let Err(error) = sender.try_send(()) {
                    log::error!("{}: {:?}", FAILED_TO_SEND_STOP_NOTIFICATION, error);
                    return Err(server_side_error(ErrorCode::UnableToSendSignal));
                };
                let msg = "Trading engine is going to turn off";
                log::info!("{} by control panel", msg);
                Ok(msg.into())
            }
            None => {
                log::warn!(
                    "{}: the signal is already sent",
                    FAILED_TO_SEND_STOP_NOTIFICATION
                );
                Err(server_side_error(ErrorCode::StopperIsNone))
            }
        }
    }
}

impl MmbRpc for RpcImpl {
    fn health(&self) -> Result<String> {
        Ok("Engine is working".into())
    }

    fn stop(&self) -> Result<String> {
        self.send_stop()
    }

    fn get_config(&self) -> Result<String> {
        Ok(self.engine_settings.clone())
    }

    fn set_config(&self, settings: String) -> Result<String> {
        save_settings(settings.as_str(), CONFIG_PATH, CREDENTIALS_PATH).map_err(|err| {
            log::warn!(
                "Error while trying to save new config in set_config endpoint: {}",
                err.to_string()
            );
            server_side_error(ErrorCode::FailedToSaveNewConfig)
        })?;

        let answer = match &self.wait_config_tx {
            Some(wait_config_tx) => {
                wait_config_tx.send_expected(());
                "Config was successfully set. Trading engine will be launched".into()
            }
            None => {
                self.send_stop()?;
                "Config was successfully updated. Trading engine will be stopped".into()
            } // TODO: need restart here #337
        };

        Ok(answer)
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
