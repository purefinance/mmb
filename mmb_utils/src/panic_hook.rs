use backtrace::Backtrace;

pub fn set_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let backtrace = Backtrace::new();

        let location = panic_info.location().unwrap();

        let panic_message = match panic_info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match panic_info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "without readable message",
            },
        };

        log::error!(
            "panic happened at {}: {}\n{:?}",
            location,
            panic_message,
            backtrace
        )
    }));
}
