use futures::future;
pub struct CancellationToken {}

impl CancellationToken {
    // TODO it's just a stub now
    pub async fn when_cancelled() -> future::Pending<()> {
        future::pending().await
    }
}
