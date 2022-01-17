use anyhow::Result;
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};

use std::sync::Arc;

use crate::{
    lifecycle::{application_manager::ApplicationManager, trading_engine::Service},
    statistic_service::StatisticService,
};

use super::{
    common::{
        crate_server_and_channels, spawn_server_stopping_action, stop_server, RpcServerAndChannels,
    },
    rpc_impl::RpcImpl,
};

pub(super) static FAILED_TO_SEND_STOP_NOTIFICATION: &str =
    "Failed to send stop notification to control_panel";

pub(crate) struct CoreApi {
    server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    work_finished_receiver: Arc<Mutex<Option<oneshot::Receiver<Result<()>>>>>,
}

impl CoreApi {
    pub(crate) fn create_and_start(
        application_manager: Arc<ApplicationManager>,
        engine_settings: String,
        statistics: Arc<StatisticService>,
    ) -> Result<Arc<Self>> {
        let (server_stopper_tx, server_stopper_rx) = mpsc::channel::<()>(10);
        let server_stopper_tx = Arc::new(Mutex::new(Some(server_stopper_tx.clone())));
        let RpcServerAndChannels {
            server,
            work_finished_sender,
            work_finished_receiver,
        } = crate_server_and_channels(RpcImpl::new(
            server_stopper_tx.clone(),
            statistics,
            engine_settings,
        ));

        spawn_server_stopping_action(
            "waiting to stop ControlPanel",
            server,
            work_finished_sender,
            Ok(()),
            server_stopper_rx,
            Some(application_manager),
        );

        log::info!("ControlPanel is started");
        Ok(Arc::new(Self {
            server_stopper_tx,
            work_finished_receiver: Arc::new(Mutex::new(Some(work_finished_receiver))),
        }))
    }
}

impl Service for CoreApi {
    fn name(&self) -> &str {
        "ControlPanel"
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>> {
        if let Err(error) = stop_server(self.server_stopper_tx.clone()) {
            log::error!("{}: {:?}", FAILED_TO_SEND_STOP_NOTIFICATION, error);
            return None;
        }

        self.work_finished_receiver.lock().take()
    }
}
