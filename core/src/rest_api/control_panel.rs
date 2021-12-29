use anyhow::{bail, Context, Result};
use jsonrpc_core::MetaIoHandler;
use jsonrpc_ipc_server::{Server, ServerBuilder};
use mmb_rpc::rest_api::{MmbRpc, IPC_ADDRESS};
use parking_lot::Mutex;
use tokio::sync::oneshot;

use std::sync::{mpsc, Arc};

use crate::core::{
    lifecycle::{application_manager::ApplicationManager, trading_engine::Service},
    statistic_service::StatisticService,
};

use super::endpoints::RpcImpl;

pub(super) static FAILED_TO_SEND_STOP_NOTIFICATION: &str =
    "Failed to send stop notification to control_panel";

pub(crate) struct ControlPanel {
    server: Arc<Mutex<Option<Server>>>,
    application_manager: Arc<ApplicationManager>,

    server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    work_finished_sender: Arc<Mutex<Option<oneshot::Sender<Result<()>>>>>,
    work_finished_receiver: Arc<Mutex<Option<oneshot::Receiver<Result<()>>>>>,
}

impl ControlPanel {
    pub(crate) fn new(application_manager: Arc<ApplicationManager>) -> Arc<Self> {
        let (work_finished_sender, work_finished_receiver) = oneshot::channel();
        Arc::new(Self {
            server: Arc::new(Mutex::new(None)),
            application_manager,
            server_stopper_tx: Arc::new(Mutex::new(None)),
            work_finished_sender: Arc::new(Mutex::new(Some(work_finished_sender))),
            work_finished_receiver: Arc::new(Mutex::new(Some(work_finished_receiver))),
        })
    }

    /// Returned receiver will take a message when shutdown are completed
    pub(crate) fn stop(self: &Arc<Self>) -> Result<()> {
        match self.server_stopper_tx.lock().take() {
            Some(sender) => sender
                .send(())
                .with_context(|| format!("{}", FAILED_TO_SEND_STOP_NOTIFICATION)),
            None => bail!("{}: stopper is None", FAILED_TO_SEND_STOP_NOTIFICATION),
        }
    }

    pub(crate) fn start(
        self: Arc<Self>,
        engine_settings: String,
        statistics: Arc<StatisticService>,
    ) -> Result<()> {
        let (server_stopper_tx, server_stopper_rx) = mpsc::channel::<()>();
        *self.server_stopper_tx.lock() = Some(server_stopper_tx.clone());

        let io = self.build_io(engine_settings, statistics);

        let builder = ServerBuilder::new(io);
        let server = builder.start(IPC_ADDRESS).expect("Couldn't open socket");

        *self.server.lock() = Some(server);
        self.clone().spawn_server_stopping_action(server_stopper_rx);

        Ok(())
    }

    fn build_io(
        self: &Arc<Self>,
        engine_settings: String,
        statistics: Arc<StatisticService>,
    ) -> MetaIoHandler<()> {
        let rpc_impl = RpcImpl::new(
            self.server_stopper_tx.clone(),
            statistics.clone(),
            engine_settings.clone(),
        );

        let mut io = MetaIoHandler::<()>::default();
        io.extend_with(rpc_impl.to_delegate());

        io
    }

    fn spawn_server_stopping_action(self: Arc<Self>, server_stopper_rx: mpsc::Receiver<()>) {
        tokio::task::spawn_blocking(move || {
            if let Err(error) = server_stopper_rx.recv() {
                log::error!("Unable to receive signal to stop IPC server: {}", error);
            }

            self.server
                .lock()
                .take()
                .expect("IPC server isn't running")
                .close();

            if let Some(work_finished_sender) = self.work_finished_sender.lock().take() {
                if let Err(_) = work_finished_sender.send(Ok(())) {
                    log::error!("Unable to send notification about server stopped.",);
                }
            }
            self.application_manager
                .spawn_graceful_shutdown("Stop signal from control_panel".into());
        });
    }
}

impl Service for ControlPanel {
    fn name(&self) -> &str {
        "ControlPanel"
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>> {
        if let Err(error) = self.stop() {
            log::error!("{}: {:?}", FAILED_TO_SEND_STOP_NOTIFICATION, error);
            return None;
        }

        self.work_finished_receiver.lock().take()
    }
}
