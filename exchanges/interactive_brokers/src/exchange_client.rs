use crate::interactive_brokers::InteractiveBrokers;
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use function_name::named;
use mmb_core::exchanges::general::exchange::RequestResult;
use mmb_core::exchanges::general::order::cancel::CancelOrderResult;
use mmb_core::exchanges::general::order::create::CreateOrderResult;
use mmb_core::exchanges::general::order::get_order_trades::OrderTrade;
use mmb_core::exchanges::traits::{ExchangeClient, ExchangeError};
use mmb_domain::events::ExchangeBalancesAndPositions;
use mmb_domain::exchanges::symbol::{Precision, Symbol};
use mmb_domain::market::{CurrencyCode, CurrencyId, CurrencyPair, ExchangeErrorType};
use mmb_domain::order::fill::EventSourceType;
use mmb_domain::order::pool::OrderRef;
use mmb_domain::order::snapshot::{OrderCancelling, OrderInfo, Price};
use mmb_domain::position::{ActivePosition, ClosedPosition};
use mmb_utils::DateTime;
use rust_decimal_macros::dec;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;

#[async_trait]
impl ExchangeClient for InteractiveBrokers {
    async fn create_order(&self, order: &OrderRef) -> CreateOrderResult {
        let res = self
            .create_order_inner(
                &order.currency_pair(),
                order.side(),
                order.price(),
                order.amount(),
            )
            .await;

        match res {
            Ok(exchange_order_id) => {
                CreateOrderResult::succeed(&exchange_order_id, EventSourceType::Rest)
            }
            Err(err_msg) => {
                return CreateOrderResult::failed(
                    ExchangeError::new(
                        ExchangeErrorType::Unknown,
                        format!("Create order error: {err_msg}"),
                        None,
                    ),
                    EventSourceType::Rest,
                );
            }
        }
    }

    async fn cancel_order(&self, order: OrderCancelling) -> CancelOrderResult {
        self.cancel_order_inner(order.exchange_order_id.as_str())
            .await
    }

    async fn cancel_all_orders(&self, currency_pair: CurrencyPair) -> anyhow::Result<()> {
        for order in self.get_open_orders_by_currency_pair(currency_pair).await? {
            let order_id = order.exchange_order_id.as_str();
            let cancel_order_result = self.cancel_order_inner(order_id).await.outcome;

            if let RequestResult::Error(err) = cancel_order_result {
                Err(err)?;
            }
        }

        Ok(())
    }

    async fn get_open_orders(&self) -> anyhow::Result<Vec<OrderInfo>> {
        self.get_open_orders_inner().await
    }

    async fn get_open_orders_by_currency_pair(
        &self,
        currency_pair: CurrencyPair,
    ) -> anyhow::Result<Vec<OrderInfo>> {
        Ok(self
            .get_open_orders_inner()
            .await?
            .into_iter()
            .filter(|v| v.currency_pair == currency_pair)
            .collect())
    }

    #[named]
    async fn get_order_info(&self, order: &OrderRef) -> anyhow::Result<OrderInfo, ExchangeError> {
        let f_n = function_name!();

        let exchange_order_id = order.exchange_order_id().ok_or_else(|| {
            ExchangeError::new(
                ExchangeErrorType::InvalidOrder,
                "Empty field `exchange_order_id`.".to_string(),
                None,
            )
        })?;

        let order = self
            .get_open_orders_inner()
            .await?
            .into_iter()
            .find(|v| v.exchange_order_id == exchange_order_id)
            .ok_or_else(|| {
                ExchangeError::new(
                    ExchangeErrorType::OrderNotFound,
                    format!("fn {f_n}, exchange_order_id: {exchange_order_id}"),
                    None,
                )
            })?;

        Ok(OrderInfo::new(
            order.currency_pair,
            exchange_order_id,
            order.client_order_id,
            order.order_side,
            order.order_status,
            order.price,
            order.amount,
            order.price,
            order.filled_amount,
            None,
            None,
            None,
        ))
    }

    /// TODO: Check if it is right
    async fn close_position(
        &self,
        position: &ActivePosition,
        price: Option<Price>,
    ) -> anyhow::Result<ClosedPosition> {
        let currency_pair = &position.derivative.currency_pair;
        let side = position.derivative.get_side();
        let price = price.context("Expected `price` is `Some`.")?;
        let amount = position.derivative.position.abs();

        // TODO: Check if it is right
        let exchange_order_id = self
            .create_order_inner(currency_pair, side, price, amount)
            .await?;

        // TODO: Check if it is right
        // Check, especially, param `amount` - maybe it must be returned as a result of creating order.
        Ok(ClosedPosition::new(exchange_order_id, amount))
    }

    async fn get_active_positions(&self) -> anyhow::Result<Vec<ActivePosition>> {
        self.get_positions_inner().await
    }

    async fn get_balance(&self) -> anyhow::Result<ExchangeBalancesAndPositions> {
        Ok(ExchangeBalancesAndPositions {
            balances: self.get_balance_inner().await?,
            positions: None,
        })
    }

    /// TODO: Optimize - rewrite with no `Vec` reallocation
    async fn get_balance_and_positions(&self) -> anyhow::Result<ExchangeBalancesAndPositions> {
        // TODO: Optimize - rewrite with no `Vec` reallocation
        let positions = self
            .get_positions_inner()
            .await?
            .into_iter()
            .map(|v| v.derivative)
            .collect();

        Ok(ExchangeBalancesAndPositions {
            balances: self.get_balance_inner().await?,
            positions: Some(positions),
        })
    }

    async fn get_my_trades(
        &self,
        symbol: &Symbol,
        min_datetime: Option<DateTime>,
    ) -> RequestResult<Vec<OrderTrade>> {
        // There we need `Mutex` that locks the entire function,
        // because methods that return `Vec` cannot be called simultaneously
        let _guard = self.mutexes.get_my_trades.lock().await;

        match self.get_my_trades_request().await {
            Ok(_) => match self.get_my_trades_response(symbol, min_datetime).await {
                Ok(v) => RequestResult::Success(v),
                Err(error) => RequestResult::Error(ExchangeError::parsing(error.to_string())),
            },
            Err(error) => RequestResult::Error(Self::cast_error(error)),
        }
    }

    #[named]
    async fn build_all_symbols(&self) -> anyhow::Result<Vec<Arc<Symbol>>> {
        let f_n = function_name!();

        let mut symbols = Vec::with_capacity(10_000);

        let quote_currency = "USD";
        let quote_currency_id = CurrencyId::from(quote_currency);
        let quote_currency_code = CurrencyCode::from(quote_currency);

        let csv_file_path = "./symbols.csv";
        let csv_file = File::open(csv_file_path).context("Open csv file error.")?;
        let _reader = BufReader::new(csv_file);

        // TODO: Remove this block if code after you make sure that CSV-parsing is working well
        // DEBUG begin
        let reader = "Symbol,Date,Open,High,Low,Close,Volume
            A,09-Sep-2022,135.98,137.92,135.43,137.63,2425200
            AA,09-Sep-2022,50.4,53.07,50.27,52.62,7269500
            AAC,09-Sep-2022,9.91,9.92,9.91,9.91,53300"
            .as_bytes();
        // DEBUG end

        let mut reader = csv::Reader::from_reader(reader);
        // We receive header line first - we skip it because we don't need it
        for line in reader.records().skip(1) {
            let line = line.context("CSV record read error.")?;

            let (base_currency, _date, _open, max_price, min_price, _close, _volume) = (
                &line[0], &line[1], &line[2], &line[3], &line[4], &line[5], &line[6],
            );
            let base_currency_id = CurrencyId::from(base_currency);
            let base_currency_code = CurrencyCode::from(base_currency);
            let min_price = Some(
                min_price
                    .parse()
                    .context(anyhow!("fn {f_n}: `min_price` parse error."))?,
            );
            let max_price = Some(
                max_price
                    .parse()
                    .context(anyhow!("fn {f_n}: `max_price` parse error."))?,
            );

            let symbol = Symbol::new(
                false,
                base_currency_id,
                base_currency_code,
                quote_currency_id,
                quote_currency_code,
                min_price,
                max_price,
                None,
                None,
                None,
                base_currency_code,
                Some(quote_currency_code),
                Precision::ByTick { tick: dec!(0.1) },
                Precision::ByTick { tick: dec!(0.001) },
            );
            symbols.push(Arc::new(symbol));
        }

        Ok(symbols)
    }
}
