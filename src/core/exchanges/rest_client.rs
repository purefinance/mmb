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
}

pub async fn send_delete_request(url: &str, parameters: HttpParams) {
    let client = awc::Client::default();
    let response = client.delete(url).send_form(&parameters).await;
    //dbg!(&response.unwrap().body().await);
    match response {
        Ok(_) => {
            dbg!(&"OK");
        }
        Err(_) => {
            dbg!(&"Not OK");
        }
    }

    System::current().stop();
}
