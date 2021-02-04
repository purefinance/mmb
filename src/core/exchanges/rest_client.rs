use super::common::*;

pub type HttpParams = Vec<(String, String)>;

pub async fn send_post_request(
    url: &str,
    api_key: &str,
    parameters: &HttpParams,
) -> RestRequestOutcome {
    let client = awc::Client::default();
    let response = client
        .post(url)
        .header("X-MBX-APIKEY", api_key)
        .send_form(&parameters)
        .await;
    let mut response = response.unwrap();

    RestRequestOutcome {
        content: std::str::from_utf8(&response.body().await.unwrap())
            .unwrap()
            .to_owned(),
        status: response.status(),
    }
}

pub async fn send_delete_request(
    url: &str,
    api_key: &str,
    parameters: &HttpParams,
) -> RestRequestOutcome {
    let client = awc::Client::default();
    let response = client
        .delete(url)
        .header("X-MBX-APIKEY", api_key)
        .send_form(&parameters)
        .await;
    let mut response = response.unwrap();

    RestRequestOutcome {
        content: std::str::from_utf8(&response.body().await.unwrap())
            .unwrap()
            .to_owned(),
        status: response.status(),
    }
}

// TODO not implemented correctly
pub async fn send_get_request(url: &str, api_key: &str, parameters: &HttpParams) {
    let client = awc::Client::default();
    let _response = client
        .get(url)
        .header("X-MBX-APIKEY", api_key)
        //.send_form(&parameters)
        .query(&parameters)
        .unwrap()
        .send()
        .await;
}
