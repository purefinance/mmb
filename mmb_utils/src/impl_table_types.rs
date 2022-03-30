use parking_lot::Mutex;
use paste::paste;
use std::collections::HashMap;

use crate::infrastructure::WithExpect;

// Declare methods add_or_get and get_str for structs AppendTable8 and AppendTable16 according to its item bit count
// This types keep items in static tables with fixed sizes
// AppendTable8 (size = 256)
// AppendTable16 (size = 65536)
macro_rules! impl_append_table {
    ($bits: literal) => {
        paste! {
            impl [<AppendTable $bits>] {
                /// Added leaked copy of string to table
                /// Return table index of string equal to specified value if found otherwise create leaked
                /// clone of string, add it to table and return index in table for new value
                pub fn add_or_get(&self, value: &str) -> [<u $bits>] {
                    let mut inner = self.0.lock();
                    let map = &mut inner.map;

                    if let Some(&index) = map.get(value) {
                        return index;
                    }

                    let count = map.len();
                    let index: [<u $bits>] = count.try_into().with_expect(|| {
                        format!(
                            "For AppendTable{} index {} is greater then {} (map: {:?})",
                            $bits, count, [<u $bits>]::MAX, map
                        )
                    });

                    let static_str: &'static str = Box::leak(value.to_owned().into_boxed_str());

                    let _ = map.insert(static_str, index);

                    let str_ptr = inner
                        .buffer
                        .get_mut(index as usize)
                        .expect("index should be inside because we check it in previous step");
                    *str_ptr = Some(static_str);

                    index
                }

                pub fn get_str(&self, index: [<u $bits>]) -> &'static str {
                    // SAFE: use unsafe just for access to item of append only table
                    let inner = unsafe { self.0.data_ptr().as_ref() }
                        .with_expect(|| format!("Can't get pointer to AppendTable{}Inner data", $bits));

                    // Should exist because it assigned when created table item instance
                    inner // can read without lock because buffer is append only table
                        .buffer
                        .get(index as usize)
                        .with_expect(|| format!("Index {} out of bounds of AppendTable{}", index, $bits)) // Index should be correct because we get it when created table item instance
                        .with_expect(|| {
                            format!(
                                "Unable to get string from AppendTable{}[{}] because it's None (table: {:?})",
                                $bits,
                                index,
                                self.0.lock()
                            )
                        })
                }
            }
        }
    };
}

#[derive(Debug)]
struct AppendTable8Inner {
    // supplementary map for fast checking existence same string when creating new AppendTable8 item
    map: HashMap<&'static str, u8>,
    // buffer for append only items
    buffer: [Option<&'static str>; 256],
}

pub struct AppendTable8(Mutex<AppendTable8Inner>);

impl Default for AppendTable8 {
    fn default() -> Self {
        Self::new()
    }
}

/// Append only table for keeping small number (<= 256) of strings with different values
/// Expected using as static table (live whole time that program work)
impl AppendTable8 {
    pub fn new() -> Self {
        AppendTable8(Mutex::new(AppendTable8Inner {
            buffer: [None; 256],
            // creating oversize map for not resizing in future
            map: HashMap::with_capacity(512),
        }))
    }
}

impl_append_table!(8);

#[derive(Debug)]
struct AppendTable16Inner {
    // supplementary map for fast checking existence same string when creating new AppendTable16 item
    map: HashMap<&'static str, u16>,
    // buffer for append only items
    // (Use Vec because array fails with stackoverflow)
    buffer: Vec<Option<&'static str>>,
}

pub struct AppendTable16(Mutex<AppendTable16Inner>);

impl Default for AppendTable16 {
    fn default() -> Self {
        Self::new()
    }
}

/// Append only table for keeping small number (<= 65536) of strings with different values
/// Expected using as static table (live whole time that program work)
impl AppendTable16 {
    pub fn new() -> Self {
        AppendTable16(Mutex::new(AppendTable16Inner {
            buffer: vec![None; 65536],
            map: HashMap::new(),
        }))
    }
}

impl_append_table!(16);

// Implement type with specified name based on AppendTable8 or AppendTable16 with methods:
// from_raw - private constructor
// as_str
//
// and implementations for traits:
// fmt::Display
// serde::Serialize
// serde::Deserialize
#[macro_export]
macro_rules! impl_table_type_raw {
    ($ty: ident, $bits:literal) => {
        paste::paste! {
            #[derive(Copy, Clone, Eq, PartialEq, Hash)]
            pub struct $ty([<u $bits>]);
        }

        paste::paste! {
            #[allow(unused_qualifications)]
            static [<SHARED_ $ty:snake:upper>]: once_cell::sync::Lazy<mmb_utils::impl_table_types::[<AppendTable $bits>]>
                = once_cell::sync::Lazy::new(|| mmb_utils::impl_table_types::[<AppendTable $bits>]::new() );
        }

        impl $ty {
            fn from_raw(value: &str) -> Self {
                Self(paste::paste! { [<SHARED_ $ty:snake:upper>] }.add_or_get(value))
            }

            /// Extracts a string slice containing the entire string.
            pub fn as_str(&self) -> &str {
                paste::paste! { [<SHARED_ $ty:snake:upper>].get_str(self.0) }
            }
        }

        #[allow(unused_qualifications)]
        impl std::fmt::Display for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.as_str(), f)
            }
        }

        #[allow(unused_qualifications)]
        impl std::fmt::Debug for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Debug::fmt( self.as_str(), f)
            }
        }

        paste::paste! {
            struct [<$ty Visitor>];
        }

        paste::paste! {
            #[allow(unused_qualifications)]
            impl<'de> serde::de::Visitor<'de> for [<$ty Visitor>] {
                type Value = $ty;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    write!(formatter, "string for {}", stringify!($ty))
                }

                fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    Ok(<$ty>::from_raw(v))
                }
            }
        }

        #[allow(unused_qualifications)]
        impl<'de> serde::de::Deserialize<'de> for $ty {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::de::Deserializer<'de>,
            {
                paste::paste! {
                    deserializer.deserialize_str([<$ty Visitor>])
                }
            }
        }

        #[allow(unused_qualifications)]
        impl serde::ser::Serialize for $ty {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::ser::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }
    };
}

#[macro_export]
#[allow(unused_qualifications)]
macro_rules! impl_table_type {
    ($ty: ident, $bits:literal) => {
        mmb_utils::impl_table_type_raw!($ty, $bits);

        impl $ty {
            pub fn new(value: &str) -> Self {
                Self(paste::paste! { [<SHARED_ $ty:snake:upper>] }.add_or_get(value))
            }
        }

        impl From<&str> for $ty {
            #[inline]
            fn from(value: &str) -> Self {
                $ty::from_raw(value)
            }
        }
    };
}
