use anyhow::Result;
use itertools::Itertools;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::{impl_table_type, impl_table_type_raw};
use regex::Regex;
use rust_decimal::{Decimal, MathematicalOps};
use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Debug, Display, Formatter};
use std::str::FromStr;
use std::time::Duration;

// unique ID of exchange
impl_table_type!(ExchangeId, 8, u8);
// Currency in unified format, e.g. eth, btc
impl_table_type_raw!(CurrencyCode, 16, u16);
// Unified format currency pair for this mmb
impl_table_type_raw!(CurrencyPair, 16, u16);

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

// Currency pair specific for exchange
impl_table_type!(SpecificCurrencyPair, 16, u16);

// Currency in Exchange format, e.g. ETH, BTC
impl_table_type!(CurrencyId, 16, u16);

/// Exchange id and currency pair
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct MarketId {
    pub exchange_id: ExchangeId,
    pub currency_pair: CurrencyPair,
}

impl Display for MarketId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}|{}", self.exchange_id, self.currency_pair)
    }
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

impl Display for MarketAccountId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}|{}", self.exchange_account_id, self.currency_pair)
    }
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CurrencyPairCodes {
    pub base: CurrencyCode,
    pub quote: CurrencyCode,
}

impl CurrencyPairCodes {
    pub fn to_array(&self) -> [CurrencyCode; 2] {
        [self.base, self.quote]
    }
}

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

pub fn powi(value: Decimal, degree: i8) -> Decimal {
    value.powi(degree as i64)
}
