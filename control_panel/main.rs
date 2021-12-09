use std::panic::AssertUnwindSafe;

use control_panel::ControlPanel;
use futures::FutureExt;
use jsonrpc_core_client::transports::ipc;
use shared::rest_api::{gen_client, IPC_ADDRESS};
use tokio::signal;

mod control_panel;
mod endpoints;

pub async fn connect() -> gen_client::Client {
    ipc::connect::<_, gen_client::Client>(IPC_ADDRESS)
        .await
        .expect("Failed to connect to the IPC socket")
}

async fn control_panel_run() {
    let control_panel = ControlPanel::new("127.0.0.1:8080").await;

    control_panel
        .clone()
        .start()
        .expect("Unable to start control panel")
        .join()
        .expect("control panel finished with error");

    signal::ctrl_c().await.expect("failed to listen for event");

    // log::info!("Ctrl-C signal was received so control_panel will be stopped");
    println!("Ctrl-C signal was received so control_panel will be stopped");

    control_panel
        .stop()
        .expect("failed to get stop receiver")
        .await
        .expect("Failed to get work finished message")
        .expect("Failed to stop control panel");

    // log::info!("Ctrl-C signal was received so control_panel will be stopped");
    println!("Control panel stopped successfully");
}

#[actix_web::main]
async fn main() {
    AssertUnwindSafe(control_panel_run())
        .catch_unwind()
        .await
        .map_err(
            |panic| match panic.as_ref().downcast_ref::<String>().clone() {
                Some(panic_message) => println!("panic happened: {}", panic_message),
                None => println!("panic happened without readable message",),
                // Some(panic_message) => log::error!("panic happened: {}", panic_message),
                // None => log::error!("panic happened without readable message"),
            },
        )
        .expect("Failed to handle panic in control_panel_run");
}
