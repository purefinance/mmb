use std::sync::Arc;

use anyhow::Result;
use mmb_utils::logger::print_info;
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};

use crate::lifecycle::app_lifetime_manager::ActionAfterGracefulShutdown;

use super::{
    common::{
        crate_server_and_channels, spawn_server_stopping_action, stop_server, RpcServerAndChannels,
    },
    rpc_impl_no_config::RpcImplNoConfig,
};

pub(crate) struct ConfigWaiter {
    server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<ActionAfterGracefulShutdown>>>>,
    pub work_finished_receiver: Mutex<Option<oneshot::Receiver<()>>>,
}

impl ConfigWaiter {
    pub(crate) fn create_and_start(wait_config_tx: mpsc::Sender<()>) -> Result<Arc<Self>> {
        let (server_stopper_tx, server_stopper_rx) =
            mpsc::channel::<ActionAfterGracefulShutdown>(10);
        let server_stopper_tx = Arc::new(Mutex::new(Some(server_stopper_tx)));
        let RpcServerAndChannels {
            server,
            work_finished_sender,
            work_finished_receiver,
        } = crate_server_and_channels(RpcImplNoConfig::new(
            server_stopper_tx.clone(),
            wait_config_tx,
        ));

        spawn_server_stopping_action(
            "waiting to stop ConfigWaiter",
            server,
            work_finished_sender,
            (),
            server_stopper_rx,
            None,
        );

        print_info("ConfigWaiter is started. Please send the config via the ControlPanel for start the TradingEngine");
        Ok(Arc::new(Self {
            server_stopper_tx,
            work_finished_receiver: Mutex::new(Some(work_finished_receiver)),
        }))
    }

    pub(crate) fn stop_server(&self) {
        stop_server(self.server_stopper_tx.clone()).expect("Failed to stop RPC server");
        log::info!("ConfigWaiter is stopped");
    }
}
