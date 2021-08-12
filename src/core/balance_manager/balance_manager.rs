use std::collections::{HashMap, HashSet};

use crate::core::balances::balance_reservation_manager::BalanceReservationManager;
use crate::core::exchanges::general::exchange::Exchange;

use crate::core::exchanges::common::TradePlaceAccount;
use crate::core::orders::fill::OrderFill;
struct BalanceManager {
    // private readonly IDateTimeService _dateTimeService;
    // private readonly ILogger _logger = Log.ForContext<BalanceManager>();
    // private readonly object _syncObject = new object();
    exchanges_by_id: HashMap<String, Exchange>,

    // private readonly ICurrencyPairToSymbolConverter _currencyPairToSymbolConverter;
    exchange_id_with_restored_positions: HashSet<String>,
    balance_reservation_manager: BalanceReservationManager,
    position_differs_times_in_row_by_exchange_id: HashMap<String, HashMap<String, usize>>,

    // private readonly IDataRecorder? _dataRecorder;
    // private volatile IBalanceChangesService? _balanceChangesService;
    last_order_fills: HashMap<TradePlaceAccount, OrderFill>,
}
