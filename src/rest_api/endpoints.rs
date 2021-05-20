use anyhow::Result;
use log::warn;
use parking_lot::Mutex;
use std::{
    sync::{mpsc, Arc},
    thread,
};

use actix_web::{dev::Server, get, rt, App, HttpResponse, HttpServer, Responder};
use tokio::runtime::Runtime;
use tokio::sync::oneshot;

use crate::core::lifecycle::trading_engine::Service;

pub(crate) struct ControlPanel {
    address: String,
    work_finished_receiver: Mutex<Option<oneshot::Receiver<Result<()>>>>,
    tx: mpsc::SyncSender<Server>,
}

impl ControlPanel {
    pub(crate) fn new(address: &str) -> Arc<Self> {
        let (tx, rx) = mpsc::sync_channel(1);
        Arc::new(Self {
            address: address.to_owned(),
            work_finished_receiver: Default::default(),
            tx,
        })
    }

    pub(crate) async fn start(self: Arc<Self>) -> std::io::Result<()> {
        thread::spawn(move || {
            self.start_server();
        });

        Ok(())
    }

    fn start_server(&self) -> std::io::Result<()> {
        let system = rt::System::new();
        //let rt = Runtime::new()?;

        let server = HttpServer::new(|| App::new().service(health).service(stop).service(stats))
            .bind(&self.address)
            .expect("fix it")
            .shutdown_timeout(3)
            .workers(1)
            .run();

        //rt.block_on(server);

        Ok(())
    }
}

impl Service for ControlPanel {
    fn name(&self) -> &str {
        "ControlPanel"
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>> {
        let (is_work_finished_sender, receiver) = oneshot::channel();
        *self.work_finished_receiver.lock() = Some(receiver);

        let work_finished_receiver = self.work_finished_receiver.lock().take();
        if work_finished_receiver.is_none() {
            warn!("'work_finished_receiver' wasn't created when started graceful shutdown in InternalEventsLoop");
        }

        // TODO if stop
        let _ = is_work_finished_sender.send(Ok(()));

        work_finished_receiver
    }
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().body("Bot is working")
}

#[get("/stop")]
async fn stop() -> impl Responder {
    // TODO It is just a stub. Fix method body in the future
    HttpResponse::Ok().body("Stub for bot stopping")
}

#[get("/stats")]
async fn stats() -> impl Responder {
    // TODO It is just a stub. Fix method body in the future
    HttpResponse::Ok().body("Stub for getting stats")
}
