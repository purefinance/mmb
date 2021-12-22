use anyhow::Result;
use jsonrpc_core::MetaIoHandler;
use jsonrpc_ipc_server::{CloseHandle, Server, ServerBuilder};
use mmb_rpc::rest_api::{MmbRpc, IPC_ADDRESS};
use parking_lot::Mutex;
use tokio::sync::oneshot;

use std::{sync::Arc, thread};

use crate::{
    lifecycle::{application_manager::ApplicationManager, trading_engine::Service},
    statistic_service::StatisticService,
};

use super::endpoints::RpcImpl;

pub(crate) struct ControlPanel {
    server_handle: Mutex<Option<CloseHandle>>,
}

impl ControlPanel {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            server_handle: Mutex::new(None),
        })
    }

    /// Returned receiver will take a message when shutdown are completed
    pub(crate) fn stop(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>> {
        let (work_finished_sender, work_finished_receiver) = oneshot::channel();

        if let Some(server_handle) = self.server_handle.lock().take() {
            thread::spawn(move || {
                server_handle.close();
                if work_finished_sender.send(Ok(())).is_err() {
                    log::error!(
                        "ControlPanel::stop() failed to send message about successful stopping"
                    )
                }
            });
        }
        Some(work_finished_receiver)
    }

    /// Start IPC Server in new thread
    pub(crate) fn start(
        self: Arc<Self>,
        engine_settings: String,
        application_manager: Arc<ApplicationManager>,
        statistics: Arc<StatisticService>,
    ) -> Result<()> {
        let io = self
            .clone()
            .build_io(engine_settings, application_manager.clone(), statistics);

        let builder = ServerBuilder::new(io);
        let server = builder.start(IPC_ADDRESS).expect("Couldn't open socket");

        *self.server_handle.lock() = Some(server.close_handle());
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
            engine_settings.clone(),
        );

        let mut io = MetaIoHandler::<()>::default();
        io.extend_with(rpc_impl.to_delegate());

        io
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
