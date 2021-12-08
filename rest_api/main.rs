use jsonrpc_core_client::transports::ipc;
use shared::rest_api::gen_client;

async fn foo() {
    let p = std::path::Path::new("/tmp/mmb.ipc");
    let client = ipc::connect::<_, gen_client::Client>(p)
        .await
        .expect("grays 2");
    let a = client.health().await.unwrap();
    println!("a = {}", a);

    let a = client.get_config().await.unwrap();
    println!("a = {}", a);
    let a = client.stats().await.unwrap();
    println!("a = {}", a);
    let a = client.stop().await.unwrap();
    println!("a = {}", a);
    let a = client.health().await.unwrap();
    println!("a = {}", a);
}

#[actix_web::main]
async fn main() {
    foo().await;
}
