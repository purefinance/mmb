use std::sync::Arc;

use anyhow::Context;
use futures::FutureExt;
use jsonrpc_core::{MetaIoHandler, Result};
use jsonrpc_ipc_server::{Server, ServerBuilder};
use mmb_rpc::rest_api::{server_side_error, ErrorCode, MmbRpc, IPC_ADDRESS};
use parking_lot::Mutex;
use tokio::sync::{mpsc, oneshot};

use crate::{
    config::{save_settings, CONFIG_PATH, CREDENTIALS_PATH},
    infrastructure::spawn_future,
    lifecycle::application_manager::ApplicationManager,
    rpc::control_panel::FAILED_TO_SEND_STOP_NOTIFICATION,
};

pub(super) fn set_config(settings: String) -> Result<()> {
    save_settings(settings.as_str(), CONFIG_PATH, CREDENTIALS_PATH).map_err(|err| {
        log::warn!(
            "Error while trying to save new config in set_config endpoint: {}",
            err.to_string()
        );
        server_side_error(ErrorCode::FailedToSaveNewConfig)
    })?;

    Ok(())
}

/// Send signal to stop TradingEngine
pub(super) fn send_stop(stopper: Arc<Mutex<Option<mpsc::Sender<()>>>>) -> Result<String> {
    match stopper.lock().take() {
        Some(sender) => {
            if let Err(error) = sender.try_send(()) {
                log::error!("{}: {:?}", FAILED_TO_SEND_STOP_NOTIFICATION, error);
                return Err(server_side_error(ErrorCode::UnableToSendSignal));
            };
            let msg = "Trading engine is going to turn off";
            log::info!("{} by control panel", msg);
            Ok(msg.into())
        }
        None => {
            log::warn!(
                "{}: the signal is already sent",
                FAILED_TO_SEND_STOP_NOTIFICATION
            );
            Err(server_side_error(ErrorCode::StopperIsNone))
        }
    }
}

/// Stop RPC server
pub(super) fn stop_server(
    server_stopper_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
) -> anyhow::Result<()> {
    if let Some(sender) = server_stopper_tx.lock().take() {
        return sender
            .try_send(())
            .context(FAILED_TO_SEND_STOP_NOTIFICATION);
    }
    Ok(())
}

pub(super) fn build_io(rpc: impl MmbRpc) -> MetaIoHandler<()> {
    let mut io = MetaIoHandler::<()>::default();
    io.extend_with(rpc.to_delegate());

    io
}

pub(super) struct RpcServerAndChannels<T> {
    pub server: Server,
    pub work_finished_sender: oneshot::Sender<T>,
    pub work_finished_receiver: oneshot::Receiver<T>,
}

pub(super) fn crate_server_and_channels<T>(rpc: impl MmbRpc) -> RpcServerAndChannels<T> {
    let (work_finished_sender, work_finished_receiver) = oneshot::channel();
    let io = build_io(rpc);
    let builder = ServerBuilder::new(io);
    let server = builder.start(IPC_ADDRESS).expect("Couldn't open socket");

    RpcServerAndChannels {
        server,
        work_finished_sender,
        work_finished_receiver,
    }
}

pub(super) fn spawn_server_stopping_action<T>(
    future_name: &str,
    server: Server,
    work_finished_sender: oneshot::Sender<T>,
    msg_to_sender: T,
    mut server_stopper_rx: mpsc::Receiver<()>,
    application_manager: Option<Arc<ApplicationManager>>,
) where
    T: Send + 'static,
{
    let stopping_action = async move {
        if server_stopper_rx.recv().await.is_none() {
            log::error!("Unable to receive signal to stop RPC server");
        }

        // Time to send a response to the ControlPanel before closing the server
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        tokio::task::spawn_blocking(move || {
            server.close();

            if let Err(_) = work_finished_sender.send(msg_to_sender) {
                log::warn!("Unable to send notification about server stopped");
            }

            if let Some(application_manager) = application_manager {
                application_manager.spawn_graceful_shutdown("Stop signal from RPC server".into());
            }
        });
        Ok(())
    };

    spawn_future(future_name, true, stopping_action.boxed());
}
