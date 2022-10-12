use crate::market::CurrencyPair;
use crate::order::snapshot::{Amount, ExchangeOrderId, OrderSide, Price, String16};
use hyper::StatusCode;
use once_cell::sync::Lazy;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct DerivativePosition {
    pub currency_pair: CurrencyPair,
    pub position: Amount,
    pub average_entry_price: Price,
    pub liquidation_price: Price,
    pub leverage: Decimal,
}

impl DerivativePosition {
    pub fn new(
        currency_pair: CurrencyPair,
        position: Decimal,
        average_entry_price: Price,
        liquidation_price: Price,
        leverage: Decimal,
    ) -> DerivativePosition {
        DerivativePosition {
            currency_pair,
            position,

            average_entry_price,
            liquidation_price,
            leverage,
        }
    }

    pub fn get_side(&self) -> OrderSide {
        debug_assert!(self.position.is_zero());

        if self.position.is_sign_negative() {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        }
    }
}

#[derive(Debug)]
pub struct ClosedPosition {
    pub exchange_order_id: ExchangeOrderId,
    pub amount: Amount,
}

impl ClosedPosition {
    pub fn new(exchange_order_id: ExchangeOrderId, amount: Amount) -> Self {
        Self {
            exchange_order_id,
            amount,
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
