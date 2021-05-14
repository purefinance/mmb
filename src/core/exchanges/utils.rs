use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn get_current_milliseconds() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Unable to get time since unix epoch started")
        .as_millis()
}

pub(crate) fn try_invoke<T>(callback: &Option<Box<dyn Fn(T) -> Result<()>>>, arg: T) -> Result<()> {
    if let Some(callback) = callback {
        (callback)(arg)?;
    }

    Ok(())
}
