use std::time::Duration;

#[derive(Clone, Debug)]
pub struct BalanceChangePeriodicCalculator {
    // private readonly IDateTimeService _dateTimeService;
    // private readonly BalanceChangePeriodSelector _balanceChangePeriodSelector;
    // private readonly IProfitBalanceChangesCalculator _profitBalanceChangesCalculator;
    pub period: Duration,
}

impl BalanceChangePeriodicCalculator {
    pub fn new(period: Duration) -> BalanceChangePeriodicCalculator {
        BalanceChangePeriodicCalculator { period: period }
    }
}
