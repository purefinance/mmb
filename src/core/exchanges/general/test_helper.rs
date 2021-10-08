#![cfg(test)]
use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;
use rust_decimal_macros::dec;
use tokio::sync::broadcast;

use super::{
    currency_pair_metadata::CurrencyPairMetadata, currency_pair_metadata::Precision,
    exchange::Exchange,
};
use crate::core::exchanges::binance::binance::BinanceBuilder;
use crate::core::exchanges::events::ExchangeEvent;
use crate::core::exchanges::general::features::*;
use crate::core::exchanges::traits::ExchangeClientBuilder;
use crate::core::lifecycle::application_manager::ApplicationManager;
use crate::core::lifecycle::cancellation_token::CancellationToken;
use crate::core::{
    exchanges::binance::binance::Binance, exchanges::common::Amount,
    exchanges::common::CurrencyPair, exchanges::common::ExchangeAccountId,
    exchanges::common::Price, exchanges::events::AllowedEventSourceType,
    exchanges::general::commission::Commission, exchanges::general::commission::CommissionForType,
    exchanges::general::features::ExchangeFeatures, exchanges::general::features::OpenOrdersType,
    exchanges::timeouts::timeout_manager::TimeoutManager, orders::order::ClientOrderId,
    orders::order::OrderRole, orders::order::OrderSide, orders::order::OrderSnapshot,
    orders::order::OrderType, orders::pool::OrderRef, orders::pool::OrdersPool, settings,
};

pub(crate) fn get_test_exchange(
    is_derivative: bool,
) -> (Arc<Exchange>, broadcast::Receiver<ExchangeEvent>) {
    let base_currency_code = "PHB";
    let quote_currency_code = "BTC";
    get_test_exchange_by_currency_codes(is_derivative, base_currency_code, quote_currency_code)
}

pub(crate) fn get_test_exchange_by_currency_codes_and_amount_code(
    is_derivative: bool,
    base_currency_code: &str,
    quote_currency_code: &str,
    amount_currency_code: &str,
) -> (Arc<Exchange>, broadcast::Receiver<ExchangeEvent>) {
    let price_tick = dec!(0.1);
    let currency_pair_metadata = Arc::new(CurrencyPairMetadata::new(
        false,
        is_derivative,
        base_currency_code.into(),
        base_currency_code.into(),
        quote_currency_code.into(),
        quote_currency_code.into(),
        None,
        None,
        None,
        None,
        None,
        amount_currency_code.into(),
        None,
        Precision::ByTick { tick: price_tick },
        Precision::ByTick { tick: dec!(0) },
    ));
    get_test_exchange_with_currency_pair_metadata(currency_pair_metadata)
}

pub(crate) fn get_test_exchange_by_currency_codes(
    is_derivative: bool,
    base_currency_code: &str,
    quote_currency_code: &str,
) -> (Arc<Exchange>, broadcast::Receiver<ExchangeEvent>) {
    let amount_currency_code = if is_derivative {
        quote_currency_code.clone()
    } else {
        base_currency_code.clone()
    };
    get_test_exchange_by_currency_codes_and_amount_code(
        is_derivative,
        base_currency_code,
        quote_currency_code,
        amount_currency_code,
    )
}

pub(crate) fn get_test_exchange_with_currency_pair_metadata(
    currency_pair_metadata: Arc<CurrencyPairMetadata>,
) -> (Arc<Exchange>, broadcast::Receiver<ExchangeEvent>) {
    let exchange_account_id = ExchangeAccountId::new("local_exchange_account_id".into(), 0);
    get_test_exchange_with_currency_pair_metadata_and_id(
        currency_pair_metadata,
        &exchange_account_id,
    )
}
pub(crate) fn get_test_exchange_with_currency_pair_metadata_and_id(
    currency_pair_metadata: Arc<CurrencyPairMetadata>,
    exchange_account_id: &ExchangeAccountId,
) -> (Arc<Exchange>, broadcast::Receiver<ExchangeEvent>) {
    let settings = settings::ExchangeSettings::new_short(
        exchange_account_id.clone(),
        "test_api_key".into(),
        "test_secret_key".into(),
        false,
    );

    let application_manager = ApplicationManager::new(CancellationToken::new());
    let (tx, rx) = broadcast::channel(10);

    let binance = Box::new(Binance::new(
        "Binance0".parse().expect("in test"),
        settings.clone(),
        tx.clone(),
        application_manager.clone(),
        false,
    ));
    let referral_reward = dec!(40);
    let commission = Commission::new(
        CommissionForType::new(dec!(0.1), referral_reward),
        CommissionForType::new(dec!(0.2), referral_reward),
    );

    let exchange = Exchange::new(
        exchange_account_id.clone(),
        binance,
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            RestFillsFeatures::default(),
            OrderFeatures::default(),
            OrderTradeOption::default(),
            WebSocketOptions::default(),
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        BinanceBuilder.get_timeout_argments(),
        tx,
        application_manager,
        TimeoutManager::new(HashMap::new()),
        commission,
    );

    exchange
        .leverage_by_currency_pair
        .insert(currency_pair_metadata.currency_pair(), dec!(1));
    exchange
        .currencies
        .lock()
        .push(currency_pair_metadata.base_currency_code());
    exchange
        .currencies
        .lock()
        .push(currency_pair_metadata.quote_currency_code());
    exchange.symbols.insert(
        currency_pair_metadata.currency_pair(),
        currency_pair_metadata,
    );

    (exchange, rx)
}

pub(crate) fn create_order_ref(
    client_order_id: &ClientOrderId,
    role: Option<OrderRole>,
    exchange_account_id: &ExchangeAccountId,
    currency_pair: &CurrencyPair,
    price: Price,
    amount: Amount,
    side: OrderSide,
) -> OrderRef {
    let order = OrderSnapshot::with_params(
        client_order_id.clone(),
        OrderType::Liquidation,
        role,
        exchange_account_id.clone(),
        currency_pair.clone(),
        price,
        amount,
        side,
        None,
        "StrategyInUnitTests",
    );

    let order_pool = OrdersPool::new();
    order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
    let order_ref = order_pool
        .cache_by_client_id
        .get(&client_order_id)
        .expect("in test");

    order_ref.clone()
}

pub(crate) fn try_add_snapshot_by_exchange_id(exchange: &Exchange, order_ref: &OrderRef) {
    if let Some(exchange_order_id) = order_ref.exchange_order_id() {
        let _ = exchange
            .orders
            .cache_by_exchange_id
            .insert(exchange_order_id.clone(), order_ref.clone());
    }
}
