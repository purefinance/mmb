use super::common::*;
use anyhow::{Context, Result};
use hyper::client::HttpConnector;
use hyper::{Body, Client, Error, Request, Response, Uri};
use hyper_tls::HttpsConnector;
use std::convert::TryInto;

pub type HttpParams = Vec<(String, String)>;

pub struct RestClient {
    client: Client<HttpsConnector<HttpConnector>>,
}

const KEEP_ALIVE: &'static str = "keep-alive";

impl RestClient {
    pub fn new() -> Self {
        Self {
            client: create_client(),
        }
    }

    pub async fn get(&self, url: Uri, api_key: &str) -> Result<RestRequestOutcome> {
        let req = Request::get(url)
            .header(hyper::header::CONNECTION, KEEP_ALIVE)
            .header("X-MBX-APIKEY", api_key)
            .body(Body::empty())
            .context("Error during creation of http GET request")?;

        let response = self.client.request(req).await;

        handle_response(response, "GET").await
    }

    pub async fn post(
        &self,
        url: Uri,
        api_key: &str,
        http_params: &HttpParams,
    ) -> Result<RestRequestOutcome> {
        let form_encoded = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(http_params)
            .finish();

        let req = Request::post(url)
            .header(hyper::header::CONNECTION, KEEP_ALIVE)
            .header("X-MBX-APIKEY", api_key)
            .body(Body::from(form_encoded))
            .context("Error during creation of http delete request")?;

        let response = self.client.request(req).await;

        handle_response(response, "POST").await
    }

    pub async fn delete(&self, url: Uri, api_key: &str) -> Result<RestRequestOutcome> {
        let req = Request::delete(url)
            .header(hyper::header::CONNECTION, KEEP_ALIVE)
            .header("X-MBX-APIKEY", api_key)
            .body(Body::empty())
            .context("Error during creation of http delete request")?;

        let response = self.client.request(req).await;

        handle_response(response, "DELETE").await
    }
}

fn create_client() -> Client<HttpsConnector<HttpConnector>> {
    let https = HttpsConnector::new();
    Client::builder().build::<_, hyper::Body>(https)
}

// Inner Hyper types. Needed just for unified response handling in handle_response()
type ResponseType = std::result::Result<Response<Body>, Error>;
async fn handle_response(response: ResponseType, rest_action: &str) -> Result<RestRequestOutcome> {
    let response = response.with_context(|| format!("Unable to send {} request", rest_action))?;

    Ok(RestRequestOutcome {
        status: response.status(),
        content: std::str::from_utf8(hyper::body::to_bytes(response.into_body()).await?.as_ref())
            .context("Unable to parse content string")?
            .to_owned(),
    })
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
