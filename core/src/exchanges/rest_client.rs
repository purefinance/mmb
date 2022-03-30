use super::common::*;
use anyhow::{bail, Context, Result};
use hyper::client::HttpConnector;
use hyper::{Body, Client, Error, Request, Response, StatusCode, Uri};
use hyper_tls::HttpsConnector;
use log::log;
use mmb_utils::infrastructure::WithExpect;
use std::convert::TryInto;
use std::fmt::Write;

pub type HttpParams = Vec<(String, String)>;

/// Trait for specific exchange errors handling
pub trait ErrorHandler {
    // To find out if there is any special exchange error in a rest outcome
    fn check_spec_rest_error(&self, _response: &RestRequestOutcome) -> Result<(), ExchangeError>;

    // Some of special errors should be classified to further handling depending on error type
    fn clarify_error_type(&self, _error: &mut ExchangeError);
}

pub struct ErrorHandlerEmpty;

// TODO change to static dispatching from dynamic
pub type BoxErrorHandler = Box<dyn ErrorHandler + Send + Sync>;

impl ErrorHandlerEmpty {
    pub fn new() -> Box<Self> {
        Box::new(ErrorHandlerEmpty {})
    }
}

impl ErrorHandler for ErrorHandlerEmpty {
    fn check_spec_rest_error(&self, _: &RestRequestOutcome) -> Result<(), ExchangeError> {
        Ok(())
    }

    fn clarify_error_type(&self, _: &mut ExchangeError) {}
}

pub struct ErrorHandlerData {
    empty_response_is_ok: bool,
    exchange_account_id: ExchangeAccountId,
    error_handler: BoxErrorHandler,
}

impl ErrorHandlerData {
    pub fn new(
        empty_response_is_ok: bool,
        exchange_account_id: ExchangeAccountId,
        error_handler_trait: BoxErrorHandler,
    ) -> Self {
        Self {
            empty_response_is_ok,
            exchange_account_id,
            error_handler: error_handler_trait,
        }
    }

    pub(super) fn request_log(&self, fn_name: &str) {
        log::trace!(
            "{} request on exchange_account_id {}",
            fn_name,
            self.exchange_account_id,
        );
    }

    pub(super) fn response_log(
        &self,
        fn_name: &str,
        log_args: &str,
        response: &RestRequestOutcome,
    ) {
        log::trace!(
            "{} response on exchange_account_id {}: {:?}, params {}",
            fn_name,
            self.exchange_account_id,
            response,
            log_args,
        );
    }

    pub(super) fn get_rest_error(
        &self,
        response: &RestRequestOutcome,
        log_args: &str,
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
                    if self.empty_response_is_ok {
                        return None;
                    }

                    ExchangeError::new(Unknown, "Empty response".to_owned(), None)
                }
                CheckContent::Usable => match self.error_handler.check_spec_rest_error(response) {
                    Ok(_) => return None,
                    Err(mut error) => match error.error_type {
                        ParsingError => error,
                        _ => {
                            // TODO For Aax Pending time should be received inside clarify_error_type
                            self.error_handler.clarify_error_type(&mut error);
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
            "Response has an error {:?}, on exchange_account_id {}: {:?}, params: {}",
            error.error_type, self.exchange_account_id, error, log_args
        )
        .expect("Writing rest error");

        let log_level = match error.error_type {
            RateLimit | Authentication | InsufficientFunds | InvalidOrder => log::Level::Error,
            _ => log::Level::Warn,
        };
        log!(log_level, "{}. Response: {:?}", &msg, response);

        Some(error)
    }
}

enum CheckContent {
    Empty,
    Usable,
}

fn check_content(content: &str) -> CheckContent {
    if content.is_empty() {
        CheckContent::Empty
    } else {
        CheckContent::Usable
    }
}

pub struct RestClient {
    client: Client<HttpsConnector<HttpConnector>>,
    error_handler: ErrorHandlerData,
}

const KEEP_ALIVE: &str = "keep-alive";
// Inner Hyper types. Needed just for unified response handling in handle_response()
type ResponseType = std::result::Result<Response<Body>, Error>;

impl RestClient {
    pub fn new(error_handler: ErrorHandlerData) -> Self {
        Self {
            client: create_client(),
            error_handler,
        }
    }

    pub async fn get(
        &self,
        url: Uri,
        api_key: &str,
        action_name: &'static str,
        log_args: String,
    ) -> Result<RestRequestOutcome> {
        self.error_handler.request_log(action_name);

        let req = Request::get(url)
            .header(hyper::header::CONNECTION, KEEP_ALIVE)
            .header("X-MBX-APIKEY", api_key)
            .body(Body::empty())
            .expect("Error during creation of http GET request");

        let response = self.client.request(req).await;

        self.handle_response(response, "GET", action_name, log_args)
            .await
    }

    pub async fn post(
        &self,
        url: Uri,
        api_key: &str,
        http_params: &HttpParams,
        action_name: &'static str,
        log_args: String,
    ) -> Result<RestRequestOutcome> {
        self.error_handler.request_log(action_name);

        let form_encoded = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(http_params)
            .finish();

        let req = Request::post(url)
            .header(hyper::header::CONNECTION, KEEP_ALIVE)
            .header("X-MBX-APIKEY", api_key)
            .body(Body::from(form_encoded))
            .expect("Error during creation of http delete request");

        let response = self.client.request(req).await;

        self.handle_response(response, "POST", action_name, log_args)
            .await
    }

    pub async fn delete(
        &self,
        url: Uri,
        api_key: &str,
        action_name: &'static str,
        log_args: String,
    ) -> Result<RestRequestOutcome> {
        self.error_handler.request_log(action_name);

        let req = Request::delete(url)
            .header(hyper::header::CONNECTION, KEEP_ALIVE)
            .header("X-MBX-APIKEY", api_key)
            .body(Body::empty())
            .expect("Error during creation of http delete request");

        let response = self.client.request(req).await;

        self.handle_response(response, "DELETE", action_name, log_args)
            .await
    }

    async fn handle_response(
        &self,
        response: ResponseType,
        rest_action: &'static str,
        action_name: &'static str,
        log_args: String,
    ) -> Result<RestRequestOutcome> {
        let response = response.with_expect(|| format!("Unable to send {} request", rest_action));
        let response_status = response.status();
        let request_bytes = hyper::body::to_bytes(response.into_body())
            .await
            .expect("Unable to convert response body to bytes");
        let request_content = std::str::from_utf8(&request_bytes)
            .with_expect(|| {
                format!(
                    "Unable to convert response content from utf8: {:?}",
                    request_bytes
                )
            })
            .to_owned();

        let request_outcome = RestRequestOutcome {
            status: response_status,
            content: request_content,
        };

        self.error_handler
            .response_log(action_name, &log_args, &request_outcome);

        if let Some(err) = self
            .error_handler
            .get_rest_error(&request_outcome, &log_args)
        {
            bail!(err);
        }

        Ok(request_outcome)
    }
}

fn create_client() -> Client<HttpsConnector<HttpConnector>> {
    let https = HttpsConnector::new();
    Client::builder().build::<_, Body>(https)
}

pub fn build_uri(host: &str, path: &str, http_params: &HttpParams) -> Result<Uri> {
    let mut url = String::with_capacity(1024);
    url.push_str(host);
    url.push_str(path);

    if !http_params.is_empty() {
        url.push('?');
    }

    let mut is_first = true;
    for (k, v) in http_params {
        if !is_first {
            url.push('&')
        }
        url.push_str(k);
        url.push('=');
        url.push_str(v);

        is_first = false;
    }

    url.try_into().context("Unable create url")
}

pub fn to_http_string(parameters: &HttpParams) -> String {
    let mut http_string = String::new();
    for (key, value) in parameters {
        if !http_string.is_empty() {
            http_string.push('&');
        }
        http_string.push_str(key);
        http_string.push('=');
        http_string.push_str(value);
    }

    http_string
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn full_uri() {
        let host = "https://host.com";
        let path = "/path";
        let params: HttpParams = vec![("key", "value"), ("key2", "value2")]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();

        let uri = build_uri(host, path, &params).expect("in test");

        let expected: Uri = "https://host.com/path?key=value&key2=value2"
            .try_into()
            .expect("in test");
        assert_eq!(uri, expected)
    }

    #[test]
    pub fn uri_without_params() {
        let host = "https://host.com";
        let path = "/path";
        let params: HttpParams = HttpParams::new();

        let uri = build_uri(host, path, &params).expect("in test");

        let expected: Uri = "https://host.com/path".try_into().expect("in test");
        assert_eq!(uri, expected)
    }
}
