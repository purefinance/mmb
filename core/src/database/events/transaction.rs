use crate::misc::time::time_manager;
use mmb_database::impl_event;
use mmb_domain::market::ExchangeId;
use mmb_domain::market::MarketId;
use mmb_domain::order::snapshot::{Amount, Price};
use mmb_domain::order::snapshot::{ExchangeOrderId, OrderSide};
use mmb_utils::DateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionTradeDirection {
    Target,
    Hedge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionTrade {
    pub exchange_order_id: ExchangeOrderId,
    pub exchange_id: ExchangeId,
    pub price: Option<Price>,
    pub amount: Amount,
    pub side: Option<OrderSide>,
}

pub type TransactionId = Uuid;

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// when got postponed fill
    New,

    /// when starting hedging
    Hedging,

    /// when waiting trailing stop
    Trailing,

    /// if hedging stopped by timeout
    Timeout,

    /// when stop loss completed
    StopLoss,

    /// if order successfully hedged or catch exception
    Finished,
}

impl TransactionStatus {
    pub fn is_finished(&self) -> bool {
        use TransactionStatus::*;
        matches!(self, StopLoss | Finished)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionSnapshot {
    revision: u64,
    transaction_id: TransactionId,
    transaction_creation_time: DateTime,
    pub market_id: MarketId,
    pub side: OrderSide,
    pub price: Option<Price>,
    pub amount: Amount,
    pub status: TransactionStatus,
    // name of strategy for showing on website
    pub strategy_name: String,
    pub hedged: Option<Amount>,
    pub profit_loss_pct: Option<Amount>,
    pub trades: Vec<TransactionTrade>,
}

impl_event!(&mut TransactionSnapshot, "transactions");

impl TransactionSnapshot {
    pub fn new(
        market_id: MarketId,
        side: OrderSide,
        price: Option<Price>,
        amount: Amount,
        status: TransactionStatus,
        strategy_name: String,
    ) -> Self {
        TransactionSnapshot {
            revision: 1,
            transaction_id: Uuid::new_v4(),
            transaction_creation_time: time_manager::now(),
            market_id,
            side,
            price,
            amount,
            status,
            strategy_name,
            hedged: None,
            profit_loss_pct: None,
            trades: vec![],
        }
    }

    pub fn revisions(&self) -> u64 {
        self.revision
    }

    pub fn increment_revision(&mut self) {
        self.revision += 1;
    }

    pub fn transaction_id(&self) -> TransactionId {
        self.transaction_id
    }

    pub fn creation_time(&self) -> DateTime {
        self.transaction_creation_time
    }
}

pub mod transaction_service {
    use crate::database::events::recorder::EventRecorder;
    use crate::database::events::transaction::{TransactionSnapshot, TransactionStatus};
    use anyhow::Context;

    pub fn save(
        transaction: &mut TransactionSnapshot,
        status: TransactionStatus,
        event_recorder: &EventRecorder,
    ) -> anyhow::Result<()> {
        transaction.status = status;
        transaction.increment_revision();

        event_recorder
            .save(transaction)
            .context("in transaction_service::save()")
    }
}
