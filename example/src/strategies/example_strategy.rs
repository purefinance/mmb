use std::sync::Arc;

use anyhow::Result;
use itertools::Itertools;
use mmb_utils::DateTime;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use mmb_core::core::balance_manager::balance_manager::BalanceManager;
use mmb_core::core::disposition_execution::{
    PriceSlot, TradeCycle, TradeDisposition, TradingContext, TradingContextBySide,
};
use mmb_core::core::exchanges::common::{
    Amount, CurrencyPair, ExchangeAccountId, TradePlace, TradePlaceAccount,
};
use mmb_core::core::exchanges::general::symbol::Round;
use mmb_core::core::explanation::{Explanation, OptionExplanationAddReasonExt, WithExplanation};
use mmb_core::core::infrastructure::WithExpect;
use mmb_core::core::lifecycle::trading_engine::EngineContext;
use mmb_core::core::order_book::local_snapshot_service::LocalSnapshotsService;
use mmb_core::core::orders::order::{OrderRole, OrderSide, OrderSnapshot};
use mmb_core::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use mmb_core::strategies::disposition_strategy::DispositionStrategy;
use mmb_utils::cancellation_token::CancellationToken;

pub struct ExampleStrategy {
    target_eai: ExchangeAccountId,
    currency_pair: CurrencyPair,
    spread: Decimal,
    engine_context: Arc<EngineContext>,
    configuration_descriptor: ConfigurationDescriptor,
}

impl ExampleStrategy {
    pub fn new(
        target_eai: ExchangeAccountId,
        currency_pair: CurrencyPair,
        spread: Decimal,
        engine_context: Arc<EngineContext>,
    ) -> Self {
        let configuration_descriptor = ConfigurationDescriptor::new(
            "ExampleStrategy".into(),
            (target_eai.to_string() + ";" + currency_pair.as_str())
                .as_str()
                .into(),
        );

        ExampleStrategy {
            target_eai,
            currency_pair,
            spread,
            engine_context,
            configuration_descriptor,
        }
    }

    fn strategy_name() -> &'static str {
        "ExampleStrategy"
    }

    fn trade_place_account(&self) -> TradePlaceAccount {
        TradePlaceAccount::new(self.target_eai, self.currency_pair)
    }

    fn trade_place(&self) -> TradePlace {
        self.trade_place_account().trade_place()
    }

    fn calc_trading_context_by_side(
        &mut self,
        side: OrderSide,
        max_amount: Amount,
        _now: DateTime,
        local_snapshots_service: &LocalSnapshotsService,
        mut explanation: Explanation,
    ) -> Option<TradingContextBySide> {
        let snapshot = local_snapshots_service.get_snapshot(self.trade_place())?;
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
                    let price = order_book_middle + (current_spread * dec!(0.5));
                    symbol.price_round(price, Round::Ceiling).ok()?
                }
                OrderSide::Buy => {
                    let price = order_book_middle - (current_spread * dec!(0.5));
                    symbol.price_round(price, Round::Floor).ok()?
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
                Some(orders),
            )
            .expect("ExampleStrategy::calc_trading_context_by_side: failed to clone and subtract not approved data for BalanceManager");

            amount = balance_manager
                .lock()
                .get_leveraged_balance_in_amount_currency_code(
                    self.configuration_descriptor.clone(),
                    side,
                    self.target_eai,
                    symbol.clone(),
                    price,
                    &mut explanation,
                )
                .with_expect(|| format!("Failed to get balance for {}", self.target_eai))
                .min(max_amount);

            explanation.add_reason(format!(
                "max_amount changed to {} because target balance wasn't enough",
                amount
            ));

            // This expect can happened if get_leveraged_balance_in_amount_currency_code() sets the explanation to None
            explanation.expect(
                "ExampleStrategy::calc_trading_context_by_side(): Explanation should be non None here"
            )
        };

        let amount = symbol.amount_round(amount, Round::Floor).ok()?;

        Some(TradingContextBySide {
            max_amount,
            estimating: vec![WithExplanation {
                value: Some(TradeCycle {
                    order_role: OrderRole::Maker,
                    strategy_name: Self::strategy_name().to_string(),
                    disposition: TradeDisposition::new(
                        self.trade_place_account(),
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
        max_amount: Decimal,
        now: DateTime,
        local_snapshots_service: &LocalSnapshotsService,
        explanation: &mut Explanation,
    ) -> Option<TradingContext> {
        let buy_trading_ctx = self.calc_trading_context_by_side(
            OrderSide::Buy,
            max_amount,
            now,
            local_snapshots_service,
            explanation.clone(),
        )?;

        let sell_trading_ctx = self.calc_trading_context_by_side(
            OrderSide::Sell,
            max_amount,
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
        self.configuration_descriptor.clone()
    }
}
