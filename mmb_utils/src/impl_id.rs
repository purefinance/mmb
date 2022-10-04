#[macro_export]
macro_rules! impl_from_for_str_id {
    ($from_ty: ty, $to_ty:ty) => {
        impl From<$from_ty> for $to_ty {
            fn from(value: $from_ty) -> Self {
                let mut str = SmallString::new();
                str.write_fmt(format_args!("{value}"))
                    .unwrap_or_else(|err| {
                        panic!(
                            concat!("Can't convert `{}` to ", stringify!($to_ty), ": {:?}"),
                            value, err
                        )
                    });
                Self::new(str)
            }
        }
    };
}

/// This macro needs to generate an string ID for some structures like ClientOrder or ExchangeOrder.
/// All IDs must be unique, here we use AtomicU64 static variable that initialize with current UNIX time(get_atomic_current_secs() function)
/// Value cannot be "0" it means that the var isn't initialized.
/// # Example:
/// ```
/// use std::fmt;
/// use std::fmt::{Display, Formatter};
/// use std::sync::atomic::{AtomicU64, Ordering};
///
/// use once_cell::sync::Lazy;
/// use smallstr::SmallString;
/// use serde::{Deserialize, Serialize};
///
/// use mmb_utils::impl_str_id;
/// use mmb_utils::infrastructure::WithExpect;
/// use mmb_utils::time::get_atomic_current_secs;
///
/// struct Example{};
///
/// impl_str_id!(ExampleId);
/// ```
#[macro_export]
macro_rules! impl_str_id {
    ($type: ident) => {
        paste::paste! {
            static [<$type:snake:upper _ID>]: Lazy<AtomicU64> = Lazy::new(|| get_atomic_current_secs());
        }

        #[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone, Serialize, Deserialize, Hash)]
        #[serde(transparent)]
        pub struct $type(SmallString::<[u8; 16]>);

        impl $type {
            pub fn unique_id() -> Self {
                let new_id = paste::paste! { [<$type:snake:upper _ID>] }.fetch_add(1, Ordering::AcqRel);
                $type(new_id.to_string().into())
            }

            pub fn new(from: SmallString::<[u8; 16]>) -> Self {
                $type(from)
            }

            /// Extracts a string slice containing the entire string.
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }

            /// Extracts a string slice containing the entire string.
            pub fn as_mut_str(&mut self) -> &mut str {
                self.0.as_mut_str()
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl Display for $type {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<&str> for $type {
            fn from(value: &str) -> Self {
                $type(SmallString::<[u8; 16]>::from_str(value))
            }
        }
    }
}

/// This macro needs to generate an u64 ID for some structures like ProfitLossBalanceChange or Reservation.
/// All IDs must be unique, here we use AtomicU64 static variable that initialize with current UNIX time(get_atomic_current_secs() function)
/// Value cannot be "0" it means that the var isn't initialized.
/// # Example:
/// ```
/// use std::fmt;
/// use std::fmt::{Display, Formatter};
/// use std::sync::atomic::{AtomicU64, Ordering};
///
/// use once_cell::sync::Lazy;
/// use serde::{Deserialize, Serialize};
///
/// use mmb_utils::impl_u64_id;
/// use mmb_utils::infrastructure::WithExpect;
/// use mmb_utils::time::get_atomic_current_secs;
///
/// struct Example{};
///
/// impl_u64_id!(ExampleId);
/// ```
#[macro_export]
macro_rules! impl_u64_id {
    ($type: ident) => {
        paste::paste! {
            static [<$type:snake:upper _ID>]: Lazy<AtomicU64> = Lazy::new(|| get_atomic_current_secs());
        }

        #[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Hash, Ord, PartialOrd)]
        #[serde(transparent)]
        pub struct $type(u64);

        impl $type {
            /// Generate unique ID
            pub fn generate() -> Self {
                let new_id = paste::paste! { [<$type:snake:upper _ID>] }.fetch_add(1, Ordering::AcqRel);
                $type(new_id)
            }
        }

        impl Display for $type {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    }
}
