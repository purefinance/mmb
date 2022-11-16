use anyhow::Result;
use itertools::Itertools;
use mmb_core::balance::manager::balance_manager::BalanceManager;
use mmb_core::disposition_execution::strategy::DispositionStrategy;
use mmb_core::disposition_execution::{
    PriceSlot, TradeCycle, TradeDisposition, TradingContext, TradingContextBySide,
};
use mmb_core::explanation::{Explanation, WithExplanation};
use mmb_core::lifecycle::trading_engine::EngineContext;
use mmb_core::order_book::local_snapshot_service::LocalSnapshotsService;
use mmb_core::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use mmb_core::settings::{CurrencyPairSetting, DispositionStrategySettings};
use mmb_domain::events::ExchangeEvent;
use mmb_domain::exchanges::symbol::Round;
use mmb_domain::market::CurrencyPair;
use mmb_domain::market::{ExchangeAccountId, MarketAccountId, MarketId};
use mmb_domain::order::snapshot::Amount;
use mmb_domain::order::snapshot::{OrderRole, OrderSide, OrderSnapshot};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::DateTime;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ExampleStrategySettings {
    pub spread: Decimal,
    pub currency_pair: CurrencyPairSetting,
    pub max_amount: Decimal,
    pub exchange_account_id: ExchangeAccountId,
}

impl DispositionStrategySettings for ExampleStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId {
        self.exchange_account_id
    }

    fn currency_pair(&self) -> CurrencyPair {
        if let CurrencyPairSetting::Ordinary { base, quote } = self.currency_pair {
            CurrencyPair::from_codes(base, quote)
        } else {
            panic!(
                "Incorrect currency pair setting enum type {:?}",
                self.currency_pair
            );
        }
    }

    // Max amount for orders that will be created
    fn max_amount(&self) -> Amount {
        self.max_amount
    }
}

pub struct ExampleStrategy {
    target_eai: ExchangeAccountId,
    currency_pair: CurrencyPair,
    spread: Decimal,
    engine_context: Arc<EngineContext>,
    configuration_descriptor: ConfigurationDescriptor,
    max_amount: Decimal,
}

impl ExampleStrategy {
    pub fn new(
        target_eai: ExchangeAccountId,
        currency_pair: CurrencyPair,
        spread: Decimal,
        max_amount: Decimal,
        engine_context: Arc<EngineContext>,
    ) -> Box<Self> {
        let configuration_descriptor = ConfigurationDescriptor::new(
            "ExampleStrategy".into(),
            format!("{target_eai};{currency_pair}").as_str().into(),
        );

        // amount_limit it's a limit for position changing for both sides
        // it's equal to half of the max amount because an order that can change a position from
        // a limit by sells to a limit by buys is possible
        let amount_limit = max_amount * dec!(0.5);

        let symbol = engine_context
            .exchanges
            .get(&target_eai)
            .with_expect(|| format!("failed to get exchange from trading_engine for {target_eai}"))
            .symbols
            .get(&currency_pair)
            .with_expect(|| format!("failed to get symbol from exchange for {currency_pair}"))
            .clone();

        engine_context
            .balance_manager
            .lock()
            .set_target_amount_limit(configuration_descriptor, target_eai, symbol, amount_limit);

        Box::new(ExampleStrategy {
            target_eai,
            currency_pair,
            spread,
            engine_context,
            configuration_descriptor,
            max_amount,
        })
    }

    fn strategy_name() -> &'static str {
        "ExampleStrategy"
    }

    fn market_account_id(&self) -> MarketAccountId {
        MarketAccountId::new(self.target_eai, self.currency_pair)
    }

    fn market_id(&self) -> MarketId {
        self.market_account_id().market_id()
    }

    fn calc_trading_context_by_side(
        &mut self,
        side: OrderSide,
        _now: DateTime,
        local_snapshots_service: &LocalSnapshotsService,
        mut explanation: Explanation,
    ) -> Option<TradingContextBySide> {
        let snapshot = local_snapshots_service.get_snapshot(self.market_id())?;
        let ask_min_price = snapshot.get_top_ask()?.0;
        let bid_max_price = snapshot.get_top_bid()?.0;

        let current_spread = ask_min_price - bid_max_price;

        let symbol = self
            .engine_context
            .exchanges
            .get(&self.target_eai)?
            .symbols
            .get(&self.currency_pair)?
            .clone();

        let price = if current_spread < self.spread {
            let order_book_middle = (bid_max_price + ask_min_price) * dec!(0.5);

            match side {
                OrderSide::Sell => {
                    let price = order_book_middle + (self.spread * dec!(0.5));
                    symbol.price_round(price, Round::Ceiling)
                }
                OrderSide::Buy => {
                    let price = order_book_middle - (self.spread * dec!(0.5));
                    symbol.price_round(price, Round::Floor)
                }
            }
        } else {
            snapshot.get_top(side)?.0
        };

        let amount;
        explanation = {
            let mut explanation = Some(explanation);

            // TODO: delete deep_clone
            let orders = self
                .engine_context
                .exchanges
                .iter()
                .flat_map(|x| {
                    x.orders
                        .not_finished
                        .iter()
                        .map(|y| y.clone())
                        .collect_vec()
                })
                .collect_vec();

            let balance_manager = BalanceManager::clone_and_subtract_not_approved_data(
                self.engine_context.balance_manager.clone(),
                Some(&mut orders.iter()),
            )
            .expect("ExampleStrategy::calc_trading_context_by_side: failed to clone and subtract not approved data for BalanceManager");

            amount = balance_manager
                .lock()
                .get_leveraged_balance_in_amount_currency_code(
                    self.configuration_descriptor,
                    side,
                    self.target_eai,
                    symbol.clone(),
                    price,
                    &mut explanation,
                )
                .with_expect(|| format!("Failed to get balance for {}", self.target_eai));

            // This expect can happened if get_leveraged_balance_in_amount_currency_code() sets the explanation to None
            explanation.expect(
                "ExampleStrategy::calc_trading_context_by_side(): Explanation should be non None here"
            )
        };

        let amount = symbol.amount_round(amount, Round::Floor);

        Some(TradingContextBySide {
            max_amount: self.max_amount,
            estimating: vec![WithExplanation {
                value: Some(TradeCycle {
                    order_role: OrderRole::Maker,
                    strategy_name: Self::strategy_name().to_string(),
                    disposition: TradeDisposition::new(
                        self.market_account_id(),
                        side,
                        price,
                        amount,
                    ),
                }),
                explanation,
            }],
        })
    }
}

impl DispositionStrategy for ExampleStrategy {
    fn calculate_trading_context(
        &mut self,
        _: &ExchangeEvent,
        now: DateTime,
        local_snapshots_service: &LocalSnapshotsService,
        explanation: &mut Explanation,
    ) -> Option<TradingContext> {
        let buy_trading_ctx = self.calc_trading_context_by_side(
            OrderSide::Buy,
            now,
            local_snapshots_service,
            explanation.clone(),
        )?;

        let sell_trading_ctx = self.calc_trading_context_by_side(
            OrderSide::Sell,
            now,
            local_snapshots_service,
            explanation.clone(),
        )?;

        Some(TradingContext::new(buy_trading_ctx, sell_trading_ctx))
    }

    fn handle_order_fill(
        &self,
        _cloned_order: &Arc<OrderSnapshot>,
        _price_slot: &PriceSlot,
        _target_eai: ExchangeAccountId,
        _cancellation_token: CancellationToken,
    ) -> Result<()> {
        // TODO save order fill info in Database
        Ok(())
    }

    fn configuration_descriptor(&self) -> ConfigurationDescriptor {
        self.configuration_descriptor
    }
}
