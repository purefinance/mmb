use actix_server::ServerHandle;
use anyhow::Result;
use futures::{executor, future::BoxFuture, FutureExt};
use jsonrpc_core_client::{transports::ipc, RpcError};
use mmb_rpc::rest_api::{MmbRpcClient, IPC_ADDRESS};
use mmb_utils::logger::print_info;
use parking_lot::Mutex;
use std::{sync::mpsc, sync::Arc, time::Duration};

use super::endpoints;
use actix_web::{dev::Server, App, HttpResponse, HttpServer};
use tokio::sync::oneshot;

use actix_web::web::Data;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::{spawn_future, FutureOutcome, SpawnFutureFlags};
use tokio::task::JoinHandle;

pub type WebMmbRpcClient = Arc<tokio::sync::Mutex<Option<MmbRpcClient>>>;
pub type DataWebMmbRpcClient = Data<WebMmbRpcClient>;

pub(crate) struct ControlPanel {
    address: String,
    client: WebMmbRpcClient,
    server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    work_finished_sender: Arc<Mutex<Option<oneshot::Sender<Result<()>>>>>,
    work_finished_receiver: Arc<Mutex<Option<oneshot::Receiver<Result<()>>>>>,
}

impl ControlPanel {
    pub(crate) async fn new(address: &str) -> Arc<Self> {
        let (work_finished_sender, work_finished_receiver) = oneshot::channel();
        let client = Arc::new(tokio::sync::Mutex::new(Self::build_rpc_client().await));

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
    pub(crate) fn start(self: Arc<Self>) -> Result<JoinHandle<FutureOutcome>> {
        let (server_stopper_tx, server_stopper_rx) = mpsc::channel::<()>();
        *self.server_stopper_tx.lock() = Some(server_stopper_tx);

        let client = self.client.clone();

        let server = HttpServer::new(move || {
            let mut webui_dir = std::env::current_dir().expect("Unable get current directory");
            webui_dir.push(r"webui");

            App::new()
                .app_data(Data::new(client.clone()))
                .service(endpoints::health)
                .service(endpoints::stop)
                .service(endpoints::stats)
                .service(endpoints::get_config)
                .service(endpoints::set_config)
                .service(
                    actix_files::Files::new("/", webui_dir)
                        .use_last_modified(true)
                        .index_file("index.html"),
                )
        })
        .bind(&self.address)?
        .shutdown_timeout(1)
        .workers(1)
        .run();

        let server_handle = server.handle();
        self.clone()
            .server_stopping(server_handle, server_stopper_rx);

        print_info(format!(
            "ControlPanel has been started. WebUI is launched on http://{}",
            self.address
        ));

        Ok(self.start_server(server))
    }

    fn server_stopping(
        self: Arc<Self>,
        server_handle: ServerHandle,
        server_stopper_rx: mpsc::Receiver<()>,
    ) {
        std::thread::spawn(move || {
            if let Err(error) = server_stopper_rx.recv() {
                log::error!("Unable to receive signal to stop actix server: {}", error);
            }

            executor::block_on(server_handle.stop(true));

            if let Some(work_finished_sender) = self.work_finished_sender.lock().take() {
                if work_finished_sender.send(Ok(())).is_err() {
                    log::error!("Unable to send notification about server stopped");
                }
            }
        });
    }

    fn start_server(self: Arc<Self>, server: Server) -> JoinHandle<FutureOutcome> {
        spawn_future(
            "start server",
            SpawnFutureFlags::STOP_BY_TOKEN & SpawnFutureFlags::DENY_CANCELLATION,
            async move { server.await.map_err(Into::into) }.boxed(),
            |_, _| {},
            CancellationToken::new(),
        )
    }
}

fn handle_rpc_error(error: RpcError) -> HttpResponse {
    match error {
        RpcError::JsonRpcError(error) => {
            HttpResponse::InternalServerError().body(error.to_string())
        }
        RpcError::ParseError(msg, error) => {
            HttpResponse::BadRequest().body(format!("Failed to parse '{msg}': {error}"))
        }
        RpcError::Timeout => HttpResponse::RequestTimeout().body("Request Timeout"),
        RpcError::Client(msg) => HttpResponse::InternalServerError().body(msg),
        RpcError::Other(error) => HttpResponse::InternalServerError().body(error.to_string()),
    }
}

pub async fn send_request(
    client: DataWebMmbRpcClient,
    action: impl Fn(&MmbRpcClient) -> BoxFuture<Result<String, RpcError>>,
) -> HttpResponse {
    let mut try_counter = 1;

    async fn try_reconnect(client: DataWebMmbRpcClient, try_counter: i32) {
        log::warn!(
            "Failed to send request {}, trying to reconnect...",
            try_counter
        );
        *client.lock().await = ControlPanel::build_rpc_client().await;
    }

    loop {
        log::info!("Trying to send request attempt {}...", try_counter);

        if let Some(client) = &*client.lock().await {
            match (action)(client).await {
                Ok(response) => return HttpResponse::Ok().body(response),
                Err(err) => {
                    if try_counter > 2 {
                        return handle_rpc_error(err);
                    }
                }
            }
        }

        try_reconnect(client.clone(), try_counter).await;

        if try_counter > 2 {
            return HttpResponse::ServiceUnavailable().body("Trading engine service unavailable");
        }

        tokio::time::sleep(Duration::from_secs(3)).await;

        try_counter += 1;
    }
}
