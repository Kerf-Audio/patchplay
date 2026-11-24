//! Validated identifiers. Every id that reaches a `Patch` has passed these
//! checks, so the text printer never has to quote or escape them.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error(
        "`{0}` is not a valid id: use lower-case letters, digits and `_`, starting with a letter"
    )]
    Invalid(String),
    #[error("`{0}` is not a jack reference: expected `module.jack`")]
    JackRef(String),
    #[error("`{0}` is not a scene: expected A, B, C or D")]
    Scene(String),
    #[error("`{0}` is not a colour: expected `#rrggbb`")]
    Colour(String),
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

macro_rules! ident {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Result<Self, IdError> {
                let s = s.into();
                if is_ident(&s) { Ok(Self(s)) } else { Err(IdError::Invalid(s)) }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;
            fn try_from(s: String) -> Result<Self, IdError> {
                Self::new(s)
            }
        }

        impl From<$name> for String {
            fn from(v: $name) -> String {
                v.0
            }
        }

        impl FromStr for $name {
            type Err = IdError;
            fn from_str(s: &str) -> Result<Self, IdError> {
                Self::new(s)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

ident! {
    /// A module instance in a patch, e.g. `osc1`.
    ModuleId
}
ident! {
    /// A part (one row on the workbench), e.g. `bass`.
    PartId
}
ident! {
    /// A param or jack key on a module, e.g. `cutoff`, `voct`.
    Key
}

/// One end of a cable: `module.jack`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct JackRef {
    pub module: ModuleId,
    pub jack: Key,
}

impl JackRef {
    pub fn new(module: ModuleId, jack: Key) -> Self {
        Self { module, jack }
    }
}

impl FromStr for JackRef {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, IdError> {
        let (module, jack) = s
            .split_once('.')
            .ok_or_else(|| IdError::JackRef(s.to_owned()))?;
        Ok(Self {
            module: module.parse()?,
            jack: jack.parse()?,
        })
    }
}

impl TryFrom<String> for JackRef {
    type Error = IdError;
    fn try_from(s: String) -> Result<Self, IdError> {
        s.parse()
    }
}

impl From<JackRef> for String {
    fn from(j: JackRef) -> String {
        j.to_string()
    }
}

impl fmt::Display for JackRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.module, self.jack)
    }
}

/// A scene slot. Scenes snapshot every param so the morph fader can glide between two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Scene {
    A,
    B,
    C,
    D,
}

impl FromStr for Scene {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, IdError> {
        match s {
            "A" => Ok(Self::A),
            "B" => Ok(Self::B),
            "C" => Ok(Self::C),
            "D" => Ok(Self::D),
            _ => Err(IdError::Scene(s.to_owned())),
        }
    }
}

impl fmt::Display for Scene {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// A part colour, `#rrggbb`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Colour(String);

impl Colour {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Colour {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, IdError> {
        let ok =
            s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit());
        if ok {
            Ok(Self(s.to_ascii_lowercase()))
        } else {
            Err(IdError::Colour(s.to_owned()))
        }
    }
}

impl TryFrom<String> for Colour {
    type Error = IdError;
    fn try_from(s: String) -> Result<Self, IdError> {
        s.parse()
    }
}

impl From<Colour> for String {
    fn from(c: Colour) -> String {
        c.0
    }
}

impl fmt::Display for Colour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_reject_what_the_text_format_cannot_hold() {
        assert!(ModuleId::new("osc1").is_ok());
        assert!(ModuleId::new("lfo_2").is_ok());
        for bad in ["", "1osc", "Osc", "osc-1", "osc 1", "osc.1", "ösc"] {
            assert!(ModuleId::new(bad).is_err(), "{bad:?} accepted");
        }
    }

    #[test]
    fn jack_refs_need_both_halves() {
        let j: JackRef = "osc1.saw".parse().unwrap();
        assert_eq!(j.to_string(), "osc1.saw");
        for bad in ["osc1", "osc1.", ".saw", "osc1.saw.x"] {
            assert!(bad.parse::<JackRef>().is_err(), "{bad:?} accepted");
        }
    }

    #[test]
    fn colours_are_six_hex_digits() {
        assert_eq!("#3D63C9".parse::<Colour>().unwrap().as_str(), "#3d63c9");
        for bad in ["3d63c9", "#3d63c", "#3d63cg", "#3d63c9ff"] {
            assert!(bad.parse::<Colour>().is_err(), "{bad:?} accepted");
        }
    }
}
