use std::fmt::{self, Display, Formatter};
use serde::{Serialize, Deserialize};
use smallstr::SmallString;
use std::str::FromStr;
use regex::Regex;
use itertools::Itertools;

type String4 = SmallString<[u8; 4]>;
type String12 = SmallString<[u8; 12]>;
type String16 = SmallString<[u8; 16]>;
type String15 = SmallString<[u8; 15]>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeIdParseError(String);


#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExchangeId {
    pub exchange_name: ExchangeName,

    /// Exchange account number
    pub number: u8
}

impl ExchangeId {
    #[inline]
    pub fn new(exchange_name: ExchangeName, number: u8) -> Self {
        ExchangeId { exchange_name, number }
    }

    pub fn to_string(&self) -> String {
        format!("{}", self)
    }
}

impl FromStr for ExchangeId {
    type Err = ExchangeIdParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let regex = Regex::new(r"(^[[:alpha:]]+)(\d+$)").unwrap();
        let captures = regex.captures(text)
            .ok_or(ExchangeIdParseError("Invalid format".into()))?
            .iter()
            .collect_vec();

        let exchange_name = captures[1]
            .ok_or(ExchangeIdParseError("Invalid format".into()))?
            .as_str()
            .into();


        let number = captures[2]
            .ok_or(ExchangeIdParseError("Invalid format".into()))?
            .as_str()
            .parse()
            .map_err(|x| ExchangeIdParseError(format!("Can't parse exchange account number: {}", x)))?;

        Ok(ExchangeId::new(exchange_name, number))
    }
}

impl Display for ExchangeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f,"{}{}", self.exchange_name.as_str(), self.number)
    }
}


#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExchangeName(String15);

impl ExchangeName {
    #[inline]
    pub fn new(exchange_name: String15) -> Self {
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
        ExchangeName(String15::from_str(value))
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
    pub fn as_str(&self) -> &str { self.0.as_str() }
}

impl From<&str> for CurrencyCode {
    fn from(value: &str) -> Self {
        CurrencyCode(String4::from_str(value))
    }
}

/// Unified format currency pair for this framework
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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
    pub currency_code_pair: CurrencyCodePair
}

impl TradePlace {
    pub fn new(exchange_id: ExchangeId, currency_code_pair: CurrencyCodePair) -> Self {
        TradePlace { exchange_id, currency_code_pair }
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    pub fn exchange_id_parse_correctly() {
        let exchange_id = "Binance0".parse::<ExchangeId>();
        assert_eq!(exchange_id, Ok(ExchangeId::new("Binance".into(), 0)));
    }

    #[test]
    pub fn exchange_id_parse_failed_exchange_name() {
        let exchange_id = "123".parse::<ExchangeId>();
        assert_eq!(exchange_id, Err(ExchangeIdParseError("Invalid format".into())))
    }

    #[test]
    pub fn exchange_id_parse_failed_missing_number() {
        let exchange_id = "binance".parse::<ExchangeId>();
        assert_eq!(exchange_id, Err(ExchangeIdParseError("Invalid format".into())))
    }

    #[test]
    pub fn exchange_id_parse_failed_number_parsing() {
        let exchange_id = "binance256".parse::<ExchangeId>();
        assert_eq!(exchange_id, Err(ExchangeIdParseError(r"Can't parse exchange account number: number too large to fit in target type".into())))
    }

    #[test]
    pub fn exchange_id_to_string() {
        let exchange_id = "Binance1".parse::<ExchangeId>().unwrap();
        let result = exchange_id.to_string();
        assert_eq!(result, "Binance1".to_string())
    }
}