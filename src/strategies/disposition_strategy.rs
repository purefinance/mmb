use std::sync::Arc;

use anyhow::Result;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::core::disposition_execution::{
    PriceSlot, TradeCycle, TradeDisposition, TradingContext, TradingContextBySide,
};
use crate::core::exchanges::common::{
    Amount, CurrencyPair, ExchangeAccountId, TradePlace, TradePlaceAccount,
};
use crate::core::exchanges::general::currency_pair_metadata::Round;
use crate::core::explanation::{Explanation, OptionExplanationAddReasonExt, WithExplanation};
use crate::core::infrastructure::WithExpect;
use crate::core::lifecycle::cancellation_token::CancellationToken;
use crate::core::lifecycle::trading_engine::EngineContext;
use crate::core::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::core::orders::order::{OrderRole, OrderSide, OrderSnapshot};
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use crate::core::DateTime;

pub trait DispositionStrategy: Send + Sync + 'static {
    fn calculate_trading_context(
        &mut self,
        max_amount: Decimal,
        now: DateTime,
        local_snapshots_service: &LocalSnapshotsService,
        explanation: &mut Explanation,
    ) -> Option<TradingContext>;

    fn handle_order_fill(
        &self,
        cloned_order: &Arc<OrderSnapshot>,
        price_slot: &PriceSlot,
        target_eai: ExchangeAccountId,
        cancellation_token: CancellationToken,
    ) -> Result<()>;
}

pub struct ExampleStrategy {
    target_eai: ExchangeAccountId,
    currency_pair: CurrencyPair,
    spread: Decimal,
    engine_context: Arc<EngineContext>,
    configuration_descriptor: Arc<ConfigurationDescriptor>,
}

impl ExampleStrategy {
    pub fn new(
        target_eai: ExchangeAccountId,
        currency_pair: CurrencyPair,
        spread: Decimal,
        engine_ctx: Arc<EngineContext>,
    ) -> Self {
        ExampleStrategy {
            target_eai,
            currency_pair,
            spread,
            engine_context: engine_ctx,
            configuration_descriptor: Arc::new(ConfigurationDescriptor::new(
                "ExampleStrategy".into(),
                format!("{};{}", target_eai, currency_pair.as_str()),
            )),
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
        mut max_amount: Amount,
        _now: DateTime,
        local_snapshots_service: &LocalSnapshotsService,
        mut explanation: Explanation,
    ) -> Option<TradingContextBySide> {
        let snapshot = local_snapshots_service.get_snapshot(self.trade_place())?;
        let ask_min_price = snapshot.get_top_ask()?.0;
        let bid_max_price = snapshot.get_top_bid()?.0;

        let current_spread = ask_min_price - bid_max_price;

        let price = if current_spread < self.spread {
            let order_book_middle = (bid_max_price + ask_min_price) * dec!(0.5);
            let currency_pair_metadata = self
                .engine_context
                .exchanges
                .get(&self.target_eai)?
                .symbols
                .get(&self.currency_pair)?
                .clone();

            match side {
                OrderSide::Sell => {
                    let price = order_book_middle + (current_spread * dec!(0.5));
                    currency_pair_metadata
                        .price_round(price, Round::Ceiling)
                        .ok()?
                }
                OrderSide::Buy => {
                    let price = order_book_middle - (current_spread * dec!(0.5));
                    currency_pair_metadata
                        .price_round(price, Round::Floor)
                        .ok()?
                }
            }
        } else {
            snapshot.get_top(side)?.0
        };

        let engine_context = self.engine_context.clone();

        let exchanges = engine_context
            .exchanges
            .get(&self.target_eai)
            .with_expect(|| {
                format!(
                    "Failed to get exchange with ExchangeAccountId: {}",
                    self.target_eai
                )
            });

        let symbol = exchanges.symbols.get(&self.currency_pair).with_expect(|| {
            format!(
                "Failed to get symbols with CurrencyPair: {}",
                self.currency_pair
            )
        });

        explanation = {
            let mut explanation = Some(explanation);

            // TODO: fix it issue 259
            log::info!(
                "max_amount: {} (delete this after fix issue 259)",
                max_amount
            );
            max_amount = self
                .engine_context
                .balance_manager
                .lock()
                .get_leveraged_balance_in_amount_currency_code(
                    self.configuration_descriptor.clone(),
                    side,
                    self.target_eai,
                    symbol.clone(),
                    price,
                    &mut explanation,
                )
                .with_expect(|| format!("Failed to get balance for {}", self.target_eai));
            explanation.add_reason(format!(
                "max_amount changed to {} because target balance wasn't enough",
                max_amount
            ));

            // This expect can happened if get_leveraged_balance_in_amount_currency_code() sets the explanation to None
            explanation.expect(
                "ExampleStrategy::calc_trading_context_by_side(): Explanation should be non None here"
            )
        };

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
                        max_amount,
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
}
