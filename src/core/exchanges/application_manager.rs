use super::cancellation_token::CancellationToken;

#[derive(Default)]
pub struct ApplicationManager {
    pub cancellation_token: CancellationToken,
}

impl ApplicationManager {
    pub fn new(cancellation_token: CancellationToken) -> Self {
        Self { cancellation_token }
    }
}
