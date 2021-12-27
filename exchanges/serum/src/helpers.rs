use anyhow::format_err;
use anyhow::Result;
use safe_transmute::transmute_many_pedantic;
use solana_program::pubkey::Pubkey;
use std::borrow::Cow;

pub fn remove_dex_account_padding<'a>(data: &'a [u8]) -> Result<Cow<'a, [u64]>> {
    use serum_dex::state::{ACCOUNT_HEAD_PADDING, ACCOUNT_TAIL_PADDING};
    let head = &data[..ACCOUNT_HEAD_PADDING.len()];
    if data.len() < ACCOUNT_HEAD_PADDING.len() + ACCOUNT_TAIL_PADDING.len() {
        return Err(format_err!(
            "dex account length {} is too small to contain valid padding",
            data.len()
        ));
    }
    if head != ACCOUNT_HEAD_PADDING {
        return Err(format_err!("dex account head padding mismatch"));
    }
    let tail = &data[data.len() - ACCOUNT_TAIL_PADDING.len()..];
    if tail != ACCOUNT_TAIL_PADDING {
        return Err(format_err!("dex account tail padding mismatch"));
    }
    let inner_data_range = ACCOUNT_HEAD_PADDING.len()..(data.len() - ACCOUNT_TAIL_PADDING.len());
    let inner: &'a [u8] = &data[inner_data_range];
    let words: Cow<'a, [u64]> = match transmute_many_pedantic::<u64>(inner) {
        Ok(word_slice) => Cow::Borrowed(word_slice),
        Err(transmute_error) => {
            let word_vec = transmute_error.copy().map_err(|e| e.without_src())?;
            Cow::Owned(word_vec)
        }
    };
    Ok(words)
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
    let mut splitter = in_string.splitn(2, separator);
    let first = splitter.next().expect("Failed to get first tuple item");
    let second = splitter.next().expect("Failed to get second tuple item");
    (first, second)
}
