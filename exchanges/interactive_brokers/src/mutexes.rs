use tokio::sync::Mutex;

#[derive(Default)]
pub struct Mutexes {
    pub get_balance: Mutex<()>,
    pub get_my_trades: Mutex<()>,
    pub get_open_orders: Mutex<()>,
    pub get_positions: Mutex<()>,
}
