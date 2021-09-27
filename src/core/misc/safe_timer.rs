use std::{sync::Arc, time::Duration};

use crate::core::{
    infrastructure::spawn_future, lifecycle::application_manager::ApplicationManager,
};

use anyhow::Result;
use async_trait::async_trait;
use futures::FutureExt;
use tokio::sync::Mutex;

/// ATTENTION: timer_action must be panic safety, because we can't handle it, with unwind_catch, while function taking `&mut self`
#[async_trait]
pub trait TimerAction {
    async fn timer_action(&mut self) -> Result<()>;
}

/// This is an entity for executing repeatable tasks with some period
pub struct SafeTimer {
    task: Arc<Mutex<dyn TimerAction + Send>>,
    name: String,
    period: Duration,
    application_manager: Arc<ApplicationManager>,
    is_critical: bool,
}

impl SafeTimer {
    pub fn new(
        task: Arc<Mutex<dyn TimerAction + Send>>,
        name: String,
        period: Duration,
        application_manager: Arc<ApplicationManager>,
        is_critical: bool,
    ) -> Arc<Mutex<Self>> {
        let this = Arc::new(Mutex::new(Self {
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
                let task = this_for_timer.lock().await.task.clone();
                let application_manager = this_for_timer.lock().await.application_manager.clone();
                SafeTimer::timer_callback(task, application_manager).await;
            }
        };

        spawn_future(name.as_str(), is_critical, action.boxed());

        this
    }
    fn create_timer(&self) {}

    async fn timer_callback(
        task: Arc<Mutex<dyn TimerAction + Send>>,
        application_manager: Arc<ApplicationManager>,
    ) {
        if let Err(error) = task.lock().await.timer_action().await {
            application_manager
                .run_graceful_shutdown(
                    format!("Timer execution callback failed: {:?}", error).as_str(),
                )
                .await;
        }
    }
}
