use std::fmt::{self, Display, Formatter};
use serde::{Serialize, Deserialize};
use smallstr::SmallString;

type String16 = SmallString<[u8; 16]>;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExchangeId(String16);

impl ExchangeId {
    pub fn new(exchange_id: String16) -> Self {
        ExchangeId(exchange_id) }
}

impl From<&str> for ExchangeId {
    fn from(value: &str) -> Self {
        ExchangeId(String16::from_str(value)) }
}

impl Display for ExchangeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExchangeName(String16);

impl ExchangeName {
    pub fn new(exchange_name: String16) -> Self {
        ExchangeName(exchange_name)
    }
}

impl From<&str> for ExchangeName {
    fn from(value: &str) -> Self {
        ExchangeName(String16::from_str(value))
    }
}


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