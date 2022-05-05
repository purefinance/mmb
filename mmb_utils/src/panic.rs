use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt::Display;

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
        let location: &dyn Display = match panic_info.location() {
            Some(location) => location,
            None => &"Failed to get location from PanicInfo",
        };

        let location_and_backtrace = format!("At {}\n{:?}", location, Backtrace::new());

        PANIC_STATE
            .try_with(|panic_state| {
                *panic_state.borrow_mut() = PanicState::PanicHappened(location_and_backtrace);
            })
            .unwrap_or_else(|_| {
                log::error!("Unable write `location_and_backtrace` to `PANIC_STATE`");
            });
    }));

    PANIC_STATE.with(|panic_state| *panic_state.borrow_mut() = PanicState::NoPanic);
}

pub fn handle_future_panic(
    action_name: String,
    future_id: Uuid,
    flags: SpawnFutureFlags,
    graceful_shutdown_spawner: impl FnOnce(String, &str),
    log_template: String,
    panic_message: &str,
) -> FutureOutcome {
    if !flags.intersects(SpawnFutureFlags::DENY_CANCELLATION)
        && panic_message.contains(OPERATION_CANCELED_MSG)
    {
        log::warn!("{} was cancelled due to panic", log_template);
        return FutureOutcome::new(action_name, future_id, CompletionReason::Canceled);
    }

    let location_and_backtrace = PANIC_STATE
        .try_with(
            |panic_state| match panic_state.replace(PanicState::NoPanic) {
                PanicState::PanicHookIsNotSet => {
                    log::warn!("{HOOK_IS_NOT_SET}");
                    Cow::Borrowed("")
                }
                PanicState::NoPanic => {
                    log::error!("{PANIC_DETECTED_IN_NO_PANIC_STATE}");
                    Cow::Borrowed("")
                }
                PanicState::PanicHappened(msg) => Cow::Owned(msg),
            },
        )
        .unwrap_or(Cow::Borrowed(
            "Unable get location and backtrace for error.",
        ));

    log::error!("panic happened: {panic_message}. {location_and_backtrace}");
    (graceful_shutdown_spawner)(log_template, panic_message);
    FutureOutcome::new(action_name, future_id, CompletionReason::Panicked)
}
