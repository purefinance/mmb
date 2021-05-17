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
                    .level_for("mmb", LevelFilter::Warn)
                    .level_for("mmb_lib", LevelFilter::Warn)
                    .chain(std::io::stdout()),
            )
            .chain(
                fern::Dispatch::new().level(LevelFilter::Trace).chain(
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open("log.txt")
                        .expect("Unable to open log file"),
                ),
            )
            .apply()
            .expect("Unable to set up logger");
    })
}
