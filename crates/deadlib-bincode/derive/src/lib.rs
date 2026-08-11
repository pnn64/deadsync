mod attribute;
mod derive_enum;
mod derive_struct;
mod error;
mod generate;
mod parse;
mod utils;

extern crate self as virtue;

pub(crate) type Result<T = ()> = std::result::Result<T, Error>;
pub(crate) use self::error::Error;

pub(crate) mod prelude {
    pub(crate) use crate::generate::{FnSelfArg, Generator, StreamBuilder};
    pub(crate) use crate::parse::{
        AttributeAccess, Body, EnumVariant, Fields, FromAttribute, Parse,
    };
    pub(crate) use crate::{Error, Result};

    #[cfg(test)]
    pub(crate) use proc_macro2::*;

    #[cfg(not(test))]
    extern crate proc_macro;
    #[cfg(not(test))]
    pub(crate) use proc_macro::*;
}

#[cfg(test)]
pub(crate) fn token_stream(
    s: &str,
) -> std::iter::Peekable<impl Iterator<Item = proc_macro2::TokenTree>> {
    use std::str::FromStr;

    let stream = proc_macro2::TokenStream::from_str(s)
        .unwrap_or_else(|e| panic!("Could not parse code: {:?}\n{:?}", s, e));
    stream.into_iter().peekable()
}

use attribute::ContainerAttributes;
use virtue::prelude::*;

#[proc_macro_derive(Encode, attributes(bincode))]
pub fn derive_encode(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    #[cfg(not(test))]
    {
        derive_encode_inner(input).unwrap_or_else(|e| e.into_token_stream())
    }
    #[cfg(test)]
    {
        derive_encode_inner(input.into())
            .unwrap_or_else(|e| e.into_token_stream())
            .into()
    }
}

fn derive_encode_inner(input: TokenStream) -> Result<TokenStream> {
    let parse = Parse::new(input)?;
    let (mut generator, attributes, body) = parse.into_generator();
    let attributes = attributes
        .get_attribute::<ContainerAttributes>()?
        .unwrap_or_default();

    match body {
        Body::Struct(body) => {
            derive_struct::DeriveStruct {
                fields: body.fields,
                attributes,
            }
            .generate_encode(&mut generator)?;
        }
        Body::Enum(body) => {
            derive_enum::DeriveEnum {
                variants: body.variants,
                attributes,
            }
            .generate_encode(&mut generator)?;
        }
    }

    generator.export_to_file("bincode", "Encode");
    generator.finish()
}

#[proc_macro_derive(Decode, attributes(bincode))]
pub fn derive_decode(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    #[cfg(not(test))]
    {
        derive_decode_inner(input).unwrap_or_else(|e| e.into_token_stream())
    }
    #[cfg(test)]
    {
        derive_decode_inner(input.into())
            .unwrap_or_else(|e| e.into_token_stream())
            .into()
    }
}

fn derive_decode_inner(input: TokenStream) -> Result<TokenStream> {
    let parse = Parse::new(input)?;
    let (mut generator, attributes, body) = parse.into_generator();
    let attributes = attributes
        .get_attribute::<ContainerAttributes>()?
        .unwrap_or_default();

    match body {
        Body::Struct(body) => {
            derive_struct::DeriveStruct {
                fields: body.fields,
                attributes,
            }
            .generate_decode(&mut generator)?;
        }
        Body::Enum(body) => {
            derive_enum::DeriveEnum {
                variants: body.variants,
                attributes,
            }
            .generate_decode(&mut generator)?;
        }
    }

    generator.export_to_file("bincode", "Decode");
    generator.finish()
}

#[proc_macro_derive(BorrowDecode, attributes(bincode))]
pub fn derive_borrow_decode(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    #[cfg(not(test))]
    {
        derive_borrow_decode_inner(input).unwrap_or_else(|e| e.into_token_stream())
    }
    #[cfg(test)]
    {
        derive_borrow_decode_inner(input.into())
            .unwrap_or_else(|e| e.into_token_stream())
            .into()
    }
}

fn derive_borrow_decode_inner(input: TokenStream) -> Result<TokenStream> {
    let parse = Parse::new(input)?;
    let (mut generator, attributes, body) = parse.into_generator();
    let attributes = attributes
        .get_attribute::<ContainerAttributes>()?
        .unwrap_or_default();

    match body {
        Body::Struct(body) => {
            derive_struct::DeriveStruct {
                fields: body.fields,
                attributes,
            }
            .generate_borrow_decode(&mut generator)?;
        }
        Body::Enum(body) => {
            derive_enum::DeriveEnum {
                variants: body.variants,
                attributes,
            }
            .generate_borrow_decode(&mut generator)?;
        }
    }

    generator.export_to_file("bincode", "BorrowDecode");
    generator.finish()
}
