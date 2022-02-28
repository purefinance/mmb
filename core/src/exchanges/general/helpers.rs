use crate::exchanges::common::{
    ExchangeAccountId, ExchangeError, ExchangeErrorType, RestRequestOutcome,
};
use crate::orders::order::OrderHeader;
use anyhow::{bail, Error, Result};
use hyper::StatusCode;
use log::log;
use serde_json::Value;
use std::fmt::Arguments;
use std::fmt::Write;

pub fn handle_parse_error(
    error: Error,
    response: &RestRequestOutcome,
    log_template: String,
    args_to_log: Option<Vec<String>>,
    exchange_account_id: ExchangeAccountId,
) -> Result<()> {
    let content = &response.content;
    let log_event_level = match serde_json::from_str::<Value>(content) {
        Ok(_) => log::Level::Error,
        Err(_) => log::Level::Warn,
    };

    let mut msg_to_log = format!(
        "Error parsing response {}, on {}: {}. Error: {:?}",
        log_template, exchange_account_id, content, error
    );

    if let Some(args) = args_to_log {
        msg_to_log = format!(" {} with args: {:?}", msg_to_log, args);
    }

    log!(log_event_level, "{}.", msg_to_log,);

    if log_event_level == log::Level::Error {
        bail!("{}", msg_to_log);
    }

    Ok(())
}

pub fn get_rest_error(
    response: &RestRequestOutcome,
    exchange_account_id: ExchangeAccountId,
    empty_response_is_ok: bool,
) -> Option<ExchangeError> {
    get_rest_error_main(
        response,
        exchange_account_id,
        empty_response_is_ok,
        format_args!(""),
    )
}

pub fn get_rest_error_order(
    response: &RestRequestOutcome,
    order_header: &OrderHeader,
    empty_response_is_ok: bool,
) -> Option<ExchangeError> {
    get_rest_error_main(
        response,
        order_header.exchange_account_id,
        empty_response_is_ok,
        format_args!(
            "order {} {}",
            order_header.client_order_id, order_header.exchange_account_id
        ),
    )
}

fn get_rest_error_main(
    response: &RestRequestOutcome,
    exchange_account_id: ExchangeAccountId,
    empty_response_is_ok: bool,
    log_template: Arguments,
) -> Option<ExchangeError> {
    use ExchangeErrorType::*;

    let error = match response.status {
        StatusCode::UNAUTHORIZED => {
            ExchangeError::new(Authentication, response.content.clone(), None)
        }
        StatusCode::GATEWAY_TIMEOUT | StatusCode::SERVICE_UNAVAILABLE => {
            ExchangeError::new(ServiceUnavailable, response.content.clone(), None)
        }
        StatusCode::TOO_MANY_REQUESTS => {
            ExchangeError::new(RateLimit, response.content.clone(), None)
        }
        _ => match check_content(&response.content) {
            CheckContent::Empty => {
                if empty_response_is_ok {
                    return None;
                }

                ExchangeError::new(Unknown, "Empty response".to_owned(), None)
            }
            CheckContent::Usable => match is_rest_error_code(response) {
                Ok(_) => return None,
                Err(mut error) => match error.error_type {
                    ParsingError => error,
                    _ => {
                        // TODO For Aax Pending time should be received inside clarify_error_type
                        clarify_error_type(&mut error);
                        error
                    }
                },
            },
        },
    };

    let extra_data_len = 512; // just apriori estimation
    let mut msg = String::with_capacity(error.message.len() + extra_data_len);
    write!(
        &mut msg,
        "Response has an error {:?}, on {}: {:?}",
        error.error_type, exchange_account_id, error
    )
    .expect("Writing rest error");

    write!(&mut msg, " {}", log_template).expect("Writing rest error");

    let log_level = match error.error_type {
        RateLimit | Authentication | InsufficientFunds | InvalidOrder => log::Level::Error,
        _ => log::Level::Warn,
    };

    log!(log_level, "{}. Response: {:?}", &msg, response);

    // TODO some HandleRestError via BotBase

    Some(error)
}

pub fn is_rest_error_code(response: &RestRequestOutcome) -> Result<(), ExchangeError> {
    //Binance is a little inconsistent: for failed responses sometimes they include
    //only code or only success:false but sometimes both
    if !(response.content.contains(r#""success":false"#) || response.content.contains(r#""code""#))
    {
        return Ok(());
    }

    let data: Value = serde_json::from_str(&response.content)
        .map_err(|err| ExchangeError::parsing_error(&format!("response.content: {:?}", err)))?;

    let message = data["msg"]
        .as_str()
        .ok_or_else(|| ExchangeError::parsing_error("`msg` field"))?;

    let code = data["code"]
        .as_i64()
        .ok_or_else(|| ExchangeError::parsing_error("`code` field"))?;

    Err(ExchangeError::new(
        ExchangeErrorType::Unknown,
        message.to_string(),
        Some(code),
    ))
}

fn clarify_error_type(error: &mut ExchangeError) {
    // -1010 ERROR_MSG_RECEIVED
    // -2010 NEW_ORDER_REJECTED
    // -2011 CANCEL_REJECTED
    let error_type = match error.message.as_str() {
        "Unknown order sent." | "Order does not exist." => ExchangeErrorType::OrderNotFound,
        "Account has insufficient balance for requested action." => {
            ExchangeErrorType::InsufficientFunds
        }
        "Invalid quantity."
        | "Filter failure: MIN_NOTIONAL"
        | "Filter failure: LOT_SIZE"
        | "Filter failure: PRICE_FILTER"
        | "Filter failure: PERCENT_PRICE"
        | "Quantity less than zero."
        | "Precision is over the maximum defined for this asset." => {
            ExchangeErrorType::InvalidOrder
        }
        msg if msg.contains("Too many requests;") => ExchangeErrorType::RateLimit,
        _ => ExchangeErrorType::Unknown,
    };

    error.error_type = error_type;
}

fn check_content(content: &str) -> CheckContent {
    if content.is_empty() {
        CheckContent::Empty
    } else {
        CheckContent::Usable
    }
}

enum CheckContent {
    Empty,
    Usable,
}
