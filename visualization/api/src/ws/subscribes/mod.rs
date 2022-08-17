pub mod balance;
pub mod liquidity;

pub trait Subscription {
    fn get_hash(&self) -> u64;
}
