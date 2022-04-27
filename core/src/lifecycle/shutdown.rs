use crate::lifecycle::trading_engine::Service;
use crate::text;
use futures::future::join_all;
use futures::FutureExt;
use itertools::Itertools;
use mmb_utils::logger::print_info;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

/// User side - for services that may be optional depending on the user's preference
/// Core side - for the core services that provide the TradingEngine to work
#[derive(Clone, Copy)]
enum Priority {
    User,
    Core,
}

#[derive(Default)]
struct State {
    user_services: Vec<Arc<dyn Service>>,
    core_services: Vec<Arc<dyn Service>>,
}

impl State {
    pub(crate) fn get_state(&self, side: Priority) -> &Vec<Arc<dyn Service>> {
        match side {
            Priority::User => &self.user_services,
            Priority::Core => &self.core_services,
        }
    }

    pub(crate) fn get_state_mut(&mut self, side: Priority) -> &mut Vec<Arc<dyn Service>> {
        match side {
            Priority::User => &mut self.user_services,
            Priority::Core => &mut self.core_services,
        }
    }
}

#[derive(Default)]
pub struct ShutdownService {
    state: Mutex<State>,
}

fn service_has_been_registered_msg(name: &str, side: &str) -> String {
    format!("\tThe {name} has been registered in ShutdownService {side} service")
}

impl ShutdownService {
    pub fn register_user_service(self: &Arc<Self>, service: Arc<dyn Service>) {
        print_info(service_has_been_registered_msg(service.name(), "user"));
        self.state.lock().user_services.push(service);
    }

    pub(crate) fn register_core_service(self: &Arc<Self>, service: Arc<dyn Service>) {
        print_info(service_has_been_registered_msg(service.name(), "core"));
        self.state.lock().core_services.push(service);
    }

    pub fn register_user_services(self: &Arc<Self>, services: &[Arc<dyn Service>]) {
        for service in services {
            print_info(service_has_been_registered_msg(service.name(), "user"));
            self.register_user_service(service.clone());
        }
    }

    pub(crate) async fn user_lvl_shutdown(&self) -> Vec<String> {
        self.graceful_shutdown(Priority::User).await
    }

    pub(crate) async fn core_lvl_shutdown(&self) -> Vec<String> {
        self.graceful_shutdown(Priority::Core).await
    }

    async fn graceful_shutdown(&self, side: Priority) -> Vec<String> {
        let mut finish_receivers = Vec::new();

        log::trace!("Prepare to drop services in ShutdownService started");

        {
            log::trace!("Running graceful shutdown for services started");

            let state_guard = self.state.lock();
            for service in state_guard.get_state(side) {
                let service_name = format!("{} service", service.name());
                print_info(format_args!(
                    "\tStarting to close the {service_name} service..."
                ));
                let receiver = service.clone().graceful_shutdown();

                if let Some(receiver) = receiver {
                    log::trace!("Waiting finishing graceful shutdown for {service_name}");
                    finish_receivers.push((service_name, receiver));
                } else {
                    print_info(format_args!(
                        "\tService {service_name} not needed waiting graceful shutdown or already finished",
                    ));
                }
            }
            log::trace!("Running graceful shutdown for services finished");
        }

        // log errors when its came
        let finishing_services_futures = finish_receivers
            .into_iter()
            .map(|(service_name, receiver)| {
                receiver.map(
                    move |finishing_service_send_result| match finishing_service_send_result {
                        Err(err) => {
                           log::error!(
                                "Can't receive message for finishing graceful shutdown in {} because of error: {:?}",
                                service_name,
                                err
                            );
                        },
                        Ok(finishing_service_result) => match finishing_service_result {
                            Err(err) => {
                               log::error!(
                                    "{} finished on graceful shutdown with error: {:?}",
                                    service_name,
                                    err
                                );
                            }
                            Ok(_) => {
                                print_info(format_args!("\tThe {service_name} has been stopped successfully"));
                            },
                        },
                    },
                )
            })
            .collect_vec();

        const TIMEOUT: Duration = Duration::from_secs(3);
        tokio::select! {
            _ = join_all(finishing_services_futures) =>log::trace!("All services sent finished marker at given time"),
            _ = sleep(TIMEOUT) =>log::error!("Not all services finished after timeout ({} sec)", TIMEOUT.as_secs()),
        }

        log::trace!("Prepare to drop services in ShutdownService finished");
        log::trace!("Drop services in ShutdownService started");

        let weak_services;
        {
            let mut state_guard = self.state.lock();
            weak_services = state_guard
                .get_state_mut(side)
                .drain(..)
                .map(|x| Arc::downgrade(&x))
                .collect_vec();
        }

        log::trace!("Drop services in ShutdownService finished");

        let not_dropped_services = weak_services
            .iter()
            .filter_map(|weak_service| {
                if weak_service.strong_count() > 0 {
                    weak_service
                        .upgrade()
                        .map(|service| service.name().to_string())
                } else {
                    None
                }
            })
            .collect_vec();

        if not_dropped_services.is_empty() {
            log::info!("After graceful shutdown all services dropped completely")
        } else {
            log::error!(
                "After graceful shutdown follow services wasn't dropped:{}{}",
                text::LINE_ENDING,
                not_dropped_services.join(text::LINE_ENDING)
            )
        }

        not_dropped_services
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use mmb_utils::logger::init_logger_file_named;
    use tokio::sync::oneshot::Receiver;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn success() {
        init_logger_file_named("log.txt");

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

            fn graceful_shutdown(self: Arc<Self>) -> Option<Receiver<Result<()>>> {
                None
            }
        }

        let shutdown_service = Arc::new(ShutdownService::default());

        let test = TestService::new();
        shutdown_service.clone().register_user_service(test);

        let not_dropped_services = shutdown_service.user_lvl_shutdown().await;
        assert_eq!(not_dropped_services.len(), 0);

        let not_dropped_services = shutdown_service.core_lvl_shutdown().await;
        assert_eq!(not_dropped_services.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn failed() {
        init_logger_file_named("log.txt");

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

            fn graceful_shutdown(self: Arc<Self>) -> Option<Receiver<Result<()>>> {
                None
            }
        }

        let shutdown_service = Arc::new(ShutdownService::default());

        let test = RefTestService::new();
        let clone = test.clone();
        test.set_ref(clone);
        shutdown_service.clone().register_user_service(test);

        let not_dropped_services = shutdown_service.user_lvl_shutdown().await;
        assert_eq!(not_dropped_services, vec![REF_TEST_SERVICE.to_string()]);

        let not_dropped_services = shutdown_service.core_lvl_shutdown().await;
        assert_eq!(not_dropped_services.len(), 0);
    }
}
