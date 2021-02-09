use crate::core::lifecycle::bot::Service;
use crate::core::text;
use actix::Recipient;
use actix::{Message, System};
use futures::future::join_all;
use log::{error, info, trace};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Sender;
use tokio::time::{sleep, Duration};

#[derive(Message)]
#[rtype(result = "()")]
pub struct GracefulShutdownMsg {
    pub service_finished: Sender<()>,
}

struct State {
    services: Vec<Arc<dyn Service>>,
    actors: Vec<Recipient<GracefulShutdownMsg>>,
}

pub struct ShutdownService {
    state: Mutex<State>,
}

impl ShutdownService {
    pub fn new() -> Arc<Self> {
        Arc::new(ShutdownService {
            state: Mutex::new(State {
                services: vec![],
                actors: vec![],
            }),
        })
    }

    pub fn register_service(&self, service: Arc<dyn Service>) {
        self.state.lock().services.push(service);
    }

    pub fn register_actor(&self, actor: Recipient<GracefulShutdownMsg>) {
        self.state.lock().actors.push(actor);
    }

    pub async fn start_graceful_shutdown(&self) -> Vec<String> {
        let mut weak_services = Vec::new();
        let mut finish_receivers = Vec::new();

        {
            let mut state_guard = self.state.lock();
            for actor in &state_guard.actors {
                let (sender, receiver) = oneshot::channel::<()>();
                let _ = actor.try_send(GracefulShutdownMsg {
                    service_finished: sender,
                });
                finish_receivers.push(receiver);
            }

            // try drop services
            for service in state_guard.services.drain(..) {
                let (sender, receiver) = oneshot::channel::<()>();
                service.graceful_shutdown(sender);
                let weak_service = Arc::downgrade(&service);
                weak_services.push(weak_service);
                finish_receivers.push(receiver);
            }
        }

        let timeout = Duration::from_secs(5);
        tokio::select! {
            _ = join_all(finish_receivers) => trace!("All services sent finished marker at given time"),
            _ = sleep(timeout) => error!("Not all services finished after timeout ({} sec)", timeout.as_secs()),
        }

        System::current().stop();

        let mut not_dropped_services = Vec::new();
        for weak_service in weak_services {
            if weak_service.strong_count() > 0 {
                match weak_service.upgrade() {
                    None => { /* Nothing to do. Object is dropped. It is ok. */ }
                    Some(service) => {
                        not_dropped_services.push(service.name().to_string());
                    }
                }
            }
        }

        if !not_dropped_services.is_empty() {
            error!(
                "After graceful shutdown follow services wasn't dropped:{}{}",
                text::LINE_ENDING,
                not_dropped_services.join(text::LINE_ENDING)
            )
        } else {
            info!("After graceful shutdown all services dropped completely")
        }

        not_dropped_services
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::logger::init_logger;

    #[actix_rt::test]
    pub async fn success() {
        pub struct TestService;

        impl TestService {
            pub fn new() -> Arc<Self> {
                Arc::new(Self)
            }
        }

        impl Service for TestService {
            fn name(&self) -> &str {
                "SomeTestService"
            }
        }

        init_logger();

        let shutdown_service = ShutdownService::new();

        let test = TestService::new();
        shutdown_service.register_service(test);

        let not_dropped_services = shutdown_service.start_graceful_shutdown().await;
        assert_eq!(not_dropped_services.len(), 0);
    }

    #[actix_rt::test]
    pub async fn failed() {
        const REF_TEST_SERVICE: &str = "RefTestService";
        pub struct RefTestService(Mutex<Option<Arc<RefTestService>>>);

        impl RefTestService {
            pub fn new() -> Arc<Self> {
                Arc::new(Self(Mutex::new(None)))
            }

            pub fn set_ref(&self, service: Arc<RefTestService>) {
                *self.0.lock() = Some(service);
            }
        }

        impl Service for RefTestService {
            fn name(&self) -> &str {
                REF_TEST_SERVICE
            }
        }

        init_logger();

        let shutdown_service = ShutdownService::new();

        let test = RefTestService::new();
        let clone = test.clone();
        test.set_ref(clone);
        shutdown_service.register_service(test);

        let not_dropped_services = shutdown_service.start_graceful_shutdown().await;
        assert_eq!(not_dropped_services, vec![REF_TEST_SERVICE.to_string()]);
    }
}
