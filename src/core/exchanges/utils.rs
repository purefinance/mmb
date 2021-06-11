use anyhow::Result;
use futures::future::FutureExt;
use futures::Future;
use log::{error, info, trace};
use panic::UnwindSafe;
use std::{panic, sync::Arc};
use std::{
    pin::Pin,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::{application_manager::ApplicationManager, common::OPERATION_CANCELED_MSG};

type BoxFutureUnwind = Box<dyn Future<Output = Result<()>> + Sync + Send + UnwindSafe>;

pub(crate) fn get_current_milliseconds() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Unable to get time since unix epoch started")
        .as_millis()
}

pub(crate) fn spawn_task(
    action_name: &str,
    _timeout: Option<Duration>,
    action: Pin<BoxFutureUnwind>,
    is_critical: bool,
    application_manager: Arc<ApplicationManager>,
) -> JoinHandle<()> {
    let action_name = action_name.to_owned();
    let future_id = Uuid::new_v4();
    let log_template = format!("Future {}, with id {}", action_name, future_id);

    info!("{} started", log_template);

    let handler = tokio::spawn(async move {
        let maybe_panic = action.catch_unwind().await;
        match maybe_panic {
            Ok(future_outcome) => match future_outcome {
                Ok(_) => trace!("{} successfully completed", log_template),
                Err(error) => {
                    if error.to_string() == OPERATION_CANCELED_MSG {
                        trace!("{} was cancelled via Result<()>", log_template);

                        return;
                    }

                    error!("{} returned error: {:?}", log_template, error);
                }
            },
            Err(error) => {
                if let Some(error_msg) = error.as_ref().downcast_ref::<String>() {
                    if error_msg.to_string() == OPERATION_CANCELED_MSG {
                        trace!("{} was cancelled via panic", log_template);

                        if !is_critical {
                            return;
                        }
                    }
                }

                let error_message = format!("{} panicked with error: {:?}", log_template, error);
                error!("{}", error_message);
                application_manager
                    .run_graceful_shutdown(&error_message)
                    .await;
            }
        }
    });
    handler
}

#[cfg(test)]
mod test {

    use futures::future::ready;

    use super::*;

    #[tokio::test]
    async fn first_test() {
        // Arrange
        // Act
        dbg!(&"TEST");
        let future = async {
            dbg!(&"Worked");
            Ok(())
        };

        let handler = spawn_task(
            "test_action_name",
            "test_service_name",
            true,
            None,
            Box::pin(future),
        );
        handler.await;
        // Assert
    }
}
