use jsonrpc_core::Params;
use jsonrpc_core::Result;
use jsonrpc_core::Value;
use mmb_rpc::rest_api::server_side_error;
use mmb_rpc::rest_api::MmbRpc;
use serde::Deserialize;

use std::sync::Arc;

use crate::core::{
    config::save_settings, config::CONFIG_PATH, config::CREDENTIALS_PATH,
    lifecycle::application_manager::ApplicationManager, statistic_service::StatisticService,
};
use mmb_rpc::rest_api::ErrorCode;

pub struct RpcImpl {
    application_manager: Arc<ApplicationManager>,
    statistics: Arc<StatisticService>,
    engine_settings: String,
}

impl RpcImpl {
    pub fn new(
        application_manager: Arc<ApplicationManager>,
        statistics: Arc<StatisticService>,
        engine_settings: String,
    ) -> Self {
        Self {
            application_manager,
            statistics,
            engine_settings,
        }
    }
}

impl MmbRpc for RpcImpl {
    fn health(&self) -> Result<Value> {
        Ok("Engine is working".into())
    }

    fn stop(&self) -> Result<Value> {
        // self.application_manager
        //     .spawn_graceful_shutdown("Stop signal from control_panel".into());

        // Ok(Value::String("ControlPanel turned off".into()))

        // TODO: fix it after actors removing
        Ok("Set config isn't implemented".into())
    }

    fn get_config(&self) -> Result<Value> {
        Ok(Value::String(self.engine_settings.clone()))
    }

    fn set_config(&self, params: Params) -> Result<Value> {
        // #[derive(Deserialize)]
        // struct Data {
        //     settings: String,
        // }

        // let data: Data = params.parse()?;

        // save_settings(data.settings.as_str(), CONFIG_PATH, CREDENTIALS_PATH).map_err(|err| {
        //     log::warn!(
        //         "Error while trying save new config in set_config endpoint: {}",
        //         err.to_string()
        //     );
        //     server_side_error(ErrorCode::FailedToSaveNewConfig)
        // })?;

        // self.application_manager
        //     .spawn_graceful_shutdown("Engine stopped cause config updating".into());

        // Ok("Config was successfully updated. Trading engine stopped".into())

        // TODO: fix it after actors removing
        Ok("Set config isn't implemented".into())
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
