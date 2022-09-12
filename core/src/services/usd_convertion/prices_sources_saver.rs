use crate::database::events::recorder::EventRecorder;
use mmb_domain::market::MarketId;
use mmb_domain::order::snapshot::PriceByOrderSide;
use mockall_double::double;

#[double]
use crate::misc::time::time_manager;

use crate::misc::price_source_model::PriceSourceModel;

pub struct PriceSourcesSaver {
    event_recorder: EventRecorder,
}

impl PriceSourcesSaver {
    pub fn new(event_recorder: EventRecorder) -> Self {
        Self { event_recorder }
    }

    pub fn save(&mut self, market_id: MarketId, prices: PriceByOrderSide) {
        let prices_source = PriceSourceModel::new(
            time_manager::now(),
            market_id.exchange_id,
            market_id.currency_pair,
            prices.top_bid,
            prices.top_ask,
        );
        self.event_recorder
            .save(prices_source)
            .expect("Failure save prices_source");
    }
}
