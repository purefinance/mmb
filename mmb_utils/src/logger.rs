use anyhow::{bail, Context, Result};
use log4rs::config::Deserializers;
use log4rs::init_file;
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::fmt::{Debug, Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::{env, fs};

pub fn init_logger() {
    if env::var("MMB_NO_LOGS").is_ok() {
        return;
    }

    static INIT_LOGGER: Once = Once::new();
    INIT_LOGGER.call_once(|| {
        init_file(get_log_config_path(), get_deserializers()).expect("Unable to set up logger");
    });

    let loggers = get_loggers().expect("Failed to get logger info");
    print_info(format_args!(
        "Logger has been initialized all logs will be stored here: {loggers:?}"
    ));
}

struct Loggers {
    info: Vec<LoggerType>,
}

impl Debug for Loggers {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut iter = self.info.iter();

        if let Some(item) = iter.next() {
            write!(f, "{item:?}")?;
        }

        for item in iter {
            write!(f, ", {item:?}")?;
        }

        Ok(())
    }
}

enum LoggerType {
    Stdout,
    File(String),
}

impl LoggerType {
    fn as_str(&self) -> String {
        match self {
            Self::Stdout => "stdout".to_owned(),
            Self::File(name) => env::current_dir()
                .expect("Failed to get current directory path")
                .join(name)
                .display()
                .to_string(),
        }
    }
}

impl Debug for LoggerType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

fn get_loggers() -> Result<Loggers> {
    let file = fs::File::open(get_log_config_path()).context("Failed to open log config file")?;
    let mut config: BTreeMap<String, Value> =
        serde_yaml::from_reader(file).context("Failed to parse raw log config")?;

    let mut root: BTreeMap<String, Value> =
        serde_yaml::from_value(config.remove("root").context("Missing config root")?)
            .context("Failed to parse config root")?;
    let root_appenders: Vec<String> =
        serde_yaml::from_value(root.remove("appenders").context("Missing root appenders")?)
            .context("Failed to parse root appenders")?;
    let mut config_appenders: BTreeMap<String, BTreeMap<String, Value>> = serde_yaml::from_value(
        config
            .remove("appenders")
            .context("Missing config appenders")?,
    )
    .context("Failed to parse config appenders")?;

    let mut loggers = Vec::with_capacity(2);
    for appender in root_appenders {
        if let Some(mut value) = config_appenders.remove(&appender) {
            let kind: String = serde_yaml::from_value(
                value
                    .remove("kind")
                    .context("Missing kind field in config appender")?,
            )
            .context("Failed to parse config appender kind")?;

            match kind.as_str() {
                "console" => loggers.push(LoggerType::Stdout),
                "file" => {
                    let path: String = serde_yaml::from_value(
                        value
                            .remove("path")
                            .context("Missing path field in file log config")?,
                    )
                    .context("Failed to parse log file path")?;
                    loggers.push(LoggerType::File(path));
                }
                _ => bail!("Unknown logger type"),
            }
        }
    }

    Ok(Loggers { info: loggers })
}

fn get_deserializers() -> Deserializers {
    let mut deserializers = log4rs_logstash::config::deserializers();
    deserializers.insert("outer_modules_filter", outer_modules_filter::Deserializer);

    deserializers
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
    #[derive(Debug, Default)]
    pub struct Filter;

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
            Ok(Box::new(Filter::default()))
        }
    }
}
