use std::cell::RefCell;

use backtrace::Backtrace;
use uuid::Uuid;

use crate::{
    infrastructure::{CompletionReason, FutureOutcome, SpawnFutureFlags},
    OPERATION_CANCELED_MSG,
};

thread_local! {
  pub static PANIC_STATE: RefCell<PanicState> = RefCell::new(PanicState::PanicHookIsNotSet);
}

pub static HOOK_IS_NOT_SET: &str = "Panic hook isn't set backtrace won't be logged";
pub static PANIC_DETECTED_IN_NO_PANIC_STATE: &str = "Panic detected but PanicState is NoPanic";

#[derive(Clone)]
pub enum PanicState {
    PanicHookIsNotSet,
    NoPanic,
    PanicHappened(String),
}

pub fn set_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let backtrace = Backtrace::new();

        let location = panic_info
            .location()
            .map(|x| x.to_string())
            .unwrap_or_else(|| "Failed to get location from PanicInfo".to_owned());

        let location_and_backtrace = format!("At {}\n{:?}", location, backtrace);

        PANIC_STATE.with(|panic_state| {
            *panic_state.borrow_mut() = PanicState::PanicHappened(location_and_backtrace)
        });
    }));

    PANIC_STATE.with(|panic_state| *panic_state.borrow_mut() = PanicState::NoPanic);
}

pub fn handle_future_panic(
    action_name: String,
    future_id: Uuid,
    flags: SpawnFutureFlags,
    graceful_shutdown_spawner: impl FnOnce(String, String),
    log_template: String,
    panic_message: String,
) -> FutureOutcome {
    let location_and_backtrace = PANIC_STATE.with(|panic_state| {
        let location_and_backtrace = match &*panic_state.borrow() {
            PanicState::PanicHookIsNotSet => {
                log::warn!("{HOOK_IS_NOT_SET}");
                None
            }
            PanicState::NoPanic => {
                log::error!("{PANIC_DETECTED_IN_NO_PANIC_STATE}");
                None
            }
            PanicState::PanicHappened(msg) => Some(msg.clone()),
        };

        *panic_state.borrow_mut() = PanicState::NoPanic;

        location_and_backtrace
    });

    let error_msg = match location_and_backtrace {
        Some(location_and_backtrace) => {
            format!("panic happened: {panic_message}. {location_and_backtrace}")
        }
        None => format!("panic happened: {panic_message}"),
    };

    if error_msg.contains(OPERATION_CANCELED_MSG)
        && !flags.intersects(SpawnFutureFlags::DENY_CANCELLATION)
    {
        log::warn!("{} was cancelled due to panic", log_template);
        return FutureOutcome::new(action_name, future_id, CompletionReason::Canceled);
    }

    log::error!("{}", error_msg);
    (graceful_shutdown_spawner)(log_template, panic_message);
    FutureOutcome::new(action_name, future_id, CompletionReason::Panicked)
}
