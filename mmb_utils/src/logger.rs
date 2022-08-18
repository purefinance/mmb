use log4rs::init_file;
use std::env;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::Once;

/// Function for getting path to log file. For `cargo run` it will be path to project directory. In other cases it will be `./`
/// if binary file were called with path that contain `rusttradingengine` dir the log will be there
fn get_log_file_path(log_file: &str) -> PathBuf {
    let path_to_bin = env::args().next().expect("Failed to get first arg");

    PathBuf::from(path_to_bin)
        .ancestors()
        .find(|ancestor| ancestor.ends_with("rusttradingengine"))
        .unwrap_or_else(|| Path::new("./"))
        .join(log_file)
}

pub fn init_logger_file_named(log_file: &str) {
    if env::var("MMB_NO_LOGS").is_ok() {
        return;
    }

    let path = get_log_file_path(log_file);
    static INIT_LOGGER: Once = Once::new();

    INIT_LOGGER.call_once(|| {
        init_logger();
    });

    print_info(format_args!(
        "Logger has been initialized all logs will be stored here: {}",
        path.to_str().expect("Unable build logger file path"),
    ));
}

fn init_logger() {
    let config_path = get_log_config_path();

    let mut deserializers = log4rs_logstash::config::deserializers();
    deserializers.insert("outer_modules_filter", outer_modules_filter::Deserializer);

    init_file(config_path, deserializers).expect("Unable to set up logger");
}

fn get_log_config_path() -> PathBuf {
    let mut config_path: PathBuf = "./log_config/config.yaml".into();
    if !Path::exists(&config_path) {
        let cur_dir = env::current_dir().expect("unable get current directory");
        config_path = cur_dir
            .ancestors()
            .find(|x| Path::exists(&x.join("log_config/config.yaml")))
            .unwrap_or_else(|| {
                panic!(
                    "unable find log config 'log_config/config.yaml' for current dir:{}",
                    cur_dir.display()
                )
            })
            .join("log_config/config.yaml");
    }
    config_path
}

pub fn print_info<T>(msg: T)
where
    T: Display,
{
    log::info!("{msg}");
    println!("{msg}");
}

pub mod outer_modules_filter {
    use anyhow::Result;
    use log::{Level, Record};
    use log4rs::config::{Deserialize, Deserializers};
    use log4rs::filter::{Filter as Log4RsFilter, Response};

    #[derive(serde::Deserialize)]
    pub struct OuterModulesFilterConfig {}
    #[derive(Debug)]
    pub struct Filter;

    impl Filter {
        pub fn new() -> Filter {
            Filter
        }
    }

    impl Log4RsFilter for Filter {
        fn filter(&self, record: &Record) -> Response {
            if record.level() <= Level::Warn {
                return Response::Accept;
            }

            let ignore_modules = [
                "actix_broker",
                "actix_codec",
                "actix_http",
                "actix_server",
                "actix_http",
                "actix_web",
                "want",
                "mio",
                "rustls",
                "tungstenite",
                "tokio_tungstenite",
                "tokio_postgres",
            ];

            if ignore_modules
                .iter()
                .any(|m| record.target().starts_with(m))
            {
                Response::Reject
            } else {
                Response::Accept
            }
        }
    }

    pub struct Deserializer;

    impl Deserialize for Deserializer {
        type Trait = dyn Log4RsFilter;

        type Config = OuterModulesFilterConfig;

        fn deserialize(
            &self,
            _config: OuterModulesFilterConfig,
            _: &Deserializers,
        ) -> Result<Box<dyn Log4RsFilter>> {
            Ok(Box::new(Filter::new()))
        }
    }
}
