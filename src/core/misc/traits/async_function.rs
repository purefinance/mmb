use futures::{future::BoxFuture, Future};

use anyhow::Result;

pub trait AsyncFnCall
where
    Self: Send + Sync,
{
    fn call(&mut self) -> BoxFuture<'static, Result<()>>;
}

impl<T, F> AsyncFnCall for T
where
    T: FnMut() -> F,
    F: Future<Output = Result<()>> + 'static + Send + Sync,
    Self: Send + Sync,
{
    fn call(&mut self) -> BoxFuture<'static, Result<()>> {
        Box::pin(self())
    }
}
