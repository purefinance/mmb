use anyhow::Result;
use awc::http::StatusCode;
use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;
use rust_decimal::*;
use rust_decimal_macros::dec;
use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use smallstr::SmallString;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::BTreeMap, time::Duration};
use thiserror::Error;

use crate::core::misc::derivative_position_info::DerivativePositionInfo;
use crate::core::orders::order::ExchangeOrderId;

pub type Price = Decimal;
pub type Amount = Decimal;
pub type SortedOrderData = BTreeMap<Price, Amount>;

type String4 = SmallString<[u8; 4]>;
type String12 = SmallString<[u8; 12]>;
type String16 = SmallString<[u8; 16]>;
type String15 = SmallString<[u8; 15]>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeIdParseError(String);

// unique user ID on the exchange
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct ExchangeAccountId {
    pub exchange_id: ExchangeId,

    /// Exchange account number
    pub account_number: u8,
}

impl ExchangeAccountId {
    #[inline]
    pub fn new(exchange_id: ExchangeId, account_number: u8) -> Self {
        ExchangeAccountId {
            exchange_id,
            account_number,
        }
    }

    pub fn to_string(&self) -> String {
        format!("{}", self)
    }
}

impl FromStr for ExchangeAccountId {
    type Err = ExchangeIdParseError;

    fn from_str(text: &str) -> std::result::Result<Self, Self::Err> {
        let regex = Regex::new(r"(^[A-Za-z]+)(\d+$)")
            .map_err(|err| ExchangeIdParseError(err.to_string()))?;

        let captures = regex
            .captures(text)
            .ok_or(ExchangeIdParseError("Invalid format".into()))?
            .iter()
            .collect_vec();

        let exchange_id = captures[1]
            .ok_or(ExchangeIdParseError("Invalid format".into()))?
            .as_str()
            .into();

        let number = captures[2]
            .ok_or(ExchangeIdParseError("Invalid format".into()))?
            .as_str()
            .parse()
            .map_err(|x| {
                ExchangeIdParseError(format!("Can't parse exchange account number: {}", x))
            })?;

        Ok(ExchangeAccountId::new(exchange_id, number))
    }
}

impl<'de> Deserialize<'de> for ExchangeAccountId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let deserialized = String::deserialize(deserializer)?;

        FromStr::from_str(&deserialized).map_err(|_| {
            de::Error::invalid_value(
                de::Unexpected::Str(&deserialized),
                &"ExchangeAccountId as a string with account number on the tail",
            )
        })
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
        write!(f, "{}{}", self.exchange_id.as_str(), self.account_number)
    }
}

// unique ID of exchange
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExchangeId(String15);

impl ExchangeId {
    #[inline]
    pub fn new(exchange_id: String15) -> Self {
        ExchangeId(exchange_id)
    }

    /// Extracts a string slice containing the entire string.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for ExchangeId {
    #[inline]
    fn from(value: &str) -> Self {
        ExchangeId(String15::from_str(value))
    }
}

impl Display for ExchangeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

/// Currency pair specific for exchange
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpecificCurrencyPair(String12);

impl SpecificCurrencyPair {
    #[inline]
    pub fn new(currency_code: String12) -> Self {
        SpecificCurrencyPair(currency_code)
    }

    /// Extracts a string slice containing the entire string.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for SpecificCurrencyPair {
    fn from(value: &str) -> Self {
        SpecificCurrencyPair(String12::from_str(value))
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
/// Currency in Exchange format, e.g. ETH, BTC
pub struct CurrencyId(String4);

impl CurrencyId {
    #[inline]
    pub fn new(currency_id: String4) -> Self {
        CurrencyId(currency_id)
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for CurrencyId {
    fn from(value: &str) -> Self {
        CurrencyId(String4::from_str(value))
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize, Hash)]
#[serde(transparent)]
/// Currency in unified format, e.g. eth, btc
pub struct CurrencyCode(String4);

impl CurrencyCode {
    #[inline]
    pub fn new(currency_code: String4) -> Self {
        CurrencyCode(currency_code.to_lowercase().into())
    }

    /// Extracts a string slice containing the entire string.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for CurrencyCode {
    fn from(value: &str) -> Self {
        CurrencyCode(String4::from_str(&value.to_lowercase()))
    }
}

impl Display for CurrencyCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Unified format currency pair for this framework
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CurrencyPair(String12);

impl CurrencyPair {
    #[inline]
    pub fn from_codes(base: &CurrencyCode, quote: &CurrencyCode) -> Self {
        CurrencyPair([base.as_str(), quote.as_str()].join("/").into()) // convention from ccxt
    }

    /// Extracts a string slice containing the entire string.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Display for CurrencyPair {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Exchange id and currency pair
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct TradePlace {
    pub exchange_id: ExchangeId,
    pub currency_pair: CurrencyPair,
}

impl TradePlace {
    pub fn new(exchange_id: ExchangeId, currency_pair: CurrencyPair) -> Self {
        TradePlace {
            exchange_id,
            currency_pair,
        }
    }
}

/// Exchange account id and currency pair
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Hash)]
pub struct TradePlaceAccount {
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
}

impl TradePlaceAccount {
    pub fn new(exchange_account_id: ExchangeAccountId, currency_pair: CurrencyPair) -> Self {
        TradePlaceAccount {
            exchange_account_id,
            currency_pair,
        }
    }

    pub fn trade_place(&self) -> TradePlace {
        TradePlace::new(
            self.exchange_account_id.exchange_id.clone(),
            self.currency_pair.clone(),
        )
    }
}

impl Serialize for TradePlaceAccount {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let trade_place_account = format!("{}|{}", self.exchange_account_id, self.currency_pair);
        serializer.serialize_str(&trade_place_account)
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

    pub(crate) fn unknown_error(message: &str) -> Self {
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

pub struct ClosedPosition {
    pub order: ClosedPositionOrder,
}

impl ClosedPosition {
    pub fn new(exchange_order_id: ExchangeOrderId, amount: Amount) -> Self {
        Self {
            order: ClosedPositionOrder {
                exchange_order_id,
                amount,
            },
        }
    }
}

pub struct ClosedPositionOrder {
    exchange_order_id: ExchangeOrderId,
    amount: Amount,
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
    pub base: Decimal, // REVIEW: what is this?
    pub time_stamp: u128,
    pub swap: Decimal, // REVIEW: what is this?
    pub pl: Amount,
    pub info: DerivativePositionInfo,
}

impl ActivePosition {
    pub fn new(info: DerivativePositionInfo) -> Self {
        Self {
            id: ActivePositionId::unique_id(),
            status: StatusCode::default(),
            base: dec!(0),
            time_stamp: 0,
            swap: dec!(0),
            pl: dec!(0),
            info,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    pub fn exchange_id_parse_correctly() {
        let exchange_account_id = "Binance0".parse::<ExchangeAccountId>();
        assert_eq!(
            exchange_account_id,
            Ok(ExchangeAccountId::new("Binance".into(), 0))
        );
    }

    #[test]
    pub fn exchange_id_parse_failed_exchange_id() {
        let exchange_account_id = "123".parse::<ExchangeAccountId>();
        assert_eq!(
            exchange_account_id,
            Err(ExchangeIdParseError("Invalid format".into()))
        )
    }

    #[test]
    pub fn exchange_id_parse_failed_missing_number() {
        let exchange_account_id = "Binance".parse::<ExchangeAccountId>();
        assert_eq!(
            exchange_account_id,
            Err(ExchangeIdParseError("Invalid format".into()))
        )
    }

    #[test]
    pub fn exchange_id_parse_failed_number_parsing() {
        let exchange_account_id = "binance256".parse::<ExchangeAccountId>();
        assert_eq!(
            exchange_account_id,
            Err(ExchangeIdParseError(
                r"Can't parse exchange account number: number too large to fit in target type"
                    .into()
            ))
        )
    }

    #[test]
    pub fn exchange_id_to_string() {
        let exchange_account_id = "Binance1".parse::<ExchangeAccountId>().expect("in test");
        let result = exchange_account_id.to_string();
        assert_eq!(result, "Binance1".to_string())
    }
}
