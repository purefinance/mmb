use crate::exchanges::traits::ExchangeError;
use anyhow::Result;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use hyper::client::HttpConnector;
use hyper::http::request::Builder;
use hyper::http::uri::{Parts, PathAndQuery};
use hyper::{Body, Client, Error, Method, Request, Response, StatusCode, Uri};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use log::log;
use mmb_domain::market::*;
use mmb_utils::infrastructure::WithExpect;
use std::borrow::Cow;
use std::convert::TryInto;
use std::fmt;
use std::fmt::{Debug, Display, Formatter, Write};
use uuid::Uuid;

pub type QueryKey = &'static str;

pub trait RestHeaders {
    fn add_specific_headers(
        &self,
        builder: Builder,
        uri: &Uri,
        request_type: RequestType,
    ) -> Builder;
}

/// Trait for specific exchange errors handling
pub trait ErrorHandler: Sized {
    // To find out if there is any special exchange error in a rest outcome
    fn check_spec_rest_error(&self, _response: &RestResponse) -> Result<(), ExchangeError>;

    // Some of special errors should be classified to further handling depending on error type
    fn clarify_error_type(&self, _error: &ExchangeError) -> ExchangeErrorType;
}

#[derive(Default)]
pub struct ErrorHandlerEmpty;

impl ErrorHandler for ErrorHandlerEmpty {
    fn check_spec_rest_error(&self, _: &RestResponse) -> Result<(), ExchangeError> {
        Ok(())
    }

    fn clarify_error_type(&self, _: &ExchangeError) -> ExchangeErrorType {
        ExchangeErrorType::Unknown
    }
}

pub struct ErrorHandlerData<ErrHandler: ErrorHandler + Send + Sync + 'static> {
    empty_response_is_ok: bool,
    exchange_account_id: ExchangeAccountId,
    error_handler: ErrHandler,
}

impl<ErrHandler: ErrorHandler + Send + Sync + 'static> ErrorHandlerData<ErrHandler> {
    pub fn new(
        empty_response_is_ok: bool,
        exchange_account_id: ExchangeAccountId,
        error_handler: ErrHandler,
    ) -> Self {
        Self {
            empty_response_is_ok,
            exchange_account_id,
            error_handler,
        }
    }

    pub(super) fn request_log(&self, action_name: &str, request_id: &Uuid) {
        log::trace!(
            "{action_name} request {request_id} on exchange_account_id {}",
            self.exchange_account_id
        );
    }

    pub(super) fn response_log(
        &self,
        fn_name: &str,
        log_args: &str,
        response: &RestResponse,
        request_id: &Uuid,
    ) {
        log::trace!(
            "{fn_name} response on {}: {response:?}, | params {log_args}, request_id: {request_id}",
            self.exchange_account_id
        );
    }

    pub(super) fn get_rest_error(
        &self,
        response: &RestResponse,
        log_args: &str,
        request_id: &Uuid,
    ) -> Result<(), ExchangeError> {
        use ExchangeErrorType::*;

        let error = match response.status {
            StatusCode::UNAUTHORIZED => ExchangeError::authentication(response.content.clone()),
            StatusCode::GATEWAY_TIMEOUT | StatusCode::SERVICE_UNAVAILABLE => {
                ExchangeError::new(ServiceUnavailable, response.content.clone(), None)
            }
            StatusCode::TOO_MANY_REQUESTS => {
                ExchangeError::new(RateLimit, response.content.clone(), None)
            }
            _ => match check_content(&response.content) {
                CheckContent::Empty => {
                    if self.empty_response_is_ok {
                        return Ok(());
                    }

                    ExchangeError::unknown("Empty response")
                }
                CheckContent::Usable => match self.error_handler.check_spec_rest_error(response) {
                    Ok(_) => return Ok(()),
                    Err(mut err) => {
                        // TODO For Aax Pending time should be received inside clarify_error_type
                        err.error_type = self.error_handler.clarify_error_type(&err);
                        err
                    }
                },
            },
        };

        let extra_data_len = 512; // just apriori estimation
        let mut msg = String::with_capacity(error.message.len() + extra_data_len);
        write!(
            msg,
            "Response has an error {:?}, on exchange_account_id {}, request_id: {request_id}: {error:?}, params: {log_args}",
            error.error_type, self.exchange_account_id,
        )
        .expect("Writing rest error");

        let log_level = match error.error_type {
            RateLimit | Authentication | InsufficientFunds | InvalidOrder => log::Level::Error,
            _ => log::Level::Warn,
        };
        log!(
            log_level,
            "Request_id: {request_id}. Message: {msg}. Response: {response:?}"
        );

        Err(error)
    }
}

enum CheckContent {
    Empty,
    Usable,
}

fn check_content(content: &str) -> CheckContent {
    match content.is_empty() {
        true => CheckContent::Empty,
        false => CheckContent::Usable,
    }
}

#[derive(Copy, Clone)]
pub enum RequestType {
    Get,
    Post,
    Delete,
}

impl RequestType {
    pub const fn as_str(&self) -> &'static str {
        match *self {
            RequestType::Get => "GET",
            RequestType::Post => "POST",
            RequestType::Delete => "DELETE",
        }
    }
}

impl Display for RequestType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Debug for RequestType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub struct RestClient<
    ErrHandler: ErrorHandler + Send + Sync + 'static,
    SpecHeaders: RestHeaders + Send + Sync + 'static,
> {
    client: Client<HttpsConnector<HttpConnector>>,
    error_handler: ErrorHandlerData<ErrHandler>,
    headers: SpecHeaders,
}

const KEEP_ALIVE: &str = "keep-alive";
// Inner Hyper types. Needed just for unified response handling in handle_response()
type ResponseType = Result<Response<Body>, Error>;

impl<ErrHandler: ErrorHandler + Send + Sync + 'static, SpecHeaders: RestHeaders + Send + Sync>
    RestClient<ErrHandler, SpecHeaders>
{
    pub fn new(error_handler: ErrorHandlerData<ErrHandler>, headers: SpecHeaders) -> Self {
        Self {
            client: create_client(),
            error_handler,
            headers,
        }
    }

    pub async fn get(
        &self,
        uri: Uri,
        action_name: &'static str,
        log_args: String,
    ) -> Result<RestResponse, ExchangeError> {
        let request_id = Uuid::new_v4();
        self.error_handler.request_log(action_name, &request_id);

        let builder = Request::builder().method(Method::GET);
        let req = self
            .headers
            .add_specific_headers(builder, &uri, RequestType::Get)
            .uri(uri)
            .header(hyper::header::CONNECTION, KEEP_ALIVE)
            .body(Body::empty())
            .with_expect(|| format!("Error during creation of http GET request {request_id}"));

        let response = self.client.request(req).await;

        self.handle_response(
            response,
            RequestType::Get.as_str(),
            action_name,
            log_args,
            request_id,
        )
        .await
    }

    pub async fn put(
        &self,
        uri: Uri,
        api_key: &str,
        action_name: &'static str,
        log_args: String,
    ) -> Result<RestResponse, ExchangeError> {
        let request_id = Uuid::new_v4();
        self.error_handler.request_log(action_name, &request_id);

        let req = Request::put(uri)
            .header(hyper::header::CONNECTION, KEEP_ALIVE)
            .header("X-MBX-APIKEY", api_key)
            .body(Body::empty())
            .with_expect(|| format!("Error during creation of http PUT request {request_id}"));

        let response = self.client.request(req).await;

        self.handle_response(response, "PUT", action_name, log_args, request_id)
            .await
    }

    pub async fn post(
        &self,
        uri: Uri,
        query: Option<Bytes>,
        action_name: &'static str,
        log_args: String,
    ) -> Result<RestResponse, ExchangeError> {
        let request_id = Uuid::new_v4();
        self.error_handler.request_log(action_name, &request_id);

        let builder = Request::builder().method(Method::POST);
        let req = self
            .headers
            .add_specific_headers(builder, &uri, RequestType::Post)
            .uri(uri)
            .header(hyper::header::CONNECTION, KEEP_ALIVE)
            .body(match query {
                Some(query) => Body::from(query),
                None => Body::empty(),
            })
            .with_expect(|| format!("Error during creation of http POST request {request_id}"));

        let response = self.client.request(req).await;

        self.handle_response(response, "POST", action_name, log_args, request_id)
            .await
    }

    pub async fn delete(
        &self,
        uri: Uri,
        api_key: &str,
        action_name: &'static str,
        log_args: String,
    ) -> Result<RestResponse, ExchangeError> {
        let request_id = Uuid::new_v4();
        self.error_handler.request_log(action_name, &request_id);

        let req = Request::delete(uri)
            .header(hyper::header::CONNECTION, KEEP_ALIVE)
            .header("X-MBX-APIKEY", api_key)
            .body(Body::empty())
            .with_expect(|| format!("Error during creation of http DELETE request {request_id}",));

        let response = self.client.request(req).await;

        self.handle_response(response, "DELETE", action_name, log_args, request_id)
            .await
    }

    async fn handle_response(
        &self,
        response: ResponseType,
        rest_action: &'static str,
        action_name: &'static str,
        log_args: String,
        request_id: Uuid,
    ) -> Result<RestResponse, ExchangeError> {
        let response = response.with_expect(|| {
            format!("Unable to send {rest_action} request, request_id: {request_id}")
        });
        let status = response.status();
        let request_bytes = hyper::body::to_bytes(response.into_body())
            .await
            .with_expect(|| {
                format!("Unable to convert response body to bytes, request_id: {request_id}")
            });

        let content = std::str::from_utf8(&request_bytes)
            .with_expect(|| format!("Unable to convert response content from utf8: {request_bytes:?}, request_id: {request_id}"))
            .to_owned();

        let request_outcome = RestResponse { status, content };

        let err_handler_data = &self.error_handler;
        err_handler_data.response_log(action_name, &log_args, &request_outcome, &request_id);
        err_handler_data.get_rest_error(&request_outcome, &log_args, &request_id)?;

        Ok(request_outcome)
    }
}

fn create_client() -> Client<HttpsConnector<HttpConnector>> {
    let https = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_only()
        .enable_http1()
        .enable_http2()
        .build();
    Client::builder().build::<_, Body>(https)
}

pub struct UriBuilder {
    // buffer for path and query parts of uri
    buffer: BytesMut,
    // start index in buffer for query part
    query_start: usize,
}

impl UriBuilder {
    pub fn new(capacity: usize, path: &str) -> Self {
        let mut buf = BytesMut::with_capacity(capacity);
        buf.extend_from_slice(path.as_bytes());
        buf.put_u8(b'?');
        let query_start = buf.len();
        Self {
            buffer: buf,
            query_start,
        }
    }

    pub fn from_path(path: &str) -> Self {
        Self::new(1024, path)
    }

    // Add key of query with near symbols '=' and '&' if needed
    fn add_static_part(&mut self, key: QueryKey) {
        let buf = &mut self.buffer;
        if buf.len() > self.query_start {
            buf.put_u8(b'&')
        }
        buf.extend_from_slice(key.as_bytes());
        buf.put_u8(b'=');
    }

    pub fn add_kv(&mut self, key: QueryKey, value: impl Display + Sized) {
        self.add_static_part(key);
        if let Err(err) = write!(self.buffer, "{value}") {
            panic!("unable add parameter to query with key {key}: {err}");
        }
    }

    pub fn ensure_free_size(&mut self, need_capacity: usize) {
        if self.buffer.remaining() < need_capacity {
            self.buffer.reserve(need_capacity)
        }
    }

    pub fn query(&mut self) -> &[u8] {
        &self.buffer[self.query_start..]
    }

    pub fn build_uri_and_query(self, host: &str, add_query_to_uri: bool) -> (Uri, Bytes) {
        let buffer = self.buffer.freeze();

        let query = buffer.slice(self.query_start..);

        let path_and_query = match add_query_to_uri {
            false => buffer.slice(..self.query_start - 1),
            true if buffer.len() == self.query_start => buffer.slice(..self.query_start - 1),
            true => buffer,
        };
        let path_and_query = PathAndQuery::from_maybe_shared(path_and_query)
            .expect("Unable create PathAndQuery from UriQueryBuilder");

        let mut parts = Parts::default();
        parts.scheme = Some("https".try_into().expect("Unable build scheme for url"));
        parts.authority = Some(host.try_into().expect("Unable build authority for url"));
        parts.path_and_query = Some(path_and_query);

        let uri = Uri::from_parts(parts).expect("Unable build url from parts");

        (uri, query)
    }

    pub fn build_uri(self, host: &str, add_query_to_uri: bool) -> Uri {
        self.build_uri_and_query(host, add_query_to_uri).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    pub fn build_query_by_builder() {
        let mut builder = UriBuilder::from_path("/path");
        builder.add_kv("symbol", "LTCBTC");
        builder.add_kv("side", "BUY");
        builder.add_kv("type", "LIMIT");
        builder.add_kv("timeInForce", "GTC");
        builder.add_kv("quantity", "1");
        builder.add_kv("price", "0.1");
        builder.add_kv("recvWindow", "5000");
        builder.add_kv("timestamp", "1499827319559");

        let query = builder.query();

        let expected = b"symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559";
        assert_eq!(query, expected);
    }

    #[test]
    pub fn build_uri_with_query_by_builder() {
        let host = "host.com";
        let path = "/path";

        let mut builder = UriBuilder::from_path(path);
        builder.add_kv("key", "value");
        builder.add_kv("key2", 32);
        builder.add_kv("key3", &dec!(42));
        let query = builder.query();
        assert_eq!(query, b"key=value&key2=32&key3=42");

        let path_and_query = builder.build_uri(host, true);
        assert_eq!(
            path_and_query,
            Uri::from_static("https://host.com/path?key=value&key2=32&key3=42")
        )
    }

    #[test]
    pub fn build_uri_without_query_by_builder() {
        let host = "host.com";
        let path = "/path";

        let mut builder = UriBuilder::from_path(path);
        builder.add_kv("key", "value");
        builder.add_kv("key2", 32);
        builder.add_kv("key3", &dec!(42));
        let query = builder.query();
        assert_eq!(query, b"key=value&key2=32&key3=42");

        let path_and_query = builder.build_uri(host, false);
        assert_eq!(path_and_query, Uri::from_static("https://host.com/path"))
    }

    #[test]
    pub fn build_uri_from_empty_builder() {
        let host = "host.com";
        let path = "/path";

        let mut builder = UriBuilder::from_path(path);
        let query = builder.query();
        assert_eq!(query, b"");

        let path_and_query = builder.build_uri(host, true);
        assert_eq!(path_and_query, Uri::from_static("https://host.com/path"))
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum RestRequestError {
    IsInProgress,
    Status(StatusCode),
}

#[derive(Eq, PartialEq, Clone)]
pub struct RestResponse {
    pub status: StatusCode,
    pub content: String,
}

impl Debug for RestResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let cut_content = if self.content.len() > 1500 {
            Cow::Owned(self.content.chars().take(1500).collect::<String>())
        } else {
            Cow::Borrowed(&self.content)
        };

        write!(f, "status: {:?} content: {}", &self.status, &cut_content)
    }
}

impl RestResponse {
    pub fn new(content: String, status: StatusCode) -> Self {
        Self { content, status }
    }
}

pub type RestRequestResult = std::result::Result<String, RestRequestError>;
