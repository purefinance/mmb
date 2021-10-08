// use chrono::Duration;

// use crate::core::balance_manager::balance_manager::BalanceManager;

// use super::{
//     balance_change::ProfitLossBalanceChange,
//     balance_change_period_selector::BalanceChangePeriodSelector,
// };

// pub(crate) struct BalanceChangeUsdPeriodicCalculator {
//     balance_change_period_selector: BalanceChangePeriodSelector,
//     profit_balance_changes_calculator: ,
// }

// impl BalanceChangeUsdPeriodicCalculator {
//     pub fn new(period: Duration, balance_manager: Option<BalanceManager>) -> Self {
//         Self {
//             balance_change_period_selector: BalanceChangePeriodSelector::new(
//                 period,
//                 balance_manager,
//             ),
//             profit_balance_changes_calculator: ,
//         }
//     }
// }
//     {

//         public BalanceChangeUsdPeriodicCalculator(
//             TimeSpan period,
//             IDateTimeService dateTimeService,
//             IBalanceManager? balanceManager)
//         {
//             Period = period;
//             _dateTimeService = dateTimeService;

//             _balanceChangePeriodSelector = new BalanceChangePeriodSelector(period, dateTimeService, balanceManager);
//             _profitBalanceChangesCalculator = new ProfitBalanceChangesCalculator();
//         }

//         public void AddBalanceChange(ProfitLossBalanceChange balanceChange)
//         {
//             _balanceChangePeriodSelector.Add(balanceChange);
//         }

//         public async Task LoadData(IDatabaseManager databaseManager, CancellationToken cancellationToken)
//         {
//             await using var session = databaseManager.Sql;

//             var fromDate = _dateTimeService.UtcNow - Period;
//             var balanceChanges = await session.Set<ProfitLossBalanceChange>()
//                 .Where(x => x.DateTime >= fromDate)
//                 .OrderBy(x => x.DateTime)
//                 .ToListAsync(cancellationToken);

//             foreach (var balanceChange in balanceChanges)
//             {
//                 _balanceChangePeriodSelector.Add(balanceChange);
//             }
//         }

//         public decimal CalculateRawUsdChange(TradePlace tradePlace)
//         {
//             var items = _balanceChangePeriodSelector.GetItems(tradePlace);
//             return _profitBalanceChangesCalculator.CalculateRaw(items);
//         }

//         public async Task<decimal> CalculateOverMarketUsdChange(
//             IUsdConverter usdConverter,
//             CancellationToken cancellationToken)
//         {
//             var items = _balanceChangePeriodSelector.GetItems();

//             var overMarketByTradePlace = await Task.WhenAll(items.Select(x => _profitBalanceChangesCalculator.CalculateOverMarket(x, usdConverter, cancellationToken)));
//             var overMarket = overMarketByTradePlace.SumF();
//             return overMarket;
//         }

//         // for web
//         public async Task<(decimal raw, decimal overMarket)> CalculateUsdChange(
//             IUsdConverter usdConverter,
//             CancellationToken cancellationToken)
//         {
//             var items = _balanceChangePeriodSelector.GetItems();
//             var raw = items.SumF(x => _profitBalanceChangesCalculator.CalculateRaw(x));
//             var overMarketByTradePlace = await Task.WhenAll(items.Select(x => _profitBalanceChangesCalculator.CalculateOverMarket(x, usdConverter, cancellationToken)));
//             var overMarket = overMarketByTradePlace.SumF();
//             return (raw, overMarket);
//         }
//     }
// }
