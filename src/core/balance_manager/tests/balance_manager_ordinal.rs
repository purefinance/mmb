#[cfg(test)]
use std::{collections::HashMap, sync::Arc};

use crate::core::{
    balance_manager::balance_manager::BalanceManager,
    exchanges::{
        common::{Amount, CurrencyCode, CurrencyId, ExchangeAccountId},
        general::{
            currency_pair_metadata::{CurrencyPairMetadata, Precision},
            currency_pair_to_currency_metadata_converter::CurrencyPairToCurrencyMetadataConverter,
            exchange::Exchange,
            test_helper::get_test_exchange_with_currency_pair_metadata,
        },
    },
    orders::{
        fill::{OrderFill, OrderFillType},
        order::OrderFillRole,
    },
    DateTime,
};
use chrono::Utc;
use uuid::Uuid;

use crate::core::balance_manager::tests::balance_manager_base::BalanceManagerBase;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub struct BalanceManagerOrdinal {
    pub balance_manager_base: BalanceManagerBase,
    now: DateTime,
}

// static
impl BalanceManagerOrdinal {
    fn create_balance_manager() -> (Arc<CurrencyPairMetadata>, BalanceManager) {
        let (currency_pair_metadata, exchanges_by_id) =
            BalanceManagerOrdinal::create_balance_manager_ctor_parameters();
        let currency_pair_to_currency_pair_metadata_converter =
            CurrencyPairToCurrencyMetadataConverter::new(exchanges_by_id.clone());

        let balance_manager = BalanceManager::new(
            exchanges_by_id.clone(),
            currency_pair_to_currency_pair_metadata_converter,
        );
        (currency_pair_metadata, balance_manager)
    }

    fn create_balance_manager_ctor_parameters() -> (
        Arc<CurrencyPairMetadata>,
        HashMap<ExchangeAccountId, Arc<Exchange>>,
    ) {
        let base_currency_code = BalanceManagerBase::eth();
        let quote_currency_code = BalanceManagerBase::btc();
        let currency_pair_metadata = Arc::from(CurrencyPairMetadata::new(
            false,
            false,
            base_currency_code.as_str().into(),
            base_currency_code.as_str().into(),
            quote_currency_code.as_str().into(),
            quote_currency_code.as_str().into(),
            None,
            None,
            quote_currency_code.as_str().into(),
            None,
            None,
            None,
            Some(base_currency_code.as_str().into()),
            Precision::ByTick { tick: dec!(0.1) },
            Precision::ByTick { tick: dec!(3) },
        ));

        let exchange =
            get_test_exchange_with_currency_pair_metadata(currency_pair_metadata.clone()).0;

        let mut res = HashMap::new();
        res.insert(exchange.exchange_account_id.clone(), exchange);
        let exchange =
            get_test_exchange_with_currency_pair_metadata(currency_pair_metadata.clone()).0;
        res.insert(exchange.exchange_account_id.clone(), exchange);

        (currency_pair_metadata, res)
    }

    fn new() -> Self {
        let (currency_pair_metadata, balance_manager) =
            BalanceManagerOrdinal::create_balance_manager();
        let mut balance_manager_base = BalanceManagerBase::new();
        balance_manager_base.set_balance_manager(balance_manager);
        balance_manager_base.set_currency_pair_metadata(currency_pair_metadata);
        Self {
            balance_manager_base,
            now: Utc::now(),
        }
    }
}

impl BalanceManagerOrdinal {
    fn create_order_fill(price: Decimal, amount: Amount, cost: Decimal) -> OrderFill {
        OrderFill::new(
            Uuid::new_v4(),
            Utc::now(),
            OrderFillType::Liquidation, // TODO: grays QA is it default?
            None,
            price,
            amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new(BalanceManagerBase::bnb().into()),
            dec!(0.1),
            dec!(0.1),
            CurrencyCode::new(BalanceManagerBase::bnb().into()),
            dec!(0.1),
            dec!(0.1),
            false,
            None,
            None,
        )
    }

    pub fn balance_manager(&self) -> &BalanceManager {
        self.balance_manager_base.balance_manager()
    }
    pub fn balance_manager_mut(&mut self) -> &mut BalanceManager {
        self.balance_manager_base.balance_manager_mut()
    }
}
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    use crate::core::exchanges::common::{CurrencyCode, ExchangeId};
    use crate::core::logger::init_logger;
    use crate::core::orders::order::{OrderSide, ReservationId};
    use crate::core::{
        balance_manager::tests::balance_manager_base::BalanceManagerBase,
        exchanges::common::ExchangeAccountId, misc::make_hash_map::make_hash_map,
    };

    use super::BalanceManagerOrdinal;

    #[test]
    pub fn balance_was_received_not_existing_exchange_account_id() {
        init_logger();
        let mut test_object = BalanceManagerOrdinal::new();
        assert_eq!(
            test_object
                .balance_manager()
                .balance_was_received(&ExchangeAccountId::new("NotExistingExchangeId".into(), 0)),
            false
        );
    }

    #[test]
    pub fn balance_was_received_existing_exchange_account_id_without_currency() {
        init_logger();
        let test_object = BalanceManagerOrdinal::new();
        assert_eq!(
            test_object
                .balance_manager()
                .balance_was_received(&test_object.balance_manager_base.exchange_account_id),
            false
        );
    }

    #[test]
    pub fn balance_was_received_existing_exchange_account_id_with_currency() {
        init_logger();
        let mut test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = &test_object.balance_manager_base.exchange_account_id.clone();
        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            &exchange_account_id,
            make_hash_map(CurrencyCode::new(BalanceManagerBase::btc().into()), dec!(2)),
        );

        assert_eq!(
            test_object
                .balance_manager()
                .balance_was_received(&test_object.balance_manager_base.exchange_account_id),
            true
        );
    }

    #[test]
    pub fn update_exchange_balance_skip_currencies_with_zero_balance_which_are_not_part_of_currency_pairs(
    ) {
        init_logger();
        let mut test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = &test_object.balance_manager_base.exchange_account_id.clone();
        let mut balance_map: HashMap<CurrencyCode, Decimal> = HashMap::new();
        let btc_currency_code: CurrencyCode = BalanceManagerBase::btc().as_str().into();
        let eth_currency_code: CurrencyCode = BalanceManagerBase::eth().as_str().into();
        let bnb_currency_code: CurrencyCode = BalanceManagerBase::bnb().as_str().into();
        let eos_currency_code: CurrencyCode = "EOS".into();
        balance_map.insert(btc_currency_code.clone(), dec!(2));
        balance_map.insert(eth_currency_code.clone(), dec!(1));
        balance_map.insert(bnb_currency_code.clone(), dec!(7.5));
        balance_map.insert(eos_currency_code.clone(), dec!(0));

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            exchange_account_id,
            balance_map,
        );

        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.currency_pair_metadata(),
                &btc_currency_code
            ),
            Some(dec!(2))
        );

        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.currency_pair_metadata(),
                &eth_currency_code
            ),
            Some(dec!(1))
        );

        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.currency_pair_metadata(),
                &bnb_currency_code
            ),
            Some(dec!(7.5))
        );

        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.currency_pair_metadata(),
                &eos_currency_code
            ),
            None
        );
    }
}
