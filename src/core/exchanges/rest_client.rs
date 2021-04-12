use std::pin::Pin;

use super::common::*;
use actix_web::{client::SendRequestError, dev::Decompress, error::PayloadError};
use anyhow::{bail, Context, Result};
use awc::ClientResponse;

pub type HttpParams = Vec<(String, String)>;

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

// Inner awc types. Needed just for unified response handling in hande_response()
type PinnedStream =
    Pin<Box<dyn futures::Stream<Item = std::result::Result<bytes::Bytes, PayloadError>>>>;
type ResponseType = std::result::Result<
    ClientResponse<Decompress<actix_web::dev::Payload<PinnedStream>>>,
    SendRequestError,
>;
async fn handle_response(response: ResponseType, rest_action: &str) -> Result<RestRequestOutcome> {
    match response {
        Ok(mut response) => Ok(RestRequestOutcome {
            content: std::str::from_utf8(
                &response
                    .body()
                    .await
                    .context("Unable to get response body")?,
            )
            .context("Unable to parse content string")?
            .to_owned(),
            status: response.status(),
        }),
        Err(error) => {
            bail!("Unable to send {} request: {}", rest_action, error)
        }
    }
}

pub async fn send_post_request(
    url: &str,
    api_key: &str,
    parameters: &HttpParams,
) -> Result<RestRequestOutcome> {
    // let client = awc::Client::default();
    // let response = client
    //     .post(url)
    //     .header("X-MBX-APIKEY", api_key)
    //     .send_form(&parameters)
    //     .await;
    //
    // handle_response(response, "POST").await

    todo!()
}

pub async fn send_delete_request(
    url: &str,
    api_key: &str,
    parameters: &HttpParams,
) -> Result<RestRequestOutcome> {
    let client = awc::Client::default();
    let response = client
        .delete(url)
        .header("X-MBX-APIKEY", api_key)
        .send_form(&parameters)
        .await;

    handle_response(response, "DELETE").await
}

// TODO not implemented correctly
pub async fn send_get_request(
    url: &str,
    api_key: &str,
    parameters: &HttpParams,
) -> Result<RestRequestOutcome> {
    let client = hyper::Client::new();

    let req = hyper::Request::get(url)
        .header("X-MBX-APIKEY", api_key)
        .body(hyper::body::Body::empty())
        .context("Error during creation of http GET request")?;
    let response = client.request(req).await?;

    todo!()

    //     let client = awc::Client::default();
    //     let response = client
    //         .get(url)
    //         .header("X-MBX-APIKEY", api_key)
    //         .query(&parameters)
    //         .context("Unable to add query")?
    //         .send()
    //         .await;
    //
    //     handle_response(response, "GET").await
}
