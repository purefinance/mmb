use mmb_domain::market::SpecificCurrencyPair;
use mmb_domain::order::snapshot::{Amount, ClientOrderId, ExchangeOrderId, OrderSide, Price};
use mmb_utils::DateTime;
use rust_decimal::Decimal;
use serde::{de, Deserialize, Deserializer};
use std::fmt;
use std::fmt::Debug;

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexSymbol<'a> {
    #[serde(rename = "symbol")]
    pub(crate) id: &'a str,
    #[serde(rename = "underlying")]
    pub(crate) base_id: &'a str,
    #[serde(rename = "quoteCurrency")]
    pub(crate) quote_id: &'a str,
    pub(crate) state: &'a str,
    #[serde(rename = "tickSize")]
    pub(crate) price_tick: Option<Decimal>,
    #[serde(rename = "lotSize")]
    pub(crate) amount_tick: Option<Decimal>,
    #[serde(rename = "maxPrice")]
    pub(crate) max_price: Option<Price>,
    #[serde(rename = "maxOrderQty")]
    pub(crate) max_amount: Option<Amount>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexOrderInfo<'a> {
    #[serde(rename = "symbol")]
    pub(crate) specific_currency_pair: SpecificCurrencyPair,
    #[serde(rename = "orderID")]
    pub(crate) exchange_order_id: ExchangeOrderId,
    #[serde(rename = "clOrdID")]
    pub(crate) client_order_id: ClientOrderId,
    pub(crate) price: Option<Price>,
    #[serde(rename = "avgPx")]
    pub(crate) average_fill_price: Option<Price>,
    #[serde(rename = "orderQty")]
    pub(crate) amount: Option<Amount>,
    #[serde(rename = "cumQty")]
    pub(crate) filled_amount: Option<Amount>,
    #[serde(rename = "ordStatus")]
    pub(crate) status: &'a str,
    pub(crate) side: OrderSide,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexOrderBookInsert {
    pub(crate) symbol: SpecificCurrencyPair,
    pub(crate) id: u64,
    pub(crate) side: OrderSide,
    pub(crate) size: Amount,
    pub(crate) price: Price,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexOrderBookDelete {
    pub(crate) symbol: SpecificCurrencyPair,
    pub(crate) id: u64,
    pub(crate) side: OrderSide,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexOrderBookUpdate {
    pub(crate) symbol: SpecificCurrencyPair,
    pub(crate) id: u64,
    pub(crate) side: OrderSide,
    pub(crate) size: Amount,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexTradePayload {
    pub(crate) symbol: SpecificCurrencyPair,
    pub(crate) side: OrderSide,
    pub(crate) size: Amount,
    pub(crate) price: Price,
    #[serde(rename = "trdMatchID")]
    pub(crate) trade_id: String,
    #[serde(deserialize_with = "deserialize_datetime")]
    pub(crate) timestamp: DateTime,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexOrderStatus {
    #[serde(rename = "clOrdID")]
    pub(crate) client_order_id: ClientOrderId,
    #[serde(rename = "orderID")]
    pub(crate) exchange_order_id: ExchangeOrderId,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BitmexOrderFill<'a> {
    #[serde(rename = "text")]
    pub(crate) details: &'a str,
    #[serde(rename = "execID")]
    pub(crate) trade_id: String,
    #[serde(rename = "clOrdID")]
    pub(crate) client_order_id: ClientOrderId,
    #[serde(rename = "orderID")]
    pub(crate) exchange_order_id: ExchangeOrderId,
    #[serde(rename = "lastPx")]
    pub(crate) fill_price: Price,
    #[serde(rename = "lastQty")]
    pub(crate) fill_amount: Amount,
    #[serde(rename = "cumQty")]
    pub(crate) total_filled_amount: Amount,
    #[serde(rename = "orderQty")]
    pub(crate) amount: Amount,
    #[serde(deserialize_with = "deserialize_datetime")]
    pub(crate) timestamp: DateTime,
    pub(crate) side: OrderSide,
    pub(crate) symbol: SpecificCurrencyPair,
    #[serde(rename = "settlCurrency")]
    pub(crate) currency: &'a str,
    #[serde(rename = "commission")]
    pub(crate) commission_rate: Decimal,
    #[serde(rename = "execComm")]
    pub(crate) commission_amount: Decimal,
}

fn deserialize_datetime<'de, D>(deserializer: D) -> Result<DateTime, D::Error>
where
    D: Deserializer<'de>,
{
    struct DateTimeVisitor;

    impl<'de> de::Visitor<'de> for DateTimeVisitor {
        type Value = DateTime;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string containing json data")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let parsed = chrono::DateTime::parse_from_rfc3339(v).map_err(E::custom)?;
            Ok(DateTime::from(parsed))
        }
    }

    deserializer.deserialize_any(DateTimeVisitor)
}
