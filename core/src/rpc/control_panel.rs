use anyhow::{Context, Result};
use futures::FutureExt;
use jsonrpc_core::MetaIoHandler;
use jsonrpc_ipc_server::{Server, ServerBuilder};
use mmb_rpc::rest_api::{MmbRpc, IPC_ADDRESS};
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};

use std::sync::Arc;

use crate::core::{
    infrastructure::spawn_future,
    lifecycle::{application_manager::ApplicationManager, trading_engine::Service},
    statistic_service::StatisticService,
};

use super::endpoints::RpcImpl;

pub(super) static FAILED_TO_SEND_STOP_NOTIFICATION: &str =
    "Failed to send stop notification to control_panel";

pub(crate) struct ControlPanel {
    server: Arc<Mutex<Option<Server>>>,
    application_manager: Option<Arc<ApplicationManager>>,

    server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    work_finished_sender: Arc<Mutex<Option<oneshot::Sender<Result<()>>>>>,
    work_finished_receiver: Arc<Mutex<Option<oneshot::Receiver<Result<()>>>>>,
}

impl ControlPanel {
    pub(crate) fn send_stop(self: &Arc<Self>) -> Result<()> {
        if let Some(sender) = self.server_stopper_tx.lock().take() {
            return sender
                .try_send(())
                .context(FAILED_TO_SEND_STOP_NOTIFICATION);
        }
        Ok(())
    }

    pub(crate) fn create_and_start(
        application_manager: Arc<ApplicationManager>,
        engine_settings: String,
        statistics: Arc<StatisticService>,
    ) -> Result<Arc<Self>> {
        Self::create_and_start_core(Some(application_manager), engine_settings, statistics, None)
    }

    pub(crate) fn create_and_start_no_config(
        wait_config_tx: mpsc::Sender<()>,
    ) -> Result<Arc<Self>> {
        let statistics = StatisticService::new();
        Self::create_and_start_core(
            None,
            "Config isn't set".into(),
            statistics,
            Some(wait_config_tx),
        )
    }

    pub fn create_and_start_core(
        application_manager: Option<Arc<ApplicationManager>>,
        engine_settings: String,
        statistics: Arc<StatisticService>,
        wait_config_tx: Option<mpsc::Sender<()>>,
    ) -> Result<Arc<Self>> {
        let (server_stopper_tx, server_stopper_rx) = mpsc::channel::<()>(10);
        let (work_finished_sender, work_finished_receiver) = oneshot::channel();
        let server_stopper_tx = Arc::new(Mutex::new(Some(server_stopper_tx)));

        let io = Self::build_io(
            engine_settings,
            statistics,
            server_stopper_tx.clone(),
            wait_config_tx,
        );

        let builder = ServerBuilder::new(io);
        let server = builder.start(IPC_ADDRESS).expect("Couldn't open socket");

        Ok(Arc::new(Self {
            server: Arc::new(Mutex::new(Some(server))),
            application_manager,
            server_stopper_tx,
            work_finished_sender: Arc::new(Mutex::new(Some(work_finished_sender))),
            work_finished_receiver: Arc::new(Mutex::new(Some(work_finished_receiver))),
        })
        .spawn_server_stopping_action(server_stopper_rx))
    }

    fn build_io(
        engine_settings: String,
        statistics: Arc<StatisticService>,
        server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
        wait_config_tx: Option<mpsc::Sender<()>>,
    ) -> MetaIoHandler<()> {
        let rpc_impl = RpcImpl::new(
            server_stopper_tx,
            statistics.clone(),
            engine_settings.clone(),
            wait_config_tx,
        );

        let mut io = MetaIoHandler::<()>::default();
        io.extend_with(rpc_impl.to_delegate());

        io
    }

    fn spawn_server_stopping_action(
        self: Arc<Self>,
        mut server_stopper_rx: mpsc::Receiver<()>,
    ) -> Arc<Self> {
        let cloned_self = self.clone();
        let stopping_action = async move {
            if server_stopper_rx.recv().await.is_none() {
                log::error!("Unable to receive signal to stop IPC server");
            }
            tokio::task::spawn_blocking(move || {
                cloned_self
                    .server
                    .lock()
                    .take()
                    .expect("IPC server isn't running")
                    .close();

                if let Some(work_finished_sender) = cloned_self.work_finished_sender.lock().take() {
                    if let Err(_) = work_finished_sender.send(Ok(())) {
                        log::error!("Unable to send notification about server stopped.",);
                    }
                }

                if let Some(application_manager) = cloned_self.application_manager.clone() {
                    application_manager
                        .spawn_graceful_shutdown("Stop signal from control_panel".into());
                }
            });
            Ok(())
        };

        spawn_future(
            "waiting to stop control panel",
            true,
            stopping_action.boxed(),
        );

        self
    }
}

impl Service for ControlPanel {
    fn name(&self) -> &str {
        "ControlPanel"
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>> {
        if let Err(error) = self.send_stop() {
            log::error!("{}: {:?}", FAILED_TO_SEND_STOP_NOTIFICATION, error);
            return None;
        }

        self.work_finished_receiver.lock().take()
    }
}
