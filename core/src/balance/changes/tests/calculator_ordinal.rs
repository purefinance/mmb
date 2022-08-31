#[cfg(test)]
mod tests {
    use domain::order::snapshot::OrderSide;
    use rust_decimal_macros::dec;

    use crate::balance::changes::tests::calculator_tests_base::tests::BalanceChangesCalculatorTestsBase;

    type TestBase = BalanceChangesCalculatorTestsBase;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn simple_buy_base_currency() {
        /*
         * /// Just sell some base amount with commission in quote ///
         * Currency pair: Base/Quote
         * Amount currency code: Base
         * Commission currency code: Quote
         */
        let mut test_obj = TestBase::new(false, false);

        let price_base_quote = dec!(0.5);
        let amount_in_base = dec!(5);
        let amount_in_quote = amount_in_base * price_base_quote; // needed amount in quote for buy base
        let filled_amount_in_base = amount_in_base;
        let commission_amount_in_base = filled_amount_in_base * TestBase::commission_rate_1();

        let order = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            OrderSide::Buy,
            price_base_quote,
            amount_in_base,
            filled_amount_in_base,
            TestBase::base(),
            commission_amount_in_base,
        );

        // Expected
        let base_balance_changed = amount_in_base - commission_amount_in_base;
        let quote_balance_changed = -amount_in_quote;

        // Actual
        test_obj.calculate_balance_changes(vec![&order]).await;

        let actual_base_balance_changed = test_obj.get_actual_balance_change(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            TestBase::base(),
        );

        let actual_quote_balance_changed = test_obj.get_actual_balance_change(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            TestBase::quote(),
        );

        assert_eq!(actual_base_balance_changed, base_balance_changed);
        assert_eq!(actual_quote_balance_changed, quote_balance_changed);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn simple_sell_base_currency() {
        /*
         * /// Just sell some base amount with commission in quote ///
         * Currency pair: Base/Quote
         * Amount currency code: Base
         * Commission currency code: Quote
         */
        let mut test_obj = TestBase::new(false, false);

        let price_base_quote = dec!(1.232);
        let amount_in_base = dec!(14);
        let amount_in_quote = amount_in_base * price_base_quote; // needed amount in quote for buy base
        let filled_amount_in_base = amount_in_base;
        let commission_amount_in_quote = amount_in_quote * TestBase::commission_rate_1();

        let order = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            OrderSide::Sell,
            price_base_quote,
            amount_in_base,
            filled_amount_in_base,
            TestBase::quote(),
            commission_amount_in_quote,
        );

        // Expected
        let base_balance_changed = -amount_in_base;
        let quote_balance_changed = amount_in_quote - commission_amount_in_quote;

        // Actual
        test_obj.calculate_balance_changes(vec![&order]).await;

        let actual_base_balance_changed = test_obj.get_actual_balance_change(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            TestBase::base(),
        );

        let actual_quote_balance_changed = test_obj.get_actual_balance_change(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            TestBase::quote(),
        );

        assert_eq!(actual_base_balance_changed, base_balance_changed);
        assert_eq!(actual_quote_balance_changed, quote_balance_changed);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn two_directions_amount_in_base_but_sell_by_equal_price_and_amount_nullable_commission(
    ) {
        /*
         *  ///
         *     This case is just to check if the balance changes to zero,
         *     when buying and selling the same amount of currency at the same price and nullable commission.
         * ///
         * /// Buy base in direction 1, sell in direction 2 ///
         *         // Direction 1 //
         * Exchange:                  EXC1
         * Currency pair:             Base/Quote
         * AmountCurrencyCode:        Base
         * CommissionCurrencyCode:    Base
         * TradeSide:                 Buy
         *        // Direction 2 //
         * Exchange:                  EXC1
         * Currency pair:             Base/Quote
         * AmountCurrencyCode:        Base
         * CommissionCurrencyCode:    Base
         * TradeSide:                 Sell
         */

        let mut test_obj = TestBase::new(false, false);

        // same for both directions
        let price_base_quote = dec!(0.7);
        let amount_in_base = dec!(12);
        let filled_amount_in_base = amount_in_base;
        let commission_amount_in_base = dec!(0);

        let trade_side_1 = OrderSide::Buy;
        let trade_side_2 = OrderSide::Sell;

        let order_1 = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            trade_side_1,
            price_base_quote,
            amount_in_base,
            filled_amount_in_base,
            TestBase::base(),
            commission_amount_in_base,
        );

        let order_2 = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            trade_side_2,
            price_base_quote,
            amount_in_base,
            filled_amount_in_base,
            TestBase::base(),
            commission_amount_in_base,
        );

        // // Expected
        let base_balance_changed = dec!(0);
        let quote_balance_changed = dec!(0);

        // // Actual
        test_obj
            .calculate_balance_changes(vec![&order_1, &order_2])
            .await;

        let actual_base_balance_changed = test_obj.get_actual_balance_change(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            TestBase::base(),
        );

        let actual_quote_balance_changed = test_obj.get_actual_balance_change(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            TestBase::quote(),
        );

        assert_eq!(actual_base_balance_changed, base_balance_changed);
        assert_eq!(actual_quote_balance_changed, quote_balance_changed);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn two_directions_amount_in_quote_but_sell_by_equal_price_and_amount_nullable_commission(
    ) {
        /*
         *  ///
         *     This case is just to check if the balance changes to zero,
         *     when buying and selling the same amount of currency at the same price and nullable commission.
         * ///
         * /// Buy base in direction 1, sell in direction 2 ///
         *         // Direction 1 //
         * Exchange:                  EXC1
         * Currency pair:             Base/Quote
         * AmountCurrencyCode:        Quote
         * CommissionCurrencyCode:    Base
         * TradeSide:                 Buy
         *        // Direction 2 //
         * Exchange:                  EXC1
         * Currency pair:             Base/Quote
         * AmountCurrencyCode:        Quote
         * CommissionCurrencyCode:    Base
         * TradeSide:                 Sell
         */

        let mut test_obj = TestBase::new(false, false);

        // same for both directions
        let price_base_quote = dec!(1.2);
        let amount_in_quote = dec!(40.2398462);
        let filled_amount_in_quote = amount_in_quote;
        let commission_amount_in_base = dec!(0);

        let trade_side_1 = OrderSide::Buy;
        let trade_side_2 = OrderSide::Sell;

        let order_1 = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            trade_side_1,
            price_base_quote,
            amount_in_quote,
            filled_amount_in_quote,
            TestBase::base(),
            commission_amount_in_base,
        );

        let order_2 = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            trade_side_2,
            price_base_quote,
            amount_in_quote,
            filled_amount_in_quote,
            TestBase::base(),
            commission_amount_in_base,
        );

        // // Expected
        let base_balance_changed = dec!(0);
        let quote_balance_changed = dec!(0);

        // // Actual
        test_obj
            .calculate_balance_changes(vec![&order_1, &order_2])
            .await;

        let actual_base_balance_changed = test_obj.get_actual_balance_change(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            TestBase::base(),
        );

        let actual_quote_balance_changed = test_obj.get_actual_balance_change(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            TestBase::quote(),
        );

        assert_eq!(actual_base_balance_changed, base_balance_changed);
        assert_eq!(actual_quote_balance_changed, quote_balance_changed);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn two_directions_buy_sell() {
        /*
         * /// Buy base in direction 1, sell in direction 2 ///
         *         // Direction 1 //
         * Exchange:                  EXC1
         * Currency pair:             Base/Quote
         * AmountCurrencyCode:        Base
         * CommissionCurrencyCode:    Base
         * TradeSide:                 Buy
         *        // Direction 2 //
         * Exchange:                  EXC1
         * Currency pair:             Base/Quote
         * AmountCurrencyCode:        Base
         * CommissionCurrencyCode:    Quote
         * TradeSide:                 Sell
         */

        let mut test_obj = TestBase::new(false, false);

        // Direction 1 description
        let trade_side_1 = OrderSide::Buy;
        let price_base_quote_1 = dec!(1.843);
        let amount_in_base_1 = dec!(49.1273);
        let filled_amount_in_base_1 = amount_in_base_1;
        let commission_amount_in_base_1 = filled_amount_in_base_1 * TestBase::commission_rate_1();

        // Direction 2 description
        let trade_side_2 = OrderSide::Sell;
        let price_base_quote_2 = dec!(3.1231);
        let amount_in_base_2 = dec!(50);
        let filled_amount_in_base_2 = amount_in_base_2;
        let commission_amount_in_quote_2 =
            filled_amount_in_base_2 * price_base_quote_2 * TestBase::commission_rate_1();

        // Create directions and orders
        let order_1 = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            trade_side_1,
            price_base_quote_1,
            amount_in_base_1,
            filled_amount_in_base_1,
            TestBase::base(),
            commission_amount_in_base_1,
        );
        let order_2 = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            trade_side_2,
            price_base_quote_2,
            amount_in_base_2,
            filled_amount_in_base_2,
            TestBase::quote(),
            commission_amount_in_quote_2,
        );

        // Expected
        let base_balance_changed_1 = amount_in_base_1 - commission_amount_in_base_1;
        let base_balance_changed_2 = -amount_in_base_2;
        let base_balance_changed = base_balance_changed_1 + base_balance_changed_2;

        let quote_balance_changed_1 = -amount_in_base_1 * price_base_quote_1;
        let quote_balance_changed_2 =
            amount_in_base_2 * price_base_quote_2 - commission_amount_in_quote_2;
        let quote_balance_changed = quote_balance_changed_1 + quote_balance_changed_2;

        // // Actual
        test_obj
            .calculate_balance_changes(vec![&order_1, &order_2])
            .await;

        let actual_base_balance_changed = test_obj.get_actual_balance_change(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            TestBase::base(),
        );

        let actual_quote_balance_changed = test_obj.get_actual_balance_change(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            TestBase::quote(),
        );

        assert_eq!(actual_base_balance_changed, base_balance_changed);
        assert_eq!(actual_quote_balance_changed, quote_balance_changed);
    }
}
