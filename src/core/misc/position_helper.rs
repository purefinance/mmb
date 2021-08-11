use crate::core::balance_changes::balance_manager::balance_manager::BalanceManager;
use crate::core::exchanges::common::TradePlaceAccount;
pub struct PositionHelper {}

impl PositionHelper {
    pub fn close_position_if_needed(
        trade_place: TradePlaceAccount,
        balance_manager: BalanceManager,
    ) {
    }

    // pub fn spawn_task_close_position()
    // public static Task SpawnTaskClosePosition(FreeTasksPool freeTasksPool, IBotApi botApi, ILogger log)
    // {
    //     return freeTasksPool.SpawnTask(
    //         "Close active positions",
    //         nameof(ProfitLossStopper),
    //         TimeSpan.FromSeconds(30),
    //         async () =>
    //         {
    //             log.Verbose("Started closing active positions");
    //             await botApi.CloseActivePositions();
    //             log.Verbose("Finished closing active positions");
    //         });
    // }
}
