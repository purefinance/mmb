use std::panic::AssertUnwindSafe;

use control_panel::ControlPanel;
use futures::FutureExt;
use tokio::signal;

mod control_panel;
mod endpoints;

async fn control_panel_run() {
    let control_panel = ControlPanel::new("127.0.0.1:8080").await;

    control_panel
        .clone()
        .start()
        .expect("Unable to start control panel")
        .join()
        .expect("control panel finished with error");

    signal::ctrl_c().await.expect("failed to listen for event");

    log::info!("Ctrl-C signal was received so control_panel will be stopped");

    control_panel
        .stop()
        .expect("failed to get stop receiver")
        .await
        .expect("Failed to get work finished message")
        .expect("Failed to stop control panel");

    log::info!("Ctrl-C signal was received so control_panel will be stopped");
}

#[actix_web::main]
async fn main() {
    // TODO: fix #316
    // init_logger();

    AssertUnwindSafe(control_panel_run())
        .catch_unwind()
        .await
        .map_err(
            |panic| match panic.as_ref().downcast_ref::<String>().clone() {
                Some(panic_message) => log::error!("panic happened: {}", panic_message),
                None => log::error!("panic happened without readable message"),
            },
        )
        .expect("Failed to handle panic in control_panel_run");
}
