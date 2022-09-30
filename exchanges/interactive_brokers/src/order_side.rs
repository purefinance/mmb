use mmb_domain::order::snapshot::OrderSide as MmbOrderSide;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Clone, Copy)]
pub enum OrderSide {
    Buy,
    Sell,
    SShort,
}

impl OrderSide {
    fn as_str(&self) -> &str {
        match self {
            OrderSide::Buy => "BUY",
            OrderSide::Sell => "SELL",
            OrderSide::SShort => "SSHORT",
        }
    }
}

impl Display for OrderSide {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for OrderSide {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "BUY" => Ok(Self::Buy),
            "SELL" => Ok(Self::Sell),
            "SSHORT" => Ok(Self::SShort),
            _ => Err(format!("Unknown variant: {s}.")),
        }
    }
}

impl From<MmbOrderSide> for OrderSide {
    fn from(mmb_side: MmbOrderSide) -> Self {
        match mmb_side {
            MmbOrderSide::Buy => Self::Buy,
            MmbOrderSide::Sell => Self::Sell,
        }
    }
}

impl TryInto<MmbOrderSide> for OrderSide {
    type Error = String;

    fn try_into(self) -> Result<MmbOrderSide, Self::Error> {
        match self {
            OrderSide::Buy => Ok(MmbOrderSide::Buy),
            OrderSide::Sell => Ok(MmbOrderSide::Sell),
            OrderSide::SShort => Err("Unsupported side: `SShort`.".to_string()),
        }
    }
}
