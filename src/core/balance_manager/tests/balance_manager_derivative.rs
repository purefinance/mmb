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
};
use chrono::Utc;
use uuid::Uuid;

use crate::core::balance_manager::tests::balance_manager_base::BalanceManagerBase;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub struct BalanceManagerDerivative {
    balance_manager_base: BalanceManagerBase,
    exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,
}

// static
impl BalanceManagerDerivative {
    pub fn price() -> Decimal {
        dec!(0.2)
    }
    pub fn amount() -> Amount {
        dec!(1.9)
    }
    pub fn leverage() -> Decimal {
        dec!(7)
    }
    fn position() -> Decimal {
        dec!(1)
    }

    fn create_balance_manager() -> (
        Arc<CurrencyPairMetadata>,
        BalanceManager,
        HashMap<ExchangeAccountId, Arc<Exchange>>,
    ) {
        let (currency_pair_metadata, exchanges_by_id) =
            BalanceManagerDerivative::create_balance_manager_ctor_parameters();
        let currency_pair_to_currency_pair_metadata_converter =
            CurrencyPairToCurrencyMetadataConverter::new(exchanges_by_id.clone());

        let balance_manager = BalanceManager::new(
            exchanges_by_id.clone(),
            currency_pair_to_currency_pair_metadata_converter,
        );
        (currency_pair_metadata, balance_manager, exchanges_by_id)
    }

    fn create_balance_manager_ctor_parameters() -> (
        Arc<CurrencyPairMetadata>,
        HashMap<ExchangeAccountId, Arc<Exchange>>,
    ) {
        let base_currency_code = BalanceManagerBase::eth();
        let quote_currency_code = BalanceManagerBase::btc();
        let currency_pair_metadata = Arc::from(CurrencyPairMetadata::new(
            false,
            true,
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
        // exchange.set_symbols(vec![currency_pair_metadata.clone()]);

        let mut res = HashMap::new();
        res.insert(exchange.exchange_account_id.clone(), exchange);
        let exchange =
            get_test_exchange_with_currency_pair_metadata(currency_pair_metadata.clone()).0;
        res.insert(exchange.exchange_account_id.clone(), exchange);

        (currency_pair_metadata, res)
    }

    fn new() -> Self {
        let (currency_pair_metadata, balance_manager, exchanges_by_id) =
            BalanceManagerDerivative::create_balance_manager();
        let mut balance_manager_base = BalanceManagerBase::new();
        balance_manager_base.set_balance_manager(balance_manager);
        balance_manager_base.set_currency_pair_metadata(currency_pair_metadata);
        Self {
            balance_manager_base,
            exchanges_by_id,
        }
    }
    fn create_order_fill(
        price: Decimal,
        amount: Amount,
        cost: Decimal,
        commission_amount: Decimal,
    ) -> OrderFill {
        OrderFill::new(
            Uuid::new_v4(),
            Utc::now(),
            OrderFillType::Liquidation, // TODO: grays QA is it default?
            None,
            price,
            amount,
            cost,
            OrderFillRole::Taker,
            BalanceManagerBase::eth(),
            commission_amount,
            dec!(0),
            BalanceManagerBase::btc(),
            dec!(0),
            dec!(0),
            false,
            None,
            None,
        )
    }
}
