use jsonrpc_core::Params;
use jsonrpc_core::Result;
use jsonrpc_core::Value;
use parking_lot::Mutex;
use serde::Deserialize;
use shared::rest_api::server_side_error;
use shared::rest_api::Rpc;

use std::sync::{mpsc::Sender, Arc};

use crate::core::{
    config::save_settings, config::CONFIG_PATH, config::CREDENTIALS_PATH,
    lifecycle::application_manager::ApplicationManager, statistic_service::StatisticService,
};
use shared::rest_api::ErrorCode;

pub struct RpcImpl {
    application_manager: Arc<ApplicationManager>,
    statistics: Arc<StatisticService>,
    server_stopper_tx: Arc<Mutex<Option<Sender<()>>>>,
    engine_settings: String,
}

impl RpcImpl {
    pub fn new(
        application_manager: Arc<ApplicationManager>,
        statistics: Arc<StatisticService>,
        server_stopper_tx: Arc<Mutex<Option<Sender<()>>>>,
        engine_settings: String,
    ) -> Self {
        Self {
            application_manager,
            statistics,
            server_stopper_tx,
            engine_settings,
        }
    }
}

impl Rpc for RpcImpl {
    fn health(&self) -> Result<Value> {
        Ok("Bot is working".into())
    }

    fn stop(&self) -> Result<Value> {
        let error_message = "Unable to send signal to stop ipc server";
        let server_stopper_tx = self.server_stopper_tx.lock().take().ok_or_else(|| {
            let reason = "server_stopper_tx shouldn't be None";
            log::error!("{}: {}", error_message, reason);
            server_side_error(ErrorCode::UnableToSendSignal)
        })?;

        server_stopper_tx.send(()).map_err(|reason| {
            log::error!("{}: {}", error_message, reason);
            server_side_error(ErrorCode::UnableToSendSignal)
        })?;

        Ok(Value::String("ControlPanel turned off".into()))
    }

    fn get_config(&self) -> Result<Value> {
        Ok(Value::String(self.engine_settings.clone()))
    }

    fn set_config(&self, params: Params) -> Result<Value> {
        #[derive(Deserialize)]
        struct Data {
            settings: String,
        }

        let data: Data = params.parse()?;

        save_settings(data.settings.as_str(), CONFIG_PATH, CREDENTIALS_PATH).map_err(|err| {
            log::warn!(
                "Error while trying save new config in set_config endpoint: {}",
                err.to_string()
            );
            server_side_error(ErrorCode::FailedToSaveNewConfig)
        })?;

        self.application_manager
            .clone()
            .spawn_graceful_shutdown("Engine stopped cause config updating".to_owned());

        Ok("Config was successfully updated. Trading engine stopped".into())
    }

    fn stats(&self) -> Result<Value> {
        let json_statistic = serde_json::to_string(&self.statistics.statistic_service_state)
            .map_err(|err| {
                log::warn!(
                    "Failed to convert {:?} to string: {}",
                    self.statistics,
                    err.to_string()
                );
                server_side_error(ErrorCode::FailedToSaveNewConfig)
            })?;

        Ok(Value::String(json_statistic))
    }
}
