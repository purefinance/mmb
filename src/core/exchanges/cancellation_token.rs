use futures::future;
pub struct CancellationToken {}

impl CancellationToken {
    pub async fn when_cancelled_stub() -> future::Pending<()> {
        future::pending().await
    }
}
