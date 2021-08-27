#[cfg(test)]
use std::{collections::HashMap, sync::Arc};

use crate::core::{
    balance_manager::{balance_manager::BalanceManager, balance_request::BalanceRequest},
    exchanges::events::ExchangeBalance,
    exchanges::{
        common::{Amount, CurrencyCode, CurrencyPair, ExchangeAccountId},
        events::ExchangeBalancesAndPositions,
        general::currency_pair_metadata::CurrencyPairMetadata,
    },
    misc::{
        derivative_position_info::DerivativePositionInfo, reserve_parameters::ReserveParameters,
    },
    orders::order::{
        ClientOrderId, OrderExecutionType, OrderHeader, OrderSide, OrderSimpleProps, OrderSnapshot,
        OrderType, ReservationId,
    },
    service_configuration::configuration_descriptor::ConfigurationDescriptor,
};

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub struct BalanceManagerBase {
    pub ten_digit_precision: Decimal,
    pub order_index: i32,
    //         protected const string ExchangeName = "Binance";
    pub exchange_account_id: ExchangeAccountId,
    //         protected const string ExchangeName2 = "Binance";
    //         protected const string ExchangeId2 = "Binance1";
    pub currency_pair: CurrencyPair,
    pub configuration_descriptor: ConfigurationDescriptor,
    //         protected Mock<IDateTimeService> DateTimeService;
    //         protected Mock<IDataRecorder> DataRecorder = new Mock<IDataRecorder>();
    currency_pair_metadata: Option<Arc<CurrencyPairMetadata>>,
    balance_manager: Option<BalanceManager>,
}
// static
impl BalanceManagerBase {
    pub fn exchange_name() -> String {
        "local_exchange_account_id".into()
    }
    pub fn exchange_id() -> String {
        BalanceManagerBase::exchange_name() + "0".into()
    }
    // Quote currency
    pub fn btc() -> String {
        "BTC".into()
    }
    // Base currency
    pub fn eth() -> String {
        "ETH".into()
    }
    // Another currency
    pub fn bnb() -> String {
        "BNB".into()
    }

    pub fn currency_pair() -> CurrencyPair {
        CurrencyPair::from_codes(
            CurrencyCode::new(BalanceManagerBase::eth().into()),
            CurrencyCode::new(BalanceManagerBase::btc().into()),
        )
    }

    pub fn update_balance(
        balance_manager: &mut BalanceManager,
        exchange_account_id: &ExchangeAccountId,
        balances_by_currency_code: HashMap<CurrencyCode, Decimal>,
    ) {
        balance_manager
            .update_exchange_balance(
                exchange_account_id,
                ExchangeBalancesAndPositions {
                    balances: balances_by_currency_code
                        .iter()
                        .map(|x| ExchangeBalance {
                            currency_code: x.0.clone(),
                            balance: x.1.clone(),
                        })
                        .collect(),
                    positions: None,
                },
            )
            .expect("failed to update exchange balance");
    }

    pub fn update_balance_with_positions(
        balance_manager: &mut BalanceManager,
        exchange_account_id: &ExchangeAccountId,
        balances_by_currency_code: HashMap<CurrencyCode, Decimal>,
        positions_by_currency_pair: HashMap<CurrencyPair, Decimal>,
    ) {
        let balances: Vec<ExchangeBalance> = balances_by_currency_code
            .iter()
            .map(|x| ExchangeBalance {
                currency_code: x.0.clone(),
                balance: x.1.clone(),
            })
            .collect();

        let positions: Vec<DerivativePositionInfo> = positions_by_currency_pair
            .iter()
            .map(|x| {
                DerivativePositionInfo::new(
                    x.0.clone(),
                    x.1.clone(),
                    None,
                    dec!(0),
                    dec!(0),
                    dec!(1),
                )
            })
            .collect();

        balance_manager
            .update_exchange_balance(
                exchange_account_id,
                ExchangeBalancesAndPositions {
                    balances,
                    positions: Some(positions),
                },
            )
            .expect("failed to update exchange balance");
    }

    pub fn new() -> Self {
        let exchange_account_id =
            ExchangeAccountId::new(BalanceManagerBase::exchange_name().as_str().into(), 0);
        Self {
            ten_digit_precision: dec!(0.0000000001),
            order_index: 1,
            exchange_account_id: exchange_account_id.clone(),
            currency_pair: BalanceManagerBase::currency_pair().clone(),
            configuration_descriptor: ConfigurationDescriptor::new(
                "LiquidityGenerator".into(),
                exchange_account_id.to_string()
                    + ";"
                    + BalanceManagerBase::currency_pair().as_str(),
            ),
            currency_pair_metadata: None,
            balance_manager: None,
        }
    }
}

impl BalanceManagerBase {
    pub fn currency_pair_metadata(&self) -> Arc<CurrencyPairMetadata> {
        match &self.currency_pair_metadata {
            Some(res) => res.clone(),
            None => std::panic!("should be non None here"),
        }
    }

    pub fn balance_manager(&self) -> &BalanceManager {
        match self.balance_manager.as_ref() {
            Some(res) => res,
            None => std::panic!("should be non None here"),
        }
    }

    pub fn set_balance_manager(&mut self, input: BalanceManager) {
        self.balance_manager = Some(input);
    }

    pub fn set_currency_pair_metadata(&mut self, input: Arc<CurrencyPairMetadata>) {
        self.currency_pair_metadata = Some(input);
    }

    pub fn balance_manager_mut(&mut self) -> &mut BalanceManager {
        match self.balance_manager.as_mut() {
            Some(res) => res,
            None => std::panic!("should be non None here"),
        }
    }

    pub fn create_balance_request(&self, currency_code: CurrencyCode) -> BalanceRequest {
        BalanceRequest::new(
            self.configuration_descriptor.clone(),
            self.exchange_account_id.clone(),
            self.currency_pair.clone(),
            currency_code,
        )
    }

    pub fn create_reserve_parameters(
        &self,
        order_side: Option<OrderSide>,
        price: Decimal,
        amount: Amount,
    ) -> ReserveParameters {
        ReserveParameters::new(
            self.configuration_descriptor.clone(),
            self.exchange_account_id.clone(),
            self.currency_pair_metadata(),
            order_side,
            price,
            amount,
        )
    }

    pub fn get_balance_by_trade_side(
        &self,
        trade_side: OrderSide,
        price: Decimal,
    ) -> Option<Decimal> {
        self.balance_manager().get_balance_by_side(
            &self.configuration_descriptor,
            &self.exchange_account_id,
            self.currency_pair_metadata().clone(),
            trade_side,
            price,
        )
    }

    pub fn get_balance_by_currency_code(
        &self,
        currency_code: CurrencyCode,
        price: Decimal,
    ) -> Option<Decimal> {
        self.balance_manager().get_balance_by_currency_code(
            &self.configuration_descriptor,
            &self.exchange_account_id,
            self.currency_pair_metadata().clone(),
            &currency_code,
            price,
        )
    }

    pub fn create_order(
        &self,
        order_side: OrderSide,
        reservation_id: ReservationId,
    ) -> OrderSnapshot {
        OrderSnapshot {
            header: OrderHeader::new(
                ClientOrderId::new(format!("order{}", self.order_index).into()),
                Utc::now(),
                self.exchange_account_id.clone(),
                self.currency_pair_metadata().currency_pair().clone(),
                OrderType::Limit,
                order_side,
                dec!(5),
                OrderExecutionType::None,
                Some(reservation_id),
                None,
                "balance_manager_base".into(),
            ),
            props: OrderSimpleProps::from_price(Some(dec!(0.2))),
            fills: Default::default(),
            status_history: Default::default(),
            internal_props: Default::default(),
        }
    }
}
