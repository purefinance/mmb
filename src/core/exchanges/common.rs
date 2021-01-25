use serde::{Deserialize, Serialize};
use smallstr::SmallString;
use std::fmt::{self, Display, Formatter};

use chrono::Utc;

pub type DateTime = chrono::DateTime<Utc>;

type String4 = SmallString<[u8; 4]>;
type String12 = SmallString<[u8; 12]>;
type String16 = SmallString<[u8; 16]>;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
// TODO ExchangeAccountId
pub struct ExchangeId(String16);

impl ExchangeId {
    #[inline]
    pub fn new(exchange_id: String16) -> Self {
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
        ExchangeId(String16::from_str(value))
    }
}

impl Display for ExchangeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
// TODO ExchangeId
pub struct ExchangeName(String16);

impl ExchangeName {
    #[inline]
    pub fn new(exchange_name: String16) -> Self {
        ExchangeName(exchange_name)
    }

    /// Extracts a string slice containing the entire string.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for ExchangeName {
    #[inline]
    fn from(value: &str) -> Self {
        ExchangeName(String16::from_str(value))
    }
}

/// Currency pair specific for exchange
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CurrencyPair(String12);

impl CurrencyPair {
    #[inline]
    pub fn new(currency_code: String12) -> Self {
        CurrencyPair(currency_code)
    }

    /// Extracts a string slice containing the entire string.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for CurrencyPair {
    fn from(value: &str) -> Self {
        CurrencyPair(String12::from_str(value))
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
// TODO CurrencyPairCode
#[serde(transparent)]
pub struct CurrencyCodePair(String12);

impl CurrencyCodePair {
    #[inline]
    pub fn new(currency_code: String12) -> Self {
        CurrencyCodePair(currency_code)
    }

    #[inline]
    pub fn from_currency_codes(base: CurrencyCode, quote: CurrencyCode) -> Self {
        CurrencyCodePair([base.as_str(), quote.as_str()].join("/").into()) // convention from ccxt
    }

    /// Extracts a string slice containing the entire string.
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Exchange id and currency code pair
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TradePlace {
    pub exchange_id: ExchangeId,
    pub currency_code_pair: CurrencyCodePair,
}

impl TradePlace {
    pub fn new(exchange_id: ExchangeId, currency_code_pair: CurrencyCodePair) -> Self {
        TradePlace {
            exchange_id,
            currency_code_pair,
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

pub enum RestRequestError {
    IsInProgress,
    HttpStatusCode(u32),
}

pub type RestRequestResult = Result<String, RestRequestError>;

pub struct RestErrorDescription {
    message: String,
    code: u32,
}

impl RestErrorDescription {
    pub fn new(message: String, code: u32) -> Self {
        RestErrorDescription { message, code }
    }
}

// TODO Как бы назвать правильно эту пару (ID обменника + валютная пара) ExchangerCurrencyPairState?
#[derive(PartialEq, Eq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct ExchangeNameSymbol {
    exchange_name: ExchangeName,
    currency_code_pair: CurrencyCodePair,
}

impl ExchangeNameSymbol {
    pub fn new(exchange_name: ExchangeName, currency_code_pair: CurrencyCodePair) -> Self {
        Self {
            exchange_name,
            currency_code_pair,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ExchangeIdSymbol {
    exchange_id: ExchangeId,
    exchange_name: ExchangeName,
    currency_code_pair: CurrencyCodePair,
}

impl ExchangeIdSymbol {
    pub fn new(
        exchange_id: ExchangeId,
        exchange_name: ExchangeName,
        currency_code_pair: CurrencyCodePair,
    ) -> Self {
        Self {
            exchange_id,
            exchange_name,
            currency_code_pair,
        }
    }

    pub fn get_exchanger_currency_state(&self) -> ExchangeNameSymbol {
        ExchangeNameSymbol {
            currency_code_pair: self.currency_code_pair.clone(),
            exchange_name: self.exchange_name.clone(),
        }
    }
}
