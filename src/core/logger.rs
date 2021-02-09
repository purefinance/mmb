use chrono::Utc;
use log::LevelFilter;
use std::sync::Once;

pub fn init_logger() {
    static INIT_LOGGER: Once = Once::new();

    INIT_LOGGER.call_once(|| {
        let _ = fern::Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!(
                    "[{}][{}][{}] {}",
                    Utc::now().format("%Y-%m-%d %H:%M:%S,%3f"),
                    record.level(),
                    record.target(),
                    message
                ))
            })
            .chain(
                fern::Dispatch::new()
                    .level(LevelFilter::Warn)
                    .level_for("mmb", LevelFilter::Trace)
                    .level_for("mmb_lib", LevelFilter::Trace)
                    .chain(std::io::stdout()),
            )
            .chain(
                fern::Dispatch::new().level(LevelFilter::Warn).chain(
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open("log.txt")
                        .unwrap(),
                ),
            )
            .apply()
            .unwrap();
    })
}
