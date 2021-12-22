use chrono::Utc;
use log::LevelFilter;
use std::env;
use std::sync::Once;

fn get_log_file_path(log_file: &str) -> String {
    let args = std::env::args()
        .next()
        .expect("Failed to get args")
        .to_string();

    let main_dir = "rusttradingengine/";
    let mut path = match args.rsplit_once(main_dir) {
        Some((path, _)) => {
            let mut result = path.to_string();
            result.push_str(main_dir);
            result
        }
        None => "./".into(),
    };
    path.push_str(log_file);

    path
}

pub fn init_logger() {
    init_logger_file_named("log.txt")
}

pub fn init_logger_file_named(log_file: &str) {
    if let Ok(_) = env::var("MMB_NO_LOGS") {
        return;
    }

    let path = get_log_file_path(log_file);
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
                    .level_for("mmb_core", LevelFilter::Warn)
                    .chain(std::io::stdout()),
            )
            .chain(
                fern::Dispatch::new()
                    .level(LevelFilter::Trace)
                    .level_for("actix_tls", LevelFilter::Warn)
                    .level_for("rustls", LevelFilter::Warn)
                    .level_for("actix_codec", LevelFilter::Warn)
                    .chain(
                        std::fs::OpenOptions::new()
                            .write(true)
                            .create(true)
                            .truncate(true)
                            .open(path)
                            .expect("Unable to open log file"),
                    ),
            )
            .apply()
            .expect("Unable to set up logger");
    })
}
