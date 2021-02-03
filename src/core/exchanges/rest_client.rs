use super::common::*;
use serde::Deserialize;

pub type HttpParams = Vec<(String, String)>;

pub async fn send_post_request(
    url: &str,
    api_key: &str,
    parameters: HttpParams,
) -> RestRequestOutcome {
    let client = awc::Client::default();
    let response = client
        .post(url)
        .header("X-MBX-APIKEY", api_key)
        .send_form(&parameters)
        .await;
    let mut response = response.unwrap();
    //dbg!(&response.status());
    //dbg!(&response.body().await);

    //Ok("test".into())
    RestRequestOutcome {
        content: std::str::from_utf8(&response.body().await.unwrap())
            .unwrap()
            .to_owned(),
        status: response.status(),
    }
}

pub async fn send_delete_request(url: &str, api_key: &str, parameters: HttpParams) {
    let client = awc::Client::default();
    let response = client
        .delete(url)
        .header("X-MBX-APIKEY", api_key)
        .send_form(&parameters)
        .await;
    dbg!(&response.unwrap().body().await);
}

#[derive(Deserialize, Debug)]
struct Balance {
    asset: String,
    free: String,
    locked: String,
}

#[derive(Deserialize, Debug)]
struct AccountInformation {
    balances: Vec<Balance>,
    permissions: Vec<String>,
}

pub async fn send_get_request(url: &str, api_key: &str, parameters: HttpParams) {
    let client = awc::Client::default();
    dbg!(&url);
    dbg!(&api_key);
    let response = client
        .get(url)
        .header("X-MBX-APIKEY", api_key)
        //.send_form(&parameters)
        .query(&parameters)
        .unwrap()
        .send()
        .await;

    //let results: Vec<AccountInformation> = response.unwrap().json().await.unwrap();
    //dbg!(results);
    dbg!(&response.unwrap().body().await);
}
