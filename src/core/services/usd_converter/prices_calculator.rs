pub(crate) mod prices_calculator {
    use std::collections::HashMap;

    use rust_decimal_macros::dec;

    use crate::core::{
        exchanges::common::{Amount, Price, TradePlace},
        misc::price_by_order_side::PriceByOrderSide,
        order_book::local_snapshot_service::LocalSnapshotsService,
        services::usd_converter::price_source_chain::PriceSourceChain,
        DateTime,
    };

    pub(crate) fn calculate(
        src_amount: Amount,
        prices_source_chain: &PriceSourceChain,
        prices: HashMap<TradePlace, Price>,
    ) -> Price {
        calculate_amount_for_chain(src_amount, prices_source_chain, |trade_place| {
            prices.get(trade_place).cloned()
        })
        .expect("Invalid price cache")
    }

    fn calculate_amount_for_chain<F>(
        src_amount: Amount,
        prices_source_chain: &PriceSourceChain,
        calculate_price: F,
    ) -> Option<Amount>
    where
        F: Fn(&TradePlace) -> Option<Price>,
    {
        let mut rebase_price = dec!(1);

        for step in &prices_source_chain.rebase_price_steps {
            let trade_place = TradePlace::new(
                step.exchange_id.clone(),
                step.currency_pair_metadata.currency_pair(),
            );
            let calculated_price = (calculate_price)(&trade_place)?;

            match step.from_base_to_quote_currency {
                true => rebase_price *= calculated_price,
                false => rebase_price /= calculated_price,
            }
        }
        Some(rebase_price * src_amount)
    }

    pub(crate) fn convert_amount_now(
        src_amount: Amount,
        local_snapshot_service: &LocalSnapshotsService,
        prices_source_chain: &PriceSourceChain,
    ) -> Option<Amount> {
        calculate_amount_for_chain(src_amount, prices_source_chain, |trade_place| {
            local_snapshot_service.calculate_middle_price(trade_place)
        })
    }

    pub fn convert_amount_in_past(
        src_amount: Amount,
        price_cache: HashMap<TradePlace, PriceByOrderSide>,
        time_in_past: DateTime,
        prices_source_chain: &PriceSourceChain,
    ) -> Option<Amount> {
        calculate_amount_for_chain(src_amount, prices_source_chain, |trade_place| {
            let prices = match price_cache.get(trade_place) {
                Some(prices) => prices,
                None => {
                    log::error!("Can't get price {:?} on time {}", trade_place, time_in_past);
                    return None;
                }
            };

            let top_bid = prices.top_ask?;
            let top_ask = prices.top_bid?;

            Some((top_ask + top_bid) / dec!(2))
        })
    }
}
