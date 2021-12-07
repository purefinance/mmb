use anyhow::Result;
use jsonrpc_core::MetaIoHandler;
use jsonrpc_ipc_server::{CloseHandle, Server, ServerBuilder};
use parking_lot::Mutex;
use shared::rest_api::{Rpc, IPC_ADDRESS};
use tokio::sync::oneshot;

use std::{sync::mpsc, sync::mpsc::Sender, sync::Arc, thread};

use crate::core::{
    lifecycle::{application_manager::ApplicationManager, trading_engine::Service},
    statistic_service::StatisticService,
};

use super::endpoints::RpcImpl;

pub(crate) struct ControlPanel {
    server_stopper_tx: Arc<Mutex<Option<Sender<()>>>>,
    work_finished_sender: Arc<Mutex<Option<oneshot::Sender<Result<()>>>>>,
    work_finished_receiver: Arc<Mutex<Option<oneshot::Receiver<Result<()>>>>>,
}

impl ControlPanel {
    pub(crate) fn new() -> Arc<Self> {
        let (work_finished_sender, work_finished_receiver) = oneshot::channel();
        Arc::new(Self {
            server_stopper_tx: Arc::new(Mutex::new(None)),
            work_finished_sender: Arc::new(Mutex::new(Some(work_finished_sender))),
            work_finished_receiver: Arc::new(Mutex::new(Some(work_finished_receiver))),
        })
    }

    /// Returned receiver will take a message when shutdown are completed
    pub(crate) fn stop(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>> {
        if let Some(server_stopper_tx) = self.server_stopper_tx.lock().take() {
            if let Err(error) = server_stopper_tx.send(()) {
                log::error!("Unable to send signal to stop IPC server: {}", error);
            }
        }

        let work_finished_receiver = self.work_finished_receiver.lock().take();
        work_finished_receiver
    }

    /// Start IPC Server in new thread
    pub(crate) fn start(
        self: Arc<Self>,
        engine_settings: String,
        application_manager: Arc<ApplicationManager>,
        statistics: Arc<StatisticService>,
    ) -> Result<()> {
        let (server_stopper_tx, server_stopper_rx) = mpsc::channel::<()>();
        *self.server_stopper_tx.lock() = Some(server_stopper_tx);

        let io = self
            .clone()
            .build_io(engine_settings, application_manager, statistics);

        let builder = ServerBuilder::new(io);
        let server = builder.start(IPC_ADDRESS).expect("Couldn't open socket");

        self.clone()
            .server_stopping(server.close_handle(), server_stopper_rx);

        self.clone().start_server(server);

        Ok(())
    }

    fn build_io(
        self: Arc<Self>,
        engine_settings: String,
        application_manager: Arc<ApplicationManager>,
        statistics: Arc<StatisticService>,
    ) -> MetaIoHandler<()> {
        let rpc_impl = RpcImpl::new(
            application_manager.clone(),
            statistics.clone(),
            self.server_stopper_tx.clone(),
            engine_settings.clone(),
        );

        let mut io = MetaIoHandler::<()>::default();
        io.extend_with(rpc_impl.to_delegate());

        io
    }

    fn server_stopping(
        self: Arc<Self>,
        server_handle: CloseHandle,
        server_stopper_rx: mpsc::Receiver<()>,
    ) {
        let cloned_self = self.clone();
        thread::spawn(move || {
            if let Err(error) = server_stopper_rx.recv() {
                log::error!("Unable to receive signal to stop IPC server: {}", error);
            }

            server_handle.close();

            if let Some(work_finished_sender) = cloned_self.work_finished_sender.lock().take() {
                if let Err(_) = work_finished_sender.send(Ok(())) {
                    log::error!("Unable to send notification about server stopped.",);
                }
            }
        });
    }

    fn start_server(self: Arc<Self>, server: Server) {
        thread::spawn(move || {
            server.wait();
        });
    }
}

impl Service for ControlPanel {
    fn name(&self) -> &str {
        "ControlPanel"
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>> {
        self.clone().stop()
    }
}
