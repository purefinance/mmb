use crate::order::snapshot::SortedOrderData;
use crate::order_book::local_order_book_snapshot::LocalOrderBookSnapshot;
use chrono::Utc;
/// Macros allows to specify in much clearer way (then usual imperative code) a structure of
/// order book with template:\
/// order_book_data![\
///   asks list\
///   ;\
///   bids list\
/// ]
///
/// asks and bids can be absent
///
/// Example:
///
/// ```ignore
/// use crate::order_book_data;
/// use rust_decimal_macros::dec;
///
/// let order_book = order_book_data![
///    dec!(1.0) => dec!(2.1),
///    dec!(3.0) => dec!(4.2),
///    ;
///    dec!(2.9) => dec!(7.8),
///    dec!(3.4) => dec!(1.2),
///  ];
/// ```
#[macro_export]
macro_rules! order_book_data {
    ($( $key_a: expr => $val_a: expr ),*, ;
     $( $key_b: expr => $val_b: expr ),*,) => {{
        use rust_decimal::Decimal;
        let mut asks = $crate::order::snapshot::SortedOrderData::new();
        asks.extend(vec![ $( ($key_a, $val_a), )* ] as Vec<(Decimal, Decimal)>);

        let mut bids = $crate::order::snapshot::SortedOrderData::new();
        bids.extend(vec![ $( ($key_b, $val_b), )* ] as Vec<(Decimal, Decimal)>);

        $crate::order_book::order_book_data::OrderBookData::new(asks, bids)
    }};
    ($( $key_a: expr => $val_a: expr ),*, ;) => {{ order_book_data!($( $key_a => $val_a ),*, ;,) }};
    (; $( $key_b: expr => $val_b: expr ),*,) => {{ order_book_data!(, ; $( $key_b => $val_b ),*,) }};
    () => {{ order_book_data!(,;,) }};
}

/// Main asks and bids storage
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBookData {
    pub asks: SortedOrderData,
    pub bids: SortedOrderData,
}

impl OrderBookData {
    pub fn new(asks: SortedOrderData, bids: SortedOrderData) -> Self {
        Self { asks, bids }
    }

    pub fn to_local_order_book_snapshot(&self) -> LocalOrderBookSnapshot {
        LocalOrderBookSnapshot::new(self.asks.clone(), self.bids.clone(), Utc::now())
    }

    /// Perform inner asks and bids update
    pub fn update(&mut self, updates: Vec<OrderBookData>) {
        if updates.is_empty() {
            return;
        }

        self.update_inner_data(updates);
    }

    fn update_inner_data(&mut self, updates: Vec<OrderBookData>) {
        for update in updates.iter() {
            Self::apply_update(&mut self.asks, &mut self.bids, update);
        }
    }

    pub fn apply_update(
        asks: &mut SortedOrderData,
        bids: &mut SortedOrderData,
        update: &OrderBookData,
    ) {
        Self::update_by_side(asks, &update.asks);
        Self::update_by_side(bids, &update.bids);
    }

    fn update_by_side(snapshot: &mut SortedOrderData, update: &SortedOrderData) {
        for (key, amount) in update {
            if amount.is_zero() {
                let _ = snapshot.remove(key);
            } else {
                let _ = snapshot.insert(*key, *amount);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    #[test]
    fn update_asks() {
        // Create updates
        let update = order_book_data![
            dec!(1.0) => dec!(2.0),
            dec!(3.0) => dec!(4.0),
            ;
        ];

        let updates = vec![update];

        // Prepare updated object
        let mut main_order_data = order_book_data![
            dec!(1.0) => dec!(1.0),
            dec!(3.0) => dec!(1.0),
            ;
        ];

        main_order_data.update(updates);

        assert_eq!(main_order_data.asks.get(&dec!(1.0)), Some(&dec!(2.0)));
        assert_eq!(main_order_data.asks.get(&dec!(3.0)), Some(&dec!(4.0)));
    }

    #[test]
    fn bids_update() {
        // Create updates
        let update = order_book_data![
            ;
            dec!(1.0) => dec!(2.2),
            dec!(3.0) => dec!(4.0),
        ];

        let updates = vec![update];

        // Prepare updated object
        let mut main_order_data = order_book_data![
            ;
            dec!(1.0) => dec!(1.0),
            dec!(3.0) => dec!(1.0),
        ];

        main_order_data.update(updates);

        assert_eq!(main_order_data.bids.get(&dec!(1.0)), Some(&dec!(2.2)));
        assert_eq!(main_order_data.bids.get(&dec!(3.0)), Some(&dec!(4.0)));
    }

    #[test]
    fn empty_update() {
        // Prepare data for empty update
        let updates = Vec::new();

        // Prepare updated object
        let mut main_order_data = order_book_data![
            ;
            dec!(1.0) => dec!(1.0),
            dec!(3.0) => dec!(1.0),
        ];

        main_order_data.update(updates);

        assert_eq!(main_order_data.bids.get(&dec!(1.0)), Some(&dec!(1.0)));
        assert_eq!(main_order_data.bids.get(&dec!(3.0)), Some(&dec!(1.0)));
    }

    #[test]
    fn several_updates() {
        // Create updates
        let first_update = order_book_data![
            dec!(1.0) => dec!(2.0),
            dec!(3.0) => dec!(4.0),
            ;
        ];
        let second_update = order_book_data![
            dec!(1.0) => dec!(2.8),
            dec!(6.0) => dec!(0),
            ;
        ];

        let updates = vec![first_update, second_update];

        // Prepare updated object
        let mut main_order_data = order_book_data![
            dec!(1.0) => dec!(1.0),
            dec!(2.0) => dec!(5.6),
            dec!(3.0) => dec!(1.0),
            dec!(6.0) => dec!(1.0),
            ;
        ];

        main_order_data.update(updates);

        // Updated from second update
        assert_eq!(main_order_data.asks.get(&dec!(1.0)), Some(&dec!(2.8)));
        // Unchanged
        assert_eq!(main_order_data.asks.get(&dec!(2.0)), Some(&dec!(5.6)));
        // Updated from first update
        assert_eq!(main_order_data.asks.get(&dec!(3.0)), Some(&dec!(4.0)));
        // Deleted because 0 amount in second update
        assert_eq!(main_order_data.asks.get(&dec!(6.0)), None);
    }
}
