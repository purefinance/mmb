use mockall_double::double;

#[double]
use crate::misc::time::time_manager;

use crate::{
    exchanges::common::MarketId,
    misc::{price_by_order_side::PriceByOrderSide, price_source_model::PriceSourceModel},
};

#[derive(Default)]
pub struct PriceSourcesSaver {
    // TODO: implement when DataRecorder will be added
// data_recorder: DataRecorder;
}

impl PriceSourcesSaver {
    pub fn new(// data_recorder: DataRecorder
    ) -> Self {
        Self{
            // data_recorder
        }
    }

    pub fn save(&mut self, market_id: MarketId, prices: PriceByOrderSide) {
        let _prices_source = PriceSourceModel::new(
            time_manager::now(),
            market_id.exchange_id,
            market_id.currency_pair,
            prices.top_bid,
            prices.top_ask,
        );
        //     _dataRecorder.Save(priceSource);
    }
}
