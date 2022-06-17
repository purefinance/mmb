#[cfg(test)]
pub mod tests {
    use rust_decimal_macros::dec;

    use crate::{
        balance::changes::tests::calculator_tests_base::tests::BalanceChangesCalculatorTestsBase,
        orders::order::OrderSide,
    };

    type TestBase = BalanceChangesCalculatorTestsBase;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn simple_buy() {
        let mut test_obj = TestBase::new(true, false);

        let price_base_quote = dec!(0.133);
        let amount_in_quote = dec!(103);
        let amount_in_base = amount_in_quote / price_base_quote;
        let filled_amount_in_quote = amount_in_quote;
        let commission_amount_in_base = amount_in_base * TestBase::commission_rate_1();

        let order = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            OrderSide::Buy,
            price_base_quote,
            amount_in_quote,
            filled_amount_in_quote,
            TestBase::base(),
            commission_amount_in_base,
        );

        // Expected
        let base_balance_changed = amount_in_quote / price_base_quote - commission_amount_in_base;
        let quote_amount_changed = -amount_in_quote;

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
        assert_eq!(actual_quote_balance_changed, quote_amount_changed);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn simple_sell() {
        let mut test_obj = TestBase::new(true, false);

        let price_base_quote = dec!(0.843);
        let amount_in_quote = dec!(12);
        let filled_amount_in_quote = amount_in_quote;
        let commission_amount_in_base =
            filled_amount_in_quote / price_base_quote * TestBase::commission_rate_1();

        let order = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            OrderSide::Sell,
            price_base_quote,
            amount_in_quote,
            filled_amount_in_quote,
            TestBase::base(),
            commission_amount_in_base,
        );

        // Expected
        let base_balance_changed = -amount_in_quote / price_base_quote - commission_amount_in_base;
        let quote_amount_changed = amount_in_quote;

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
        assert_eq!(actual_quote_balance_changed, quote_amount_changed);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn two_directions_buy_sell() {
        /*
         * /// Buy base in direction 1, sell in direction 2 ///
         *         // Direction 1 //
         * Exchange:                  EXC1
         * Currency pair:             Base/Quote
         * AmountCurrencyCode:        Quote
         * CommissionCurrencyCode:    Quote
         * TradeSide:                 Buy
         *        // Direction 2 //
         * Exchange:                  EXC1
         * Currency pair:             Base/Quote
         * AmountCurrencyCode:        Quote
         * CommissionCurrencyCode:    Quote
         * TradeSide:                 Sell
         */

        let mut test_obj = TestBase::new(true, false);

        // Direction 1 description
        let trade_side_1 = OrderSide::Buy;
        let price_base_quote_1 = dec!(0.9483);
        let amount_in_base_1 = dec!(10);
        let amount_in_quote_1 = amount_in_base_1 * price_base_quote_1;
        let filled_amount_in_quote_1 = amount_in_quote_1;
        let commission_amount_in_base_1 =
            amount_in_quote_1 / price_base_quote_1 * TestBase::commission_rate_1();

        // Direction 2 description
        let trade_side_2 = OrderSide::Sell;
        let price_base_quote_2 = dec!(1.5302);
        let amount_in_base_2 = dec!(15);
        let amount_in_quote_2 = amount_in_base_2 * price_base_quote_2;
        let filled_amount_in_quote_2 = amount_in_quote_2;
        let commission_amount_in_base_2 =
            amount_in_quote_2 / price_base_quote_2 * TestBase::commission_rate_1();

        // Create directions and orders
        let order_1 = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            trade_side_1,
            price_base_quote_1,
            amount_in_base_1,
            filled_amount_in_quote_1,
            TestBase::base(),
            commission_amount_in_base_1,
        );
        let order_2 = TestBase::create_order_with_commission_amount(
            TestBase::exchange_account_id_1(),
            TestBase::currency_pair(),
            trade_side_2,
            price_base_quote_2,
            amount_in_quote_2,
            filled_amount_in_quote_2,
            TestBase::quote(),
            commission_amount_in_base_2,
        );

        // Expected
        let base_balance_changed_1 = amount_in_base_1 - commission_amount_in_base_1;
        let base_balance_changed_2 = -amount_in_base_2 - commission_amount_in_base_2;
        let base_balance_changed = base_balance_changed_1 + base_balance_changed_2;

        let quote_balance_changed_1 = -amount_in_quote_1;
        let quote_balance_changed_2 = amount_in_quote_2;
        let quote_balance_changed = quote_balance_changed_1 + quote_balance_changed_2;

        // Actual
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

// TODO: add Reversed derivatives
