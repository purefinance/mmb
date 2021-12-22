use actix_server::ServerHandle;
use anyhow::Result;
use futures::executor;
use jsonrpc_core_client::transports::ipc;
use mmb_rpc::rest_api::{MmbRpcClient, IPC_ADDRESS};
use parking_lot::Mutex;
use std::{
    sync::mpsc,
    sync::Arc,
    thread::{self, JoinHandle},
};

use super::endpoints;
use actix_web::{dev::Server, rt, App, HttpServer};
use tokio::sync::oneshot;

use actix_web::web::Data;

pub(crate) struct ControlPanel {
    address: String,
    client: Arc<Mutex<Option<MmbRpcClient>>>,
    server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    work_finished_sender: Arc<Mutex<Option<oneshot::Sender<Result<()>>>>>,
    work_finished_receiver: Arc<Mutex<Option<oneshot::Receiver<Result<()>>>>>,
}

impl ControlPanel {
    pub(crate) async fn new(address: &str) -> Arc<Self> {
        let (work_finished_sender, work_finished_receiver) = oneshot::channel();
        let client = Arc::new(Mutex::new(Self::build_rpc_client().await));

        Arc::new(Self {
            address: address.to_owned(),
            client,
            server_stopper_tx: Arc::new(Mutex::new(None)),
            work_finished_sender: Arc::new(Mutex::new(Some(work_finished_sender))),
            work_finished_receiver: Arc::new(Mutex::new(Some(work_finished_receiver))),
        })
    }

    pub async fn build_rpc_client() -> Option<MmbRpcClient> {
        ipc::connect::<_, MmbRpcClient>(IPC_ADDRESS)
            .await
            .map_err(|err| log::warn! {"Failed to connect to IPC server: {}", err.to_string()})
            .ok()
    }

    /// Returned receiver will take a message when shutdown are completed
    pub(crate) fn stop(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>> {
        if let Some(server_stopper_tx) = self.server_stopper_tx.lock().take() {
            if let Err(error) = server_stopper_tx.send(()) {
                log::error!("Unable to send signal to stop actix server: {}", error);
            }
        }

        let work_finished_receiver = self.work_finished_receiver.lock().take();
        work_finished_receiver
    }

    /// Start Actix Server in new thread
    pub(crate) fn start(self: Arc<Self>) -> Result<JoinHandle<()>> {
        let (server_stopper_tx, server_stopper_rx) = mpsc::channel::<()>();
        *self.server_stopper_tx.lock() = Some(server_stopper_tx.clone());

        let client = self.client.clone();

        let server = HttpServer::new(move || {
            App::new()
                .app_data(Data::new(client.clone()))
                .service(endpoints::health)
                .service(endpoints::stop)
                .service(endpoints::stats)
                .service(endpoints::get_config)
                .service(endpoints::set_config)
        })
        .bind(&self.address)?
        .shutdown_timeout(1)
        .workers(1)
        .run();

        let server_handle = server.handle();
        self.clone()
            .server_stopping(server_handle, server_stopper_rx);

        Ok(self.clone().start_server(server))
    }

    fn server_stopping(
        self: Arc<Self>,
        server_handle: ServerHandle,
        server_stopper_rx: mpsc::Receiver<()>,
    ) {
        let cloned_self = self.clone();
        thread::spawn(move || {
            if let Err(error) = server_stopper_rx.recv() {
                log::error!("Unable to receive signal to stop actix server: {}", error);
            }

            executor::block_on(server_handle.stop(true));

            if let Some(work_finished_sender) = cloned_self.work_finished_sender.lock().take() {
                if let Err(_) = work_finished_sender.send(Ok(())) {
                    log::error!(
                        "Unable to send notification about server stopped. Probably receiver is already dropped",
                    );
                }
            }
        });
    }

    fn start_server(self: Arc<Self>, server: Server) -> JoinHandle<()> {
        thread::spawn(move || {
            let system = Arc::new(rt::System::new());

            system.block_on(async {
                let _ = server;
            });
        })
    }
}