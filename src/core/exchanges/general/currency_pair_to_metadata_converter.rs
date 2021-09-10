use std::collections::HashMap;
use std::sync::Arc;

use crate::core::exchanges::common::CurrencyPair;
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::general::currency_pair_metadata::CurrencyPairMetadata;
use crate::core::exchanges::general::exchange::Exchange;

use anyhow::{bail, Result};

#[derive(Clone)]
pub struct CurrencyPairToMetadataConverter {
    exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,
}

impl CurrencyPairToMetadataConverter {
    pub(crate) fn new(exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>) -> Self {
        Self { exchanges_by_id }
    }

    pub(crate) fn try_get_currency_pair_metadata(
        &self,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
    ) -> Result<Arc<CurrencyPairMetadata>> {
        match self.exchanges_by_id.get(exchange_account_id) {
            Some(exchange) => return exchange.get_currency_pair_metadata(currency_pair),
            None => bail!(
                "try_get_currency_pair_metadata failed to get exchange by id: {}",
                exchange_account_id
            ),
        }
    }
}
