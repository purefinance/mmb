use anyhow::{anyhow, Result};
use log::{error, warn};
use parking_lot::Mutex;
use std::{sync::Arc, thread};

use super::endpoints;
use actix_web::{dev::Server, rt, App, HttpServer};
use tokio::sync::oneshot;

use crate::core::lifecycle::trading_engine::Service;

pub(crate) struct ControlPanel {
    address: String,
    // FIXME rename
    server_stopper_tx: Arc<Mutex<Option<Sender<()>>>>,
}

impl ControlPanel {
    pub(crate) fn new(address: &str) -> Arc<Self> {
        Arc::new(Self {
            address: address.to_owned(),
            server_stopper_tx: Arc::new(Mutex::new(None)),
        })
    }

    /// Stop Actix Server if it is working.
    /// Returned receiver will take a message when shutdown are completed
    pub(crate) fn stop(self: Arc<Self>) -> tokio::sync::oneshot::Receiver<Result<()>> {
        let (tx, rx) = oneshot::channel();
        rx
    }

    /// Start Actix Server in new thread
    pub(crate) fn start(self: Arc<Self>) {
        let (server_stopper_tx, server_stopper_rx) = mpsc::channel::<()>();
        *self.server_stopper_tx.lock() = Some(server_stopper_tx.clone());
        let server = HttpServer::new(move || {
            App::new()
                .data(server_stopper_rx.clone())
                .service(endpoints::health)
                .service(endpoints::stop)
                .service(endpoints::stats)
        })
        .bind(&address)?
        .shutdown_timeout(1)
        .workers(1)
        .run();

        let cloned_server = server.clone();
        thread::spawn(move || {
            rx.recv().unwrap();

            executor::block_on(cloned_server.stop(false));
        });

        thread::spawn(move || {
            if let Err(error) = self.start_server(server) {
                dbg!("Unable start rest api server: {}", error.to_string());
            }
        });
    }

    fn start_server(self: Arc<Self>, server: Server) {
        let system = Arc::new(rt::System::new());

        system.block_on({ server });
    }
}

impl Service for ControlPanel {
    fn name(&self) -> &str {
        "ControlPanel"
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>> {
        let work_finished_receiver = self.clone().stop();

        Some(work_finished_receiver)
    }
}
