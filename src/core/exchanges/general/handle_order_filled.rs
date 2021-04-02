use std::sync::Arc;

use super::{currency_pair_metadata::CurrencyPairMetadata, exchange::Exchange};
use crate::core::{
    exchanges::common::Amount, exchanges::common::CurrencyCode, exchanges::common::CurrencyPair,
    exchanges::common::ExchangeAccountId, exchanges::common::ExchangeIdCurrencyPair,
    exchanges::common::Price, exchanges::events::AllowedEventSourceType,
    orders::fill::EventSourceType, orders::fill::OrderFillType, orders::order::ClientOrderId,
    orders::order::ExchangeOrderId, orders::order::OrderRole, orders::order::OrderSide,
    orders::order::OrderSnapshot, orders::order::OrderStatus, orders::order::OrderType,
    orders::pool::OrderRef,
};
use anyhow::{bail, Result};
use log::{error, info, warn};
use parking_lot::RwLock;
use rust_decimal::prelude::Zero;
use rust_decimal_macros::dec;

type ArgsToLog = (
    ExchangeAccountId,
    String,
    Option<ClientOrderId>,
    ExchangeOrderId,
    AllowedEventSourceType,
    EventSourceType,
);

#[derive(Debug, Clone)]
pub struct FillEventData {
    pub source_type: EventSourceType,
    pub trade_id: String,
    pub client_order_id: Option<ClientOrderId>,
    pub exchange_order_id: ExchangeOrderId,
    pub fill_price: Price,
    pub fill_amount: Amount,
    pub is_diff: bool,
    pub total_filled_amount: Option<Amount>,
    pub order_role: Option<OrderRole>,
    pub commission_currency_code: Option<CurrencyCode>,
    pub commission_rate: Option<Amount>,
    pub commission_amount: Option<Amount>,
    pub fill_type: OrderFillType,
    pub trade_currency_pair: Option<CurrencyPair>,
    pub order_side: Option<OrderSide>,
    pub order_amount: Option<Amount>,
}

impl Exchange {
    pub fn handle_order_filled(&self, mut event_data: FillEventData) -> Result<()> {
        let args_to_log = (
            self.exchange_account_id.clone(),
            event_data.trade_id.clone(),
            event_data.client_order_id.clone(),
            event_data.exchange_order_id.clone(),
            self.features.allowed_fill_event_source_type,
            event_data.source_type,
        );

        if Self::should_ignore_event(
            self.features.allowed_fill_event_source_type,
            event_data.source_type,
        ) {
            info!("Ignoring fill {:?}", args_to_log);
            return Ok(());
        }

        if event_data.exchange_order_id.as_str().is_empty() {
            Self::log_fill_handling_error_and_propagate(
                "Received HandleOrderFilled with an empty exchangeOrderId",
                &args_to_log,
            )?;
        }

        self.check_based_on_fill_type(&mut event_data, &args_to_log)?;

        match self
            .orders
            .by_exchange_id
            .get(&event_data.exchange_order_id)
        {
            None => {
                info!("Received a fill for not existing order {:?}", &args_to_log);
                // TODO BufferedFillsManager.add_fill()

                if let Some(client_order_id) = event_data.client_order_id {
                    self.raise_order_created(
                        client_order_id,
                        event_data.exchange_order_id,
                        event_data.source_type,
                    );
                }

                return Ok(());
            }
            Some(order) => {
                self.local_order_exist(&mut event_data, &*order)?;
            }
        }

        //FIXME handle it in the end
        Ok(())
    }

    fn local_order_exist(&self, event_data: &mut FillEventData, order: &OrderRef) -> Result<()> {
        let (order_fills, order_filled_amount) = order.get_fills();

        if !event_data.trade_id.is_empty()
            && order_fills.iter().any(|fill| {
                if let Some(fill_trade_id) = fill.trade_id() {
                    return fill_trade_id == &event_data.trade_id;
                }

                false
            })
        {
            info!(
                "Trade with {} was received already for order {:?}",
                event_data.trade_id, order
            );

            return Ok(());
        }

        if event_data.is_diff && order_fills.iter().any(|fill| !fill.is_diff()) {
            // Most likely we received a trade update (diff), then received a non-diff fill via fallback and then again received a diff trade update
            // It happens when WebSocket is glitchy and we miss update and the problem is we have no idea how to handle diff updates
            // after applying a non-diff one as there's no TradeId, so we have to ignore all the diff updates afterwards
            // relying only on fallbacks
            warn!(
                "Unable to process a diff fill after a non-diff one {:?}",
                order
            );

            return Ok(());
        }

        if !event_data.is_diff && order_filled_amount >= event_data.fill_amount {
            warn!(
                "order.filled_amount is {} >= received fill {}, so non-diff fill for {} {:?} should be ignored",
                order_filled_amount,
                event_data.fill_amount,
                order.client_order_id(),
                order.exchange_order_id(),
            );

            return Ok(());
        }

        let mut last_fill_amount = event_data.fill_amount;
        let mut last_fill_price = event_data.fill_price;
        let symbol =
            CurrencyPairMetadata::new(self.exchange_account_id.clone(), order.currency_pair());
        let mut last_fill_cost = if !symbol.is_derivative() {
            last_fill_amount * last_fill_price
        } else {
            last_fill_amount / last_fill_price
        };

        if !event_data.is_diff && order_fills.len() > 0 {
            // Diff should be calculated only if it is not the first fill
            let mut total_filled_cost = dec!(0);
            order_fills
                .iter()
                .for_each(|fill| total_filled_cost += fill.cost());
            let cost_diff = last_fill_cost - total_filled_cost;
            if cost_diff <= dec!(0) {
                warn!("cost_diff if {} which is <= for {:?}", cost_diff, order);
                return Ok(());
            }

            let amount_diff = last_fill_amount - order_filled_amount;
            let res_fill_price = if !symbol.is_derivative() {
                cost_diff / amount_diff
            } else {
                amount_diff / cost_diff
            };
            // TODO second parameter rounda.to_newarest_neighbor
            last_fill_price = symbol.price_round(res_fill_price);

            last_fill_amount = amount_diff;
            last_fill_cost = cost_diff;

            if let Some(commission_amount) = event_data.commission_amount {
                let mut current_commission = dec!(0);
                order_fills
                    .iter()
                    .for_each(|fill| current_commission += fill.commission_amount());
                event_data.commission_amount = Some(commission_amount - current_commission);
            }
        }

        if last_fill_amount.is_zero() {
            warn!(
                "last_fill_amount was received for 0 for {}, {:?}",
                order.client_order_id(),
                order.exchange_order_id()
            );

            return Ok(());
        }

        if let Some(total_filled_amount) = event_data.total_filled_amount {
            if order_filled_amount + last_fill_amount != total_filled_amount {
                warn!(
                    "Fill was missed because {} != {} for {:?}",
                    order_filled_amount, total_filled_amount, order
                );

                return Ok(());
            }
        }

        if order.status() == OrderStatus::FailedToCreate
            || order.status() == OrderStatus::Completed
            || order.was_cancellation_event_raised()
        {
            let error_msg = format!(
                "Fill was received for a {:?} {} {:?}",
                order.status(),
                order.was_cancellation_event_raised(),
                event_data
            );

            error!("{}", error_msg);
            bail!("{}", error_msg)
        }

        info!("Received fill {:?}", event_data);

        if event_data.commission_currency_code.is_none() {
            event_data.commission_currency_code =
                Some(symbol.get_commision_currency_code(order.side()));
        }

        if event_data.order_role.is_none() {
            if event_data.commission_amount.is_none()
                && event_data.commission_rate.is_none()
                && order.role().is_none()
            {
                let error_msg = format!(
                    "Fill has neither commission nor comission rate. Order role in order was set too",
                );

                error!("{}", error_msg);
                bail!("{}", error_msg)
            }

            event_data.order_role = order.role();
        }

        // FIXME What is the better name?
        let some_magical_number = dec!(0.01);
        let expected_commission_rate =
            self.commission.get_commission(event_data.order_role)?.fee * some_magical_number;

        if event_data.commission_amount.is_none() && event_data.commission_rate.is_none() {
            event_data.commission_rate = Some(expected_commission_rate);
        }

        if event_data.commission_amount.is_none() {
            let last_fill_amount_in_currency_code = symbol
                .convert_amount_from_amount_currency_code(
                    // FIXME refactor this
                    event_data.commission_currency_code.clone().expect(
                        "Impossible sitation: event_data.commission_currency_code are set above already"
                    ),
                    last_fill_amount,
                    last_fill_price,
                );
            event_data.commission_amount = Some(
                last_fill_amount_in_currency_code
                    * event_data.commission_rate.expect(
                        // FIXME that is not true! commission rate can be null here
                        "Impossible sitation: event_data.commission_rate are set above already",
                    ),
            );
        }

        // FIXME refactoring this handling Option<comission_currency_code>
        let commission_currency_code = event_data.commission_currency_code.clone().expect(
            "Impossible sitation: event_data.commission_currency_code are set above already",
        );
        // FIXME refactoring this handling Option<comission_amount>>
        let commission_amount = event_data
            .commission_amount
            .clone()
            .expect("Impossible sitation: event_data.commission_amount are set above already");

        let mut converted_commission_currency_code = commission_currency_code.clone();
        let mut converted_commission_amount = commission_amount;

        if commission_currency_code != symbol.base_currency_code
            && commission_currency_code != symbol.quote_currency_code
        {
            let mut currency_pair = CurrencyPair::from_currency_codes(
                commission_currency_code.clone(),
                symbol.quote_currency_code.clone(),
            );
            match self.top_prices.get(&currency_pair) {
                Some(top_prices) => {
                    let (_, bid) = *top_prices;
                    let price_bnb_quote = bid.0;
                    converted_commission_amount = commission_amount * price_bnb_quote;
                    converted_commission_currency_code = symbol.quote_currency_code.clone();
                }
                None => {
                    currency_pair = CurrencyPair::from_currency_codes(
                        symbol.quote_currency_code.clone(),
                        commission_currency_code,
                    );

                    match self.top_prices.get(&currency_pair) {
                        Some(top_prices) => {
                            let (ask, _) = *top_prices;
                            let price_quote_bnb = ask.0;
                            converted_commission_amount = commission_amount / price_quote_bnb;
                            converted_commission_currency_code = symbol.quote_currency_code.clone();
                        }
                        None => error!(
                            "Top bids and asks for {} and currency pair {:?} do not exist",
                            self.exchange_account_id, currency_pair
                        ),
                    }
                }
            }
        }

        // TODO continue here
        //let last_fill_amount_in_converted_commission_currency_code =

        // FIXME handle it in the end
        Ok(())
    }

    fn check_based_on_fill_type(
        &self,
        event_data: &mut FillEventData,
        args_to_log: &ArgsToLog,
    ) -> Result<()> {
        if event_data.fill_type == OrderFillType::Liquidation
            || event_data.fill_type == OrderFillType::ClosePosition
        {
            if event_data.fill_type == OrderFillType::Liquidation
                && event_data.trade_currency_pair.is_none()
            {
                Self::log_fill_handling_error_and_propagate(
                    "Currency pair should be set for liquidation trade",
                    &args_to_log,
                )?;
            }

            if event_data.order_side.is_none() {
                Self::log_fill_handling_error_and_propagate(
                    "Side should be set for liquidatioin or close position trade",
                    &args_to_log,
                )?;
            }

            if event_data.client_order_id.is_some() {
                Self::log_fill_handling_error_and_propagate(
                    "Client order id cannot be set for liquidation or close position trade",
                    &args_to_log,
                )?;
            }

            if event_data.order_amount.is_none() {
                Self::log_fill_handling_error_and_propagate(
                    "Order amount should be set for liquidation or close position trade",
                    &args_to_log,
                )?;
            }

            match self
                .orders
                .by_exchange_id
                .get(&event_data.exchange_order_id)
            {
                Some(order) => {
                    event_data.client_order_id = Some(order.client_order_id());
                }
                None => {
                    let order_instance = self.create_order_instance(event_data);

                    event_data.client_order_id =
                        Some(order_instance.header.client_order_id.clone());
                    self.handle_create_order_succeeded(
                        &self.exchange_account_id,
                        &order_instance.header.client_order_id,
                        &event_data.exchange_order_id,
                        &event_data.source_type,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn create_order_instance(&self, event_data: &FillEventData) -> OrderSnapshot {
        let currency_pair = event_data
            .trade_currency_pair
            .clone()
            .expect("Impossible situation: currency pair are checked above already");
        let order_amount = event_data
            .order_amount
            .clone()
            .expect("Impossible situation: amount are checked above already");
        let order_side = event_data
            .order_side
            .clone()
            .expect("Impossible situation: order_side are checked above already");

        let client_order_id = ClientOrderId::unique_id();

        let order_instance = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            self.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        self.orders
            .add_snapshot_initial(Arc::new(RwLock::new(order_instance.clone())));

        order_instance
    }

    fn log_fill_handling_error_and_propagate(
        template: &str,
        args_to_log: &(
            ExchangeAccountId,
            String,
            Option<ClientOrderId>,
            ExchangeOrderId,
            AllowedEventSourceType,
            EventSourceType,
        ),
    ) -> Result<()> {
        let error_msg = format!("{} {:?}", template, args_to_log);

        error!("{}", error_msg);
        bail!("{}", error_msg)
    }

    fn should_ignore_event(
        allowed_event_source_type: AllowedEventSourceType,
        source_type: EventSourceType,
    ) -> bool {
        if allowed_event_source_type == AllowedEventSourceType::FallbackOnly
            && source_type != EventSourceType::RestFallback
        {
            return true;
        }

        if allowed_event_source_type == AllowedEventSourceType::NonFallback
            && source_type != EventSourceType::Rest
            && source_type != EventSourceType::WebSocket
        {
            return true;
        }

        return false;
    }
}

#[cfg(test)]
mod test {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::core::{
        exchanges::binance::binance::Binance, exchanges::common::CurrencyCode,
        exchanges::events::OrderEvent, exchanges::general::commission::Commission,
        exchanges::general::features::ExchangeFeatures,
        exchanges::general::features::OpenOrdersType, orders::fill::OrderFill,
        orders::order::OrderFillRole, orders::pool::OrdersPool, settings,
    };
    use std::sync::mpsc::{channel, Receiver};

    fn get_test_exchange() -> (Arc<Exchange>, Receiver<OrderEvent>) {
        let settings =
            settings::ExchangeSettings::new("test_api_key".into(), "test_secret_key".into(), false);

        let binance = Binance::new(settings, "Binance0".parse().expect("in test"));

        let (tx, rx) = channel();
        let exchange = Exchange::new(
            ExchangeAccountId::new("local_exchange_account_id".into(), 0),
            "host".into(),
            vec![],
            vec![],
            Box::new(binance),
            ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                false,
                true,
                AllowedEventSourceType::default(),
            ),
            tx,
            Commission::default(),
        );

        (exchange, rx)
    }

    mod liquidation {
        use super::*;

        #[test]
        fn empty_currency_pair() {
            let event_data = FillEventData {
                source_type: EventSourceType::WebSocket,
                trade_id: String::new(),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price: dec!(0),
                fill_amount: dec!(0),
                is_diff: false,
                total_filled_amount: None,
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                trade_currency_pair: None,
                order_side: None,
                order_amount: None,
            };

            let (exchange, _) = get_test_exchange();
            match exchange.handle_order_filled(event_data) {
                Ok(_) => assert!(false),
                Err(error) => {
                    assert_eq!(
                        "Currency pair should be set for liquidation trade",
                        &error.to_string()[..49]
                    );
                }
            }
        }

        #[test]
        fn empty_order_side() {
            let event_data = FillEventData {
                source_type: EventSourceType::WebSocket,
                trade_id: String::new(),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price: dec!(0),
                fill_amount: dec!(0),
                is_diff: false,
                total_filled_amount: None,
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                trade_currency_pair: Some(CurrencyPair::from_currency_codes(
                    "te".into(),
                    "st".into(),
                )),
                order_side: None,
                order_amount: None,
            };

            let (exchange, _) = get_test_exchange();
            match exchange.handle_order_filled(event_data) {
                Ok(_) => assert!(false),
                Err(error) => {
                    assert_eq!(
                        "Side should be set for liquidatioin or close position trade",
                        &error.to_string()[..59]
                    );
                }
            }
        }

        #[test]
        fn not_empty_client_order_id() {
            let event_data = FillEventData {
                source_type: EventSourceType::WebSocket,
                trade_id: String::new(),
                client_order_id: Some(ClientOrderId::unique_id()),
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price: dec!(0),
                fill_amount: dec!(0),
                is_diff: false,
                total_filled_amount: None,
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                trade_currency_pair: Some(CurrencyPair::from_currency_codes(
                    "te".into(),
                    "st".into(),
                )),
                order_side: Some(OrderSide::Buy),
                order_amount: None,
            };

            let (exchange, _) = get_test_exchange();
            match exchange.handle_order_filled(event_data) {
                Ok(_) => assert!(false),
                Err(error) => {
                    assert_eq!(
                        "Client order id cannot be set for liquidation or close position trade",
                        &error.to_string()[..69]
                    );
                }
            }
        }

        #[test]
        fn not_empty_order_amount() {
            let event_data = FillEventData {
                source_type: EventSourceType::WebSocket,
                trade_id: String::new(),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price: dec!(0),
                fill_amount: dec!(0),
                is_diff: false,
                total_filled_amount: None,
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                trade_currency_pair: Some(CurrencyPair::from_currency_codes(
                    "te".into(),
                    "st".into(),
                )),
                order_side: Some(OrderSide::Buy),
                order_amount: None,
            };

            let (exchange, _) = get_test_exchange();
            match exchange.handle_order_filled(event_data) {
                Ok(_) => assert!(false),
                Err(error) => {
                    assert_eq!(
                        "Order amount should be set for liquidation or close position trade",
                        &error.to_string()[..66]
                    );
                }
            }
        }

        #[test]
        fn should_add_order() {
            let currency_pair = CurrencyPair::from_currency_codes("te".into(), "st".into());
            let order_side = OrderSide::Buy;
            let order_amount = dec!(1);
            let order_role = None;
            let fill_price = dec!(1);
            let fill_amount = dec!(0);

            let event_data = FillEventData {
                source_type: EventSourceType::WebSocket,
                trade_id: String::new(),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price,
                fill_amount,
                is_diff: false,
                total_filled_amount: None,
                order_role,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                trade_currency_pair: Some(currency_pair.clone()),
                order_side: Some(order_side),
                order_amount: Some(order_amount),
            };

            let (exchange, _event_received) = get_test_exchange();
            match exchange.handle_order_filled(event_data) {
                Ok(_) => {
                    let order = exchange
                        .orders
                        .by_client_id
                        .iter()
                        .next()
                        .expect("order should be added already");
                    assert_eq!(order.order_type(), OrderType::Liquidation);
                    assert_eq!(order.exchange_account_id(), exchange.exchange_account_id);
                    assert_eq!(order.currency_pair(), currency_pair);
                    assert_eq!(order.side(), order_side);
                    assert_eq!(order.amount(), order_amount);
                    assert_eq!(order.price(), fill_price);
                    assert_eq!(order.role(), order_role);

                    // TODO FIX it when Symbol wil be implemented
                    //let (fills, filled_amount) = order.get_fills();
                    //assert_eq!(filled_amount, fill_amount);
                    //assert_eq!(fills.iter().next().expect("in test").price(), fill_price);
                }
                Err(error) => {
                    dbg!(&error.to_string());
                    assert!(false);
                }
            }
        }

        #[test]
        fn empty_exchange_order_id() {
            let event_data = FillEventData {
                source_type: EventSourceType::WebSocket,
                trade_id: String::new(),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("".into()),
                fill_price: dec!(0),
                fill_amount: dec!(0),
                is_diff: false,
                total_filled_amount: None,
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                trade_currency_pair: Some(CurrencyPair::from_currency_codes(
                    "te".into(),
                    "st".into(),
                )),
                order_side: Some(OrderSide::Buy),
                order_amount: Some(dec!(0)),
            };

            let (exchange, _event_receiver) = get_test_exchange();
            match exchange.handle_order_filled(event_data) {
                Ok(_) => assert!(false),
                Err(error) => {
                    assert_eq!(
                        "Received HandleOrderFilled with an empty exchangeOrderId",
                        &error.to_string()[..56]
                    );
                }
            }
        }
    }

    #[test]
    fn ignore_fill_with_same_trade_id() {
        let (exchange, _event_receiver) = get_test_exchange();

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let order_price = dec!(1);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();
        let fill_amount = dec!(0.2);

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(CurrencyPair::from_currency_codes("te".into(), "st".into())),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        let cost = dec!(0);
        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            Utc::now(),
            OrderFillType::Liquidation,
            Some(trade_id),
            order_price,
            fill_amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            false,
            None,
            None,
        );
        order.add_fill(order_fill);
        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        exchange
            .local_order_exist(&mut event_data, &*order_ref)
            .expect("in test");

        let (_, order_filled_amount) = order_ref.get_fills();
        assert_eq!(order_filled_amount, fill_amount);
    }

    #[test]
    fn ignore_diff_fill_after_non_diff() {
        let (exchange, _event_receiver) = get_test_exchange();

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let order_price = dec!(1);
        let fill_amount = dec!(0.2);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(CurrencyPair::from_currency_codes("te".into(), "st".into())),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        let cost = dec!(0);
        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            Utc::now(),
            OrderFillType::Liquidation,
            Some("different_trade_id".to_owned()),
            order_price,
            fill_amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            false,
            None,
            None,
        );
        order.add_fill(order_fill);
        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        exchange
            .local_order_exist(&mut event_data, &*order_ref)
            .expect("in test");

        let (_, order_filled_amount) = order_ref.get_fills();
        assert_eq!(order_filled_amount, fill_amount);
    }

    #[test]
    fn ignore_non_diff_fill_if_current_filled_amount_is_more_or_equal() {
        let (exchange, _event_receiver) = get_test_exchange();

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let order_price = dec!(1);
        let fill_amount = dec!(0.2);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(CurrencyPair::from_currency_codes("te".into(), "st".into())),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        let cost = dec!(0);
        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            Utc::now(),
            OrderFillType::Liquidation,
            Some("different_trade_id".to_owned()),
            order_price,
            fill_amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            false,
            None,
            None,
        );
        order.add_fill(order_fill);
        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        exchange
            .local_order_exist(&mut event_data, &*order_ref)
            .expect("in test");

        let (_, order_filled_amount) = order_ref.get_fills();
        assert_eq!(order_filled_amount, fill_amount);
    }

    #[test]
    fn ignore_diff_fill_if_filled_amount_is_zero() {
        let (exchange, _event_receiver) = get_test_exchange();

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let order_price = dec!(1);
        let fill_amount = dec!(0);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        let cost = dec!(0);
        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            Utc::now(),
            OrderFillType::Liquidation,
            Some("different_trade_id".to_owned()),
            order_price,
            fill_amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            true,
            None,
            None,
        );
        order.add_fill(order_fill);
        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        exchange
            .local_order_exist(&mut event_data, &*order_ref)
            .expect("in test");

        let (_, order_filled_amount) = order_ref.get_fills();
        assert_eq!(order_filled_amount, dec!(0));
    }

    #[test]
    fn error_if_order_status_is_failed_to_create() {
        let (exchange, _event_receiver) = get_test_exchange();

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(1);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );
        order.set_status(OrderStatus::FailedToCreate, Utc::now());

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => assert!(false),
            Err(error) => {
                assert_eq!(
                    "Fill was received for a FailedToCreate false",
                    &error.to_string()[..44]
                );
            }
        }
    }

    #[test]
    fn error_if_order_status_is_completed() {
        let (exchange, _event_receiver) = get_test_exchange();

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(1);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );
        order.set_status(OrderStatus::Completed, Utc::now());

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => assert!(false),
            Err(error) => {
                assert_eq!(
                    "Fill was received for a Completed false",
                    &error.to_string()[..39]
                );
            }
        }
    }

    #[test]
    fn error_if_cancellation_event_was_raised() {
        let (exchange, _event_receiver) = get_test_exchange();

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(1);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );
        order.internal_props.cancellation_event_was_raised = true;

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => assert!(false),
            Err(error) => {
                // TODO has to be Created!
                // Does it mean order status had to be changed somewhere?
                assert_eq!(
                    "Fill was received for a Creating true",
                    &error.to_string()[..37]
                );
            }
        }
    }

    #[test]
    fn ignore_fill_if_total_filled_amount_is_incorrect() {
        let (exchange, _event_receiver) = get_test_exchange();

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(5);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: Some(dec!(9)),
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert!(fills.is_empty());
            }
            Err(_) => assert!(false),
        }
    }
}
