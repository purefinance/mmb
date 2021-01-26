use chrono::Utc;
use log::LevelFilter;

pub fn init_logger() {
    fern::Dispatch::new()
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
                .chain(std::io::stdout())
        )
        .chain(
            fern::Dispatch::new()
                .level(LevelFilter::Warn)
                .chain(
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open("log.txt")
                        .unwrap()
                )
        )
        .apply()
        .unwrap();
}
