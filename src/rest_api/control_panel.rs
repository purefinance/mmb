use anyhow::Result;
use log::{error, warn};
use parking_lot::Mutex;
use std::{sync::Arc, thread};

use super::endpoints;
use actix_web::{dev::Server, rt, App, HttpServer};
use tokio::sync::oneshot;

use crate::core::lifecycle::trading_engine::Service;

pub(crate) struct ControlPanel {
    address: String,
    server: Arc<Mutex<Option<Server>>>,
}

impl ControlPanel {
    pub(crate) fn new(address: &str) -> Arc<Self> {
        Arc::new(Self {
            address: address.to_owned(),
            server: Arc::new(Mutex::new(None)),
        })
    }

    /// Start Actix Server in new thread
    pub(crate) fn start(self: Arc<Self>) {
        thread::spawn(move || {
            if let Err(error) = self.start_server() {
                error!("Unable start rest api server: {}", error.to_string());
            }
        });
    }

    /// Stop Actix Server if it is working.
    /// Returned receiver will take a message when shutdown are completed
    pub(crate) fn stop(self: Arc<Self>) -> tokio::sync::oneshot::Receiver<Result<()>> {
        let (tx, rx) = oneshot::channel();

        let cloned_self = self.clone();
        let runtime_handler = tokio::runtime::Handle::current();
        thread::spawn(move || {
            let maybe_server = cloned_self.server.lock();

            match &(*maybe_server) {
                Some(server) => runtime_handler.block_on(async {
                    server.stop(true).await;

                    let _ = tx.send(Ok(()));
                }),
                None => {
                    let error_message =
                        "Unable to stop ControlPanel because server is not runnning";
                    warn!("{}", error_message);
                    let _ = tx.send(Ok(()));
                }
            }
        });

        rx
    }

    fn start_server(self: Arc<Self>) -> std::io::Result<()> {
        let address = self.address.clone();

        let system = Arc::new(rt::System::new());
        let server = HttpServer::new(|| {
            App::new()
                .service(endpoints::health)
                .service(endpoints::stop)
                .service(endpoints::stats)
        })
        .bind(&address)?
        .shutdown_timeout(1)
        .workers(1);

        system.block_on(async {
            *self.server.lock() = Some(server.run());
        });

        Ok(())
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
