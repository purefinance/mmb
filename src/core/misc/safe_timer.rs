use std::{sync::Arc, time::Duration};

use crate::core::{
    infrastructure::spawn_future, lifecycle::application_manager::ApplicationManager,
};

use anyhow::Result;
use futures::FutureExt;
use tokio::sync::Mutex;

use super::traits::async_function::AsyncFnCall;

pub struct SafeTimer {
    action: Option<Box<dyn FnMut() -> Result<()> + Send + Sync>>,
    task: Option<Box<dyn AsyncFnCall>>,
    name: String,
    period: Duration,
    application_manager: ApplicationManager,
    is_critical: bool,
}

impl SafeTimer {
    pub fn new(
        action: Option<Box<dyn FnMut() -> Result<()> + Send + Sync>>,
        task: Option<Box<dyn AsyncFnCall>>,
        name: String,
        period: Duration,
        application_manager: ApplicationManager,
        is_critical: bool,
    ) -> Arc<Mutex<Self>> {
        let this = Arc::new(Mutex::new(Self {
            action,
            task,
            name: name.clone(),
            period: period.clone(),
            application_manager,
            is_critical: is_critical.clone(),
        }));

        let this_for_timer = this.clone();
        let action = async move {
            loop {
                tokio::time::sleep(period).await;
                this_for_timer.lock().await.timer_callback().await;
            }
        };

        spawn_future(name.as_str(), is_critical, action.boxed());

        this
    }
    fn create_timer(&self) {}

    async fn timer_callback(&mut self) {
        let res = if let Some(ref mut task) = self.task {
            task.call().await
        } else if let Some(ref mut action) = self.action {
            (action)()
        } else {
            Err(anyhow::Error::msg(
                "Task or action is not assigned for SafeTimer",
            ))
        };

        if let Err(error) = res {
            self.application_manager
                .run_graceful_shutdown(
                    format!("Timer execution callback failed: {:?}", error).as_str(),
                )
                .await;
        }
    }
}
