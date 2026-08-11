//! Token generation used by the bincode derives.

mod generate_item;
mod generator;
mod impl_for;
mod stream_builder;

use crate::{
    parse::{GenericConstraints, Generics},
    prelude::Ident,
};
use std::fmt;

pub(crate) use self::generate_item::{FnBuilder, FnSelfArg};
pub(crate) use self::generator::Generator;
pub(crate) use self::impl_for::ImplFor;
pub(crate) use self::stream_builder::{PushParseError, StreamBuilder};

pub(crate) trait Parent {
    fn append(&mut self, builder: StreamBuilder);
    fn generics(&self) -> Option<&Generics>;
    fn generic_constraints(&self) -> Option<&GenericConstraints>;
}

pub(crate) enum StringOrIdent {
    String(String),
    Ident(Ident),
}

impl fmt::Display for StringOrIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => s.fmt(f),
            Self::Ident(i) => i.fmt(f),
        }
    }
}

impl From<String> for StringOrIdent {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<Ident> for StringOrIdent {
    fn from(i: Ident) -> Self {
        Self::Ident(i)
    }
}

impl From<&str> for StringOrIdent {
    fn from(s: &str) -> Self {
        Self::String(s.to_owned())
    }
}
