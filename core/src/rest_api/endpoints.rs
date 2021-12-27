use jsonrpc_core::Result;
use mmb_rpc::rest_api::server_side_error;
use mmb_rpc::rest_api::MmbRpc;
use parking_lot::Mutex;
use serde::Deserialize;

use std::sync::mpsc;
use std::sync::Arc;

use crate::core::config::save_settings;
use crate::core::config::CONFIG_PATH;
use crate::core::config::CREDENTIALS_PATH;
use crate::core::statistic_service::StatisticService;
use crate::rest_api::control_panel::FAILED_TO_SEND_STOP_NOTIFICATION;
use mmb_rpc::rest_api::ErrorCode;

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

    fn send_stop(&self) -> Result<String> {
        match self.server_stopper_tx.lock().take() {
            Some(sender) => {
                if let Err(error) = sender.send(()) {
                    log::error!("{}: {:?}", FAILED_TO_SEND_STOP_NOTIFICATION, error);
                    return Err(server_side_error(ErrorCode::UnableToSendSignal));
                };
                Ok("ControlPanel is going to turn off".into())
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

    fn set_config(&self, params: String) -> Result<String> {
        #[derive(Deserialize)]
        struct Data {
            settings: String,
        }

        let data: Data = serde_json::from_str(params.as_str()).map_err(|err| {
            log::warn!(
                "Error while trying parse new config('{}') in set_config endpoint: {}",
                params,
                err.to_string()
            );
            server_side_error(ErrorCode::UnableToParseNewConfig)
        })?;

        save_settings(data.settings.as_str(), CONFIG_PATH, CREDENTIALS_PATH).map_err(|err| {
            log::warn!(
                "Error while trying to save new config in set_config endpoint: {}",
                err.to_string()
            );
            server_side_error(ErrorCode::FailedToSaveNewConfig)
        })?;

        self.send_stop()?;

        Ok("Config was successfully updated. Trading engine stopped".into())
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
