use mmb_domain::order::snapshot::OrderStatus as MmbOrderStatus;
use std::convert::Infallible;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum OrderStatus {
    ApiCancelled,
    Cancelled,
    Filled,
    Other(String),
    PendingCancel,
    PendingSubmit,
    PreSubmitted,
    Submitted,
}

impl OrderStatus {
    fn as_str(&self) -> &str {
        match self {
            Self::ApiCancelled => "ApiCancelled",
            Self::Cancelled => "Cancelled",
            Self::Filled => "Filled",
            Self::Other(s) => s,
            Self::PendingCancel => "PendingCancel",
            Self::PendingSubmit => "PendingSubmit",
            Self::PreSubmitted => "PreSubmitted",
            Self::Submitted => "Submitted",
        }
    }
}

impl Display for OrderStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for OrderStatus {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "ApiCancelled" => Self::ApiCancelled,
            "Cancelled" => Self::Cancelled,
            "Filled" => Self::Filled,
            "PendingCancel" => Self::PendingCancel,
            "PendingSubmit" => Self::PendingSubmit,
            "PreSubmitted" => Self::PreSubmitted,
            "Submitted" => Self::Submitted,
            _ => Self::Other(s.to_string()),
        })
    }
}

impl TryInto<MmbOrderStatus> for OrderStatus {
    type Error = String;

    fn try_into(self) -> Result<MmbOrderStatus, Self::Error> {
        match self {
            OrderStatus::ApiCancelled => Ok(MmbOrderStatus::Canceled),
            OrderStatus::Cancelled => Ok(MmbOrderStatus::Canceled),
            OrderStatus::Filled => Ok(MmbOrderStatus::Completed),
            OrderStatus::Other(s) => Err(format!("Unsupported status: {s}.")),
            OrderStatus::PendingCancel => Ok(MmbOrderStatus::Canceling),
            OrderStatus::PendingSubmit => Ok(MmbOrderStatus::Creating),
            OrderStatus::PreSubmitted => Ok(MmbOrderStatus::Creating),
            OrderStatus::Submitted => Ok(MmbOrderStatus::Created),
        }
    }
}
