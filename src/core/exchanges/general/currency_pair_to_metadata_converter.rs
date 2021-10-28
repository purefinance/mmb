#[cfg(test)]
use mockall::automock;
#[cfg(test)]
use parking_lot::{Mutex, MutexGuard};

use std::collections::HashMap;
use std::sync::Arc;

use crate::core::exchanges::common::CurrencyPair;
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::general::currency_pair_metadata::CurrencyPairMetadata;
use crate::core::exchanges::general::exchange::Exchange;

#[derive(Clone)]
pub struct CurrencyPairToMetadataConverter {
    exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,
}

#[cfg_attr(test, automock)]
impl CurrencyPairToMetadataConverter {
    pub(crate) fn new(exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>) -> Arc<Self> {
        Arc::new(Self { exchanges_by_id })
    }

    pub(crate) fn get_currency_pair_metadata(
        &self,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
    ) -> Arc<CurrencyPairMetadata> {
        let exchange = self.exchanges_by_id.get(&exchange_account_id).expect(
            format!(
                "get_currency_pair_metadata failed to get exchange by id: {}",
                exchange_account_id
            )
            .as_str(),
        );
        exchange
            .get_currency_pair_metadata(currency_pair)
            .expect("failed to get currency pair")
    }

    pub(crate) fn exchanges_by_id(&self) -> HashMap<ExchangeAccountId, Arc<Exchange>> {
        self.exchanges_by_id.clone()
    }
}

#[cfg(test)]
crate::create_mock_initializer!(
    MockCurrencyPairToMetadataConverter,
    CP_TO_METADATA_LOCKER_MOCK_LOCKER
);
