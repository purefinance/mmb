use actix::System;

pub type HttpParams = Vec<(String, String)>;

pub async fn send_post_request(url: &str, api_key: &str, parameters: HttpParams) {
    let client = awc::Client::default();
    let response = client
        .post(url)
        .header("X-MBX-APIKEY", api_key)
        .send_form(&parameters)
        .await;
    dbg!(&response.unwrap().body().await);

    System::current().stop();
}
