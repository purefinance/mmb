use actix::{Actor, Context, Handler, Message, System};
use std::collections::HashMap;

pub async fn send_post_request(url: &str, api_key: &str, parameters: HashMap<String, String>) {
    let client = awc::Client::default();
    let response = client
        .post(url)
        .header("X-MBX-APIKEY", api_key)
        .send_form(&parameters)
        .await;
    dbg!(&response.unwrap().body().await);

    System::current().stop();
}
