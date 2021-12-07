use anyhow::Result;
use futures::Future;
use tokio::time::Duration;

pub async fn with_timeout<T, Fut>(timeout: Duration, fut: Fut) -> Result<T>
where
    Fut: Future<Output = Result<T>>,
{
    tokio::select! {
        result = fut => result,
        _ = tokio::time::sleep(timeout) => panic!("Timeout {} ms is exceeded", timeout.as_millis()),
    }
}
