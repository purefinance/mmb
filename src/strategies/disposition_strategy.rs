use std::sync::Arc;

use anyhow::Result;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::core::disposition_execution::{
    PriceSlot, TradeCycle, TradeDisposition, TradingContext, TradingContextBySide,
};
use crate::core::exchanges::common::{
    CurrencyPair, ExchangeAccountId, TradePlace, TradePlaceAccount,
};
use crate::core::explanation::{Explanation, WithExplanation};
use crate::core::lifecycle::cancellation_token::CancellationToken;
use crate::core::lifecycle::trading_engine::EngineContext;
use crate::core::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::core::orders::order::{OrderRole, OrderSide, OrderSnapshot};
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
        target_eai: &ExchangeAccountId,
        cancellation_token: CancellationToken,
    ) -> Result<()>;
}

pub struct ExampleStrategy {
    target_eai: ExchangeAccountId,
    currency_pair: CurrencyPair,
    spread: Decimal,
    _engine_context: Arc<EngineContext>,
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
            _engine_context: engine_ctx,
        }
    }

    fn strategy_name() -> &'static str {
        "ExampleStrategy"
    }

    fn trade_place_account(&self) -> TradePlaceAccount {
        TradePlaceAccount::new(self.target_eai.clone(), self.currency_pair.clone())
    }

    fn trade_place(&self) -> TradePlace {
        self.trade_place_account().trade_place()
    }

    fn calc_trading_context_by_side(
        &mut self,
        side: OrderSide,
        max_amount: Decimal,
        _now: DateTime,
        local_snapshots_service: &LocalSnapshotsService,
        explanation: Explanation,
    ) -> Option<TradingContextBySide> {
        let snapshot = local_snapshots_service.get_snapshot(self.trade_place())?;
        let ask_min_price = snapshot.get_top_ask()?.0;
        let bid_max_price = snapshot.get_top_bid()?.0;

        let order_book_middle = (bid_max_price + ask_min_price) * dec!(0.5);
        let current_spread = ask_min_price - bid_max_price;

        let price = if current_spread < self.spread {
            order_book_middle + (current_spread * dec!(0.5))
        } else {
            snapshot.get_top(side)?.0
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
                explanation: explanation.clone(),
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
        _target_eai: &ExchangeAccountId,
        _cancellation_token: CancellationToken,
    ) -> Result<()> {
        // TODO save order fill info in Database
        Ok(())
    }
}
