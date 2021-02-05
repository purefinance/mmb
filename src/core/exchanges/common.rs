use awc::http::StatusCode;
use itertools::Itertools;
use regex::Regex;
use rust_decimal::*;
use serde::{Deserialize, Serialize};
use smallstr::SmallString;
use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use chrono::Utc;

pub type DateTime = chrono::DateTime<Utc>;

pub type Price = Decimal;
pub type Amount = Decimal;
pub type SortedOrderData = BTreeMap<Price, Amount>;

type String4 = SmallString<[u8; 4]>;
type String12 = SmallString<[u8; 12]>;
type String16 = SmallString<[u8; 16]>;
type String15 = SmallString<[u8; 15]>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeIdParseError(String);

#[derive(Debug, Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
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

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let regex = Regex::new(r"(^[[:alpha:]]+)(\d+$)").unwrap();
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

impl Display for ExchangeAccountId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.exchange_id.as_str(), self.account_number)
    }
}

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

/// Currency pair specific for exchange
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CurrencyCode(String4);

impl CurrencyCode {
    #[inline]
    pub fn new(currency_code: String4) -> Self {
        CurrencyCode(currency_code)
    }

    /// Extracts a string slice containing the entire string.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for CurrencyCode {
    fn from(value: &str) -> Self {
        CurrencyCode(String4::from_str(value))
    }
}

/// Unified format currency pair for this framework
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CurrencyPair(String12);

impl CurrencyPair {
    #[inline]
    pub fn new(currency_code: String12) -> Self {
        CurrencyPair(currency_code)
    }

    #[inline]
    pub fn from_currency_codes(base: CurrencyCode, quote: CurrencyCode) -> Self {
        CurrencyPair([base.as_str(), quote.as_str()].join("/").into()) // convention from ccxt
    }

    /// Extracts a string slice containing the entire string.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Exchange id and currency pair
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(PartialEq, Eq, Clone, Hash, Debug, Serialize, Deserialize)]
pub struct ExchangeIdCurrencyPair {
    exchange_account_id: ExchangeAccountId,
    currency_pair: CurrencyPair,
}

impl ExchangeIdCurrencyPair {
    pub fn new(exchange_account_id: ExchangeAccountId, currency_pair: CurrencyPair) -> Self {
        Self {
            exchange_account_id,
            currency_pair,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub enum ExchangeErrorType {
    Unknown,
    RateLimit,
    OrderNotFound,
    OrderCompleted,
    InsufficientFunds,
    InvalidOrder,
    Authentication,
    ParsingError,
    PendingError,
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

pub type RestRequestResult = Result<String, RestRequestError>;

pub struct RestErrorDescription {
    pub message: String,
    pub code: i64,
}

impl RestErrorDescription {
    pub fn new(message: String, code: i64) -> Self {
        RestErrorDescription { message, code }
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
        let exchange_account_id = "binance".parse::<ExchangeAccountId>();
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
        let exchange_account_id = "Binance1".parse::<ExchangeAccountId>().unwrap();
        let result = exchange_account_id.to_string();
        assert_eq!(result, "Binance1".to_string())
    }
}
