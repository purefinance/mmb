use mockall_double::double;

#[double]
use crate::core::misc::time_manager::time_manager;

use crate::core::{
    exchanges::common::TradePlace,
    misc::{price_by_order_side::PriceByOrderSide, price_source_model::PriceSourceModel},
};

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

    pub fn save(&mut self, trade_place: TradePlace, prices: PriceByOrderSide) {
        let _prices_source = PriceSourceModel::new(
            time_manager::now(),
            trade_place.exchange_id,
            trade_place.currency_pair,
            prices.top_bid,
            prices.top_ask,
        );
        //     _dataRecorder.Save(priceSource);
    }
}
