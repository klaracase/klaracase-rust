//! Open enums, and the macros that define the closed vocabularies and the response structs.
//!
//! A vocabulary the server may extend is held as [`Open<E>`]: either a value this crate knows
//! (`Known`) or the exact string the server sent (`Other`). Both survive a round trip, so a verdict
//! added by a later API release never fails a call that was already paid for.

use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A closed vocabulary with a fixed wire spelling. Implemented by every enum of this crate that can
/// sit inside [`Open`].
pub trait WireValue: Sized + Copy {
    /// The value as it goes on the wire.
    fn as_str(self) -> &'static str;
    /// Reads a wire value. `None` for a spelling this crate does not know.
    fn from_wire(value: &str) -> Option<Self>;
}

/// A value of a vocabulary that can grow: one this crate knows, or the server's own string.
///
/// `Open<Verdict>` compares equal to a bare `Verdict`, so `finding.verdict == Some(Verdict::NotFound)`
/// style checks stay short. An unknown spelling is kept byte for byte in [`Open::Other`].
///
/// ```
/// use klaracase::{Open, Verdict};
///
/// let known: Open<Verdict> = serde_json::from_str("\"not_found\"").unwrap();
/// assert_eq!(known, Verdict::NotFound);
///
/// let later: Open<Verdict> = serde_json::from_str("\"quote_mismatch\"").unwrap();
/// assert_eq!(later.known(), None);
/// assert_eq!(later.as_str(), "quote_mismatch");
/// assert_eq!(serde_json::to_string(&later).unwrap(), "\"quote_mismatch\"");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Open<E> {
    /// A value this crate knows.
    Known(E),
    /// A value the server sent that this crate does not know yet.
    Other(String),
}

impl<E: WireValue> Open<E> {
    /// The known value, if this is one.
    #[must_use]
    pub fn known(&self) -> Option<E> {
        match self {
            Self::Known(value) => Some(*value),
            Self::Other(_) => None,
        }
    }

    /// The wire spelling, known or not.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Known(value) => WireValue::as_str(*value),
            Self::Other(text) => text,
        }
    }
}

impl<E: WireValue + PartialEq> PartialEq<E> for Open<E> {
    fn eq(&self, other: &E) -> bool {
        matches!(self, Self::Known(value) if value == other)
    }
}

impl<E: WireValue> fmt::Display for Open<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<E: WireValue> From<E> for Open<E> {
    fn from(value: E) -> Self {
        Self::Known(value)
    }
}

impl<E: WireValue> From<&str> for Open<E> {
    fn from(value: &str) -> Self {
        E::from_wire(value).map_or_else(|| Self::Other(value.to_owned()), Self::Known)
    }
}

impl<E: WireValue> From<String> for Open<E> {
    fn from(value: String) -> Self {
        E::from_wire(&value).map_or(Self::Other(value), Self::Known)
    }
}

impl<E: WireValue> FromStr for Open<E> {
    type Err = Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(value))
    }
}

impl<E: WireValue> Serialize for Open<E> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de, E: WireValue> Deserialize<'de> for Open<E> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(String::deserialize(deserializer)?))
    }
}

/// The schema of an open vocabulary: the list of values this crate knows.
///
/// The server may accept more values, and the string forms of the setters send them as given. The
/// schema is meant for tool definitions, where a closed list guides a model best, so it does not
/// add "or any other string". A validator that must accept a later value should not use it.
#[cfg(feature = "schemars")]
impl<E: schemars::JsonSchema> schemars::JsonSchema for Open<E> {
    fn inline_schema() -> bool {
        E::inline_schema()
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        E::schema_name()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        E::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        E::json_schema(generator)
    }
}

/// A JSON `null` reads as the default value, so a null list never fails a call that was paid for.
pub(crate) fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// Defines a closed vocabulary with a fixed wire spelling.
///
/// The enum is `#[non_exhaustive]`, orders by declaration, and implements `Display`, `FromStr`,
/// [`WireValue`] and serde (an unknown spelling is an error). Wrap it in [`Open`] to read a
/// vocabulary the server can extend.
macro_rules! wire_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $($(#[$vmeta:meta])* $variant:ident => $wire:literal),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
            ::serde::Serialize, ::serde::Deserialize,
        )]
        #[cfg_attr(feature = "schemars", derive(::schemars::JsonSchema))]
        #[non_exhaustive]
        pub enum $name {
            $($(#[$vmeta])* #[serde(rename = $wire)] $variant),+
        }

        impl $name {
            /// The value as it goes on the wire.
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire),+
                }
            }
        }

        impl $crate::open::WireValue for $name {
            fn as_str(self) -> &'static str {
                $name::as_str(self)
            }

            fn from_wire(value: &str) -> Option<Self> {
                match value {
                    $($wire => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::types::UnknownValue;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                <Self as $crate::open::WireValue>::from_wire(value)
                    .ok_or_else(|| $crate::types::UnknownValue::new(stringify!($name), value))
            }
        }
    };
}
pub(crate) use wire_enum;

/// Defines a response struct.
///
/// Field kinds, written before the name:
///
/// - `req`: a required identity field of type `T`.
/// - `opt`: `Option<T>`, skipped when serializing if `None`. A missing key and a `null` both read as
///   `None`. The crate never invents a default.
/// - `list`: `Vec<T>`. A missing key and a `null` read as the empty list.
/// - `dflt`: `T: Default`, with the same rule as `list`.
///
/// Every struct is `#[non_exhaustive]` and ends with a flattened `extra` map that keeps every key the
/// crate does not model, in server order.
macro_rules! response_struct {
    (
        $(#[$meta:meta])*
        $name:ident { $($body:tt)* }
    ) => {
        $crate::open::response_struct!(@acc [$(#[$meta])*] $name [] $($body)*);
    };
    (@acc [$($sm:tt)*] $name:ident [$($done:tt)*]
        $(#[$fm:meta])* req $f:ident : $t:ty , $($rest:tt)*) => {
        $crate::open::response_struct!(@acc [$($sm)*] $name [$($done)*
            $(#[$fm])* pub $f: $t,
        ] $($rest)*);
    };
    (@acc [$($sm:tt)*] $name:ident [$($done:tt)*]
        $(#[$fm:meta])* opt $f:ident : $t:ty , $($rest:tt)*) => {
        $crate::open::response_struct!(@acc [$($sm)*] $name [$($done)*
            $(#[$fm])*
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub $f: Option<$t>,
        ] $($rest)*);
    };
    (@acc [$($sm:tt)*] $name:ident [$($done:tt)*]
        $(#[$fm:meta])* list $f:ident : $t:ty , $($rest:tt)*) => {
        $crate::open::response_struct!(@acc [$($sm)*] $name [$($done)*
            $(#[$fm])*
            #[serde(default, deserialize_with = "crate::open::null_default")]
            pub $f: Vec<$t>,
        ] $($rest)*);
    };
    (@acc [$($sm:tt)*] $name:ident [$($done:tt)*]
        $(#[$fm:meta])* dflt $f:ident : $t:ty , $($rest:tt)*) => {
        $crate::open::response_struct!(@acc [$($sm)*] $name [$($done)*
            $(#[$fm])*
            #[serde(default, deserialize_with = "crate::open::null_default")]
            pub $f: $t,
        ] $($rest)*);
    };
    (@acc [$($sm:tt)*] $name:ident [$($done:tt)*]) => {
        $($sm)*
        #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
        #[non_exhaustive]
        pub struct $name {
            $($done)*
            /// Keys this crate does not model, in the order the server sent them.
            #[serde(flatten)]
            pub extra: ::serde_json::Map<String, ::serde_json::Value>,
        }

        impl $crate::response::Extras for $name {
            fn extra(&self) -> &::serde_json::Map<String, ::serde_json::Value> {
                &self.extra
            }
        }
    };
}
pub(crate) use response_struct;
