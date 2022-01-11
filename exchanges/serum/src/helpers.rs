use anyhow::{anyhow, Result};
use itertools;
use itertools::Itertools;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use solana_program::pubkey::Pubkey;

pub fn decimal_from_u64(n: u64) -> Result<Decimal> {
    Decimal::from_u64(n).ok_or(anyhow!("Error parsing decimal from u64"))
}

pub fn convert64_to_pubkey(arr: [u64; 4]) -> Pubkey {
    let mut key: [u8; 32] = [0; 32];
    arr.iter()
        .flat_map(|x| x.to_le_bytes())
        .enumerate()
        .for_each(|(i, x)| key[i] = x);

    Pubkey::new_from_array(key)
}

pub fn split_once<'a>(in_string: &'a str, separator: &'a str) -> (&'a str, &'a str) {
    in_string
        .split(separator)
        .collect_tuple()
        .expect("Failed to get items of tuple")
}
