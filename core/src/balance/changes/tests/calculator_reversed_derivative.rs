#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use mmb_domain::market::CurrencyCode;
    use mmb_domain::order::snapshot::OrderSide;
    use mmb_domain::order::snapshot::Price;
    use mmb_utils::hashmap;
    use mockall_double::double;
    use parking_lot::ReentrantMutexGuard;
    use rstest::rstest;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    use crate::balance::changes::tests::calculator_tests_base::tests::BalanceChangesCalculatorTestsBase;
    #[double]
    use crate::services::usd_convertion::usd_converter::UsdConverter;

    type TestBase = BalanceChangesCalculatorTestsBase;

    fn init_usd_converter(
        prices: HashMap<CurrencyCode, Price>,
    ) -> (UsdConverter, ReentrantMutexGuard<'static, ()>) {
        let (mut usd_converter, usd_converter_locker) = UsdConverter::init_mock();
        usd_converter
            .expect_convert_amount()
            .returning(move |from, amount, _| {
                if from == TestBase::quote() {
                    return Some(amount);
                }

                let price = *prices.get(&from).expect("in test");
                Some(amount * price)
            })
            .times(2);
        (usd_converter, usd_converter_locker)
    }

    #[rstest]
    #[case(OrderSide::Buy, dec!(8_000), dec!(4_000), dec!(-50))] // buy, price dropped
    #[case(OrderSide::Buy, dec!(4_000), dec!(8_000), dec!(100))] // buy, price rose
    #[case(OrderSide::Buy, dec!(8_000), dec!(8_000), dec!(0))] // buy, same price
    #[case(OrderSide::Sell, dec!(8_000), dec!(4_000), dec!(50))] // sell, price dropped
    #[case(OrderSide::Sell, dec!(4_000), dec!(8_000), dec!(-100))] // sell, price rose
    #[case(OrderSide::Sell, dec!(8_000), dec!(8_000), dec!(0))] // sell, same price
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn simple_profit_by_side_and_price_no_commission(
        #[case] side: OrderSide,
        #[case] trade_price: Decimal,
        #[case] new_price: Decimal,
        #[case] profit: Decimal,
    ) {
        let (usd_converter, usd_converter_locker) = init_usd_converter(hashmap![
            TestBase::base() => new_price
        ]);

        let mut test_obj =
            TestBase::new_with_usd_converter(true, true, usd_converter, usd_converter_locker);

        let amount = dec!(100) / trade_price / TestBase::amount_multiplier(); //equivalent of $100

        let order = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            side,
            trade_price,
            amount,
            amount,
            TestBase::quote(),
            dec!(0),
        );

        test_obj.calculate_balance_changes(vec![&order]).await;

        let raw_profit = test_obj.calculate_raw_profit();
        assert!(raw_profit.is_zero());

        let usd_over_market = test_obj.calculate_over_market_profit().await;
        assert_eq!(usd_over_market, profit);
    }

    #[rstest]
    #[case(OrderSide::Buy, dec!(8_000), dec!(8_000), dec!(-10))] // no price change, minus commission
    #[case(OrderSide::Sell, dec!(8_000), dec!(8_000), dec!(-10))] // no price change, minus commission
    #[case(OrderSide::Buy, dec!(8_000), dec!(8_800), dec!(0))] // positive minus commission
    #[case(OrderSide::Sell, dec!(8_000), dec!(7_200), dec!(0))] // positive minus commission
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn simple_profit_by_side_and_price_with_commission(
        #[case] side: OrderSide,
        #[case] trade_price: Decimal,
        #[case] new_price: Decimal,
        #[case] profit: Decimal,
    ) {
        let (usd_converter, usd_converter_locker) = init_usd_converter(hashmap![
            TestBase::base() => new_price
        ]);

        let mut test_obj =
            TestBase::new_with_usd_converter(true, true, usd_converter, usd_converter_locker);

        let commission_in_quote = dec!(10);
        let amount = dec!(100) / trade_price / TestBase::amount_multiplier(); //equivalent of $100

        let order = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            side,
            trade_price,
            amount,
            amount,
            TestBase::quote(),
            commission_in_quote,
        );

        test_obj.calculate_balance_changes(vec![&order]).await;

        let raw_profit = test_obj.calculate_raw_profit();
        assert_eq!(raw_profit, -commission_in_quote);

        let usd_over_market = test_obj.calculate_over_market_profit().await;
        assert_eq!(usd_over_market, profit);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn simple_profit_two_orders_with_commission() {
        let first_price = dec!(10_000);
        let second_price = dec!(2_000);

        let (usd_converter, usd_converter_locker) = init_usd_converter(hashmap![
            TestBase::base() => second_price
        ]);

        let mut test_obj =
            TestBase::new_with_usd_converter(true, true, usd_converter, usd_converter_locker);

        let amount = dec!(10_000);
        let commission_rate_make = dec!(-0.025);

        let first_side = OrderSide::Buy;
        let first_btc = amount / first_price / TestBase::amount_multiplier(); // buy btc

        let second_side = OrderSide::Sell;
        let second_btc = amount / second_price / TestBase::amount_multiplier(); // sell btc

        let first_commission_amount = amount * commission_rate_make * dec!(0.01);
        let first_order = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            first_side,
            first_price,
            first_btc,
            first_btc,
            TestBase::quote(),
            first_commission_amount,
        );

        let second_commission_amount = amount * commission_rate_make * dec!(0.01);
        let second_order = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            second_side,
            second_price,
            second_btc,
            second_btc,
            TestBase::quote(),
            second_commission_amount,
        );

        test_obj
            .calculate_balance_changes(vec![&first_order, &second_order])
            .await;

        let raw_profit = test_obj.calculate_raw_profit();
        let usd_over_market_profit = test_obj.calculate_over_market_profit().await;

        let raw = (first_btc * first_price + second_btc * second_price)
            * -commission_rate_make
            * dec!(0.01)
            * TestBase::amount_multiplier();
        let usd_value = (first_btc - second_btc) * TestBase::amount_multiplier() * second_price
            - first_commission_amount
            - second_commission_amount;

        assert_eq!(raw_profit, raw);
        assert_eq!(usd_over_market_profit, usd_value);
    }
}
