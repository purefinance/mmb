use crate::exchanges::events::ExchangeEvent;
use crate::lifecycle::app_lifetime_manager::AppLifetimeManager;
use anyhow::{anyhow, Result};
use hyper::StatusCode;
use itertools::Itertools;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::{impl_table_type, impl_table_type_raw};
use once_cell::sync::Lazy;
use regex::Regex;
use rust_decimal::*;
use rust_decimal_macros::dec;
use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use smallstr::SmallString;
use std::fmt::{self, Debug, Display, Formatter};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::BTreeMap, time::Duration};
use thiserror::Error;
use tokio::sync::broadcast;

use crate::misc::derivative_position::DerivativePosition;
use crate::orders::order::ExchangeOrderId;

pub type Price = Decimal;
pub type Amount = Decimal;
pub type SortedOrderData = BTreeMap<Price, Amount>;

type String16 = SmallString<[u8; 16]>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeIdParseError(String);

// unique user ID on the exchange
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct ExchangeAccountId {
    pub exchange_id: ExchangeId,

    /// Exchange account number
    pub account_number: u8,
}

impl ExchangeAccountId {
    #[inline]
    pub fn new(exchange_id: impl Into<ExchangeId>, account_number: u8) -> Self {
        ExchangeAccountId {
            exchange_id: exchange_id.into(),
            account_number,
        }
    }
}

impl FromStr for ExchangeAccountId {
    type Err = ExchangeIdParseError;

    fn from_str(text: &str) -> std::result::Result<Self, Self::Err> {
        let regex = Regex::new(r"(^[A-Za-z0-9\-\.]+)_(\d+$)")
            .map_err(|err| ExchangeIdParseError(err.to_string()))?;

        let captures = regex
            .captures(text)
            .ok_or_else(|| ExchangeIdParseError("Invalid format".into()))?
            .iter()
            .collect_vec();

        let exchange_id = captures[1]
            .ok_or_else(|| ExchangeIdParseError("Invalid format".into()))?
            .as_str();

        let number = captures[2]
            .ok_or_else(|| ExchangeIdParseError("Invalid format".into()))?
            .as_str()
            .parse()
            .map_err(|x| {
                ExchangeIdParseError(format!("Can't parse exchange account number: {}", x))
            })?;

        Ok(ExchangeAccountId::new(exchange_id, number))
    }
}

struct ExchangeAccountIdVisitor;

impl<'de> Visitor<'de> for ExchangeAccountIdVisitor {
    type Value = ExchangeAccountId;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "string for ExchangeAccountId")
    }

    fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        v.parse().map_err(|_| {
            de::Error::invalid_value(
                de::Unexpected::Str(v),
                &"ExchangeAccountId as a string with account number on the tail that separated by a '_' character",
            )
        })
    }
}

impl<'de> Deserialize<'de> for ExchangeAccountId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(ExchangeAccountIdVisitor)
    }
}

impl Serialize for ExchangeAccountId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let id_as_str = self.to_string();
        serializer.serialize_str(&id_as_str)
    }
}

impl Display for ExchangeAccountId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}_{}", self.exchange_id.as_str(), self.account_number)
    }
}

impl Debug for ExchangeAccountId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}_{}", self.exchange_id.as_str(), self.account_number)
    }
}

// unique ID of exchange
impl_table_type!(ExchangeId, 8);

// Currency pair specific for exchange
impl_table_type!(SpecificCurrencyPair, 16);

// Currency in Exchange format, e.g. ETH, BTC
impl_table_type!(CurrencyId, 16);

// Currency in unified format, e.g. eth, btc
impl_table_type_raw!(CurrencyCode, 16);

impl CurrencyCode {
    pub fn new(currency_code: &str) -> Self {
        let currency_code = currency_code.to_lowercase();
        Self(SHARED_CURRENCY_CODE.add_or_get(&currency_code))
    }
}

impl From<&str> for CurrencyCode {
    fn from(value: &str) -> Self {
        CurrencyCode::new(value)
    }
}

pub struct CurrencyPairCodes {
    pub base: CurrencyCode,
    pub quote: CurrencyCode,
}

// Unified format currency pair for this mmb
impl_table_type_raw!(CurrencyPair, 16);

impl CurrencyPair {
    pub fn from_codes(base: CurrencyCode, quote: CurrencyCode) -> Self {
        // convention into ccxt format
        Self(SHARED_CURRENCY_PAIR.add_or_get(&[base.as_str(), quote.as_str()].join("/")))
    }

    pub fn to_codes(&self) -> CurrencyPairCodes {
        let (base, quote) = self
            .as_str()
            .split_once('/')
            .with_expect(|| format!("Failed to get base and quote value from CurrencyPair {self}"));

        CurrencyPairCodes {
            base: base.into(),
            quote: quote.into(),
        }
    }
}

/// Exchange id and currency pair
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct MarketId {
    pub exchange_id: ExchangeId,
    pub currency_pair: CurrencyPair,
}

impl MarketId {
    pub fn new(exchange_id: ExchangeId, currency_pair: CurrencyPair) -> Self {
        MarketId {
            exchange_id,
            currency_pair,
        }
    }
}

/// Exchange account id and currency pair
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Hash)]
pub struct MarketAccountId {
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
}

impl MarketAccountId {
    pub fn new(exchange_account_id: ExchangeAccountId, currency_pair: CurrencyPair) -> Self {
        MarketAccountId {
            exchange_account_id,
            currency_pair,
        }
    }

    pub fn market_id(&self) -> MarketId {
        MarketId::new(self.exchange_account_id.exchange_id, self.currency_pair)
    }
}

impl Serialize for MarketAccountId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let market_account_id = format!("{}|{}", self.exchange_account_id, self.currency_pair);
        serializer.serialize_str(&market_account_id)
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Error)]
#[error("Type: {error_type:?} Message: {message} Code {code:?}")]
pub struct ExchangeError {
    pub error_type: ExchangeErrorType,
    pub message: String,
    pub code: Option<i64>,
}

impl ExchangeError {
    pub fn new(error_type: ExchangeErrorType, message: String, code: Option<i64>) -> Self {
        Self {
            error_type,
            message,
            code,
        }
    }

    pub fn parsing_error(message: String) -> Self {
        ExchangeError::new(ExchangeErrorType::ParsingError, message, None)
    }
    pub fn unknown_error(message: &str) -> Self {
        Self {
            error_type: ExchangeErrorType::Unknown,
            message: message.to_owned(),
            code: None,
        }
    }

    pub fn set_pending(&mut self, pending_time: Duration) {
        self.error_type = ExchangeErrorType::PendingError(pending_time);
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum ExchangeErrorType {
    Unknown,
    SendError,
    RateLimit,
    OrderNotFound,
    OrderCompleted,
    InsufficientFunds,
    InvalidOrder,
    Authentication,
    ParsingError,
    PendingError(Duration),
    ServiceUnavailable,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum RestRequestError {
    IsInProgress,
    Status(StatusCode),
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RestRequestOutcome {
    pub content: String,
    pub status: StatusCode,
}

impl RestRequestOutcome {
    pub fn new(content: String, status: StatusCode) -> Self {
        Self { content, status }
    }
}

pub type RestRequestResult = std::result::Result<String, RestRequestError>;

pub trait ToStdExpected {
    fn to_std_expected(&self) -> Duration;
}

impl ToStdExpected for chrono::Duration {
    /// Converts chrono::Duration to std::time::Duration.
    ///
    /// # Panics
    /// Panic only on negative delay
    fn to_std_expected(&self) -> Duration {
        self.to_std().with_expect(|| {
            format!(
                "Unable to convert value = {} from chrono::Duration to std::time::Duration",
                self
            )
        })
    }
}

pub struct ClosedPosition {
    _exchange_order_id: ExchangeOrderId,
    _amount: Amount,
}

impl ClosedPosition {
    pub fn new(_exchange_order_id: ExchangeOrderId, _amount: Amount) -> Self {
        Self {
            _exchange_order_id,
            _amount,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActivePositionId(String16);

static ACTIVE_POSITION_ID_COUNTER: Lazy<AtomicU64> = Lazy::new(|| {
    AtomicU64::new(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get system time since UNIX_EPOCH")
            .as_secs(),
    )
});

impl ActivePositionId {
    pub fn unique_id() -> Self {
        let new_id = ACTIVE_POSITION_ID_COUNTER.fetch_add(1, Ordering::AcqRel);
        ActivePositionId(new_id.to_string().into())
    }

    pub fn new(client_order_id: String16) -> Self {
        ActivePositionId(client_order_id)
    }

    /// Extracts a string slice containing the entire string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Extracts a string slice containing the entire string.
    pub fn as_mut_str(&mut self) -> &mut str {
        self.0.as_mut_str()
    }
}

impl From<&str> for ActivePositionId {
    fn from(value: &str) -> Self {
        ActivePositionId(String16::from_str(value))
    }
}

impl Display for ActivePositionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct ActivePosition {
    pub id: ActivePositionId,
    pub status: StatusCode,
    pub time_stamp: u128,
    pub pl: Amount,
    pub derivative: DerivativePosition,
}

impl ActivePosition {
    pub fn new(derivative: DerivativePosition) -> Self {
        Self {
            id: ActivePositionId::unique_id(),
            status: StatusCode::default(),
            time_stamp: 0,
            pl: dec!(0),
            derivative,
        }
    }
}

pub fn send_event(
    events_channel: &broadcast::Sender<ExchangeEvent>,
    lifetime_manager: Arc<AppLifetimeManager>,
    id: ExchangeAccountId,
    event: ExchangeEvent,
) -> Result<()> {
    match events_channel.send(event) {
        Ok(_) => Ok(()),
        Err(error) => {
            let msg = format!("Unable to send exchange event in {}: {}", id, error);
            log::error!("{}", msg);
            lifetime_manager.spawn_graceful_shutdown(&msg);
            Err(anyhow!(msg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod basic_exchange_id {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        pub fn just_create() {
            let exchange_id: ExchangeId = ExchangeId::new("Binance");
            let copied = exchange_id;

            assert_eq!(copied, exchange_id);
            assert_eq!(exchange_id.as_str(), "Binance");
        }

        #[test]
        pub fn same_2_id() {
            let exchange_id1 = ExchangeId::new("Binance");
            let exchange_id2 = ExchangeId::new("Binance");
            assert_eq!(exchange_id2, exchange_id1);
            assert_eq!(exchange_id1.as_str(), exchange_id2.as_str());
        }

        #[test]
        pub fn different_2_id() {
            let exchange_id1 = ExchangeId::new("Binance");
            let exchange_id2 = ExchangeId::new("Bitmex");
            assert_ne!(exchange_id2, exchange_id1);
            assert_ne!(exchange_id1.as_str(), exchange_id2.as_str());
        }

        #[test]
        pub fn deserialization() {
            #[derive(Deserialize)]
            struct TestValue {
                id: ExchangeId,
            }

            let input = r#"{"id":"TestExchangeId"}"#;

            let deserialized: TestValue = serde_json::from_str(input).expect("in test");

            assert_eq!(deserialized.id.as_str(), "TestExchangeId");
        }

        #[test]
        pub fn serialization() {
            #[derive(Serialize)]
            struct TestValue {
                id: ExchangeId,
            }

            let value = TestValue {
                id: ExchangeId::new("TestExchangeId"),
            };

            let serialized = serde_json::to_string(&value).expect("in test");

            assert_eq!(serialized, r#"{"id":"TestExchangeId"}"#);
        }
    }

    mod parse_exchange_account_id {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        pub fn correct() {
            let exchange_account_id = "Binance.test-hello-world111_0".parse::<ExchangeAccountId>();
            assert_eq!(
                exchange_account_id,
                Ok(ExchangeAccountId::new("Binance.test-hello-world111", 0))
            );
        }

        #[test]
        pub fn failed_because_no_exchange_name() {
            let exchange_account_id = "123".parse::<ExchangeAccountId>();
            assert_eq!(
                exchange_account_id,
                Err(ExchangeIdParseError("Invalid format".into()))
            )
        }

        #[test]
        pub fn failed_because_missing_number() {
            let exchange_account_id = "Binance".parse::<ExchangeAccountId>();
            assert_eq!(
                exchange_account_id,
                Err(ExchangeIdParseError("Invalid format".into()))
            )
        }

        #[test]
        pub fn failed_because_invalid_number() {
            let exchange_account_id = "binance_256".parse::<ExchangeAccountId>();
            assert_eq!(
                exchange_account_id,
                Err(ExchangeIdParseError(
                    r"Can't parse exchange account number: number too large to fit in target type"
                        .into()
                ))
            )
        }
    }

    mod to_string_exchange_account_id {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        pub fn simple() {
            let exchange_account_id = "Binance_1".parse::<ExchangeAccountId>().expect("in test");
            let result = exchange_account_id.to_string();
            assert_eq!(result, "Binance_1".to_string())
        }
    }
}
