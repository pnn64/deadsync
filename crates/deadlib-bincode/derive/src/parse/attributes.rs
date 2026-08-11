use super::utils::*;
use crate::prelude::{Delimiter, Group, TokenTree};
use crate::{Error, Result};
use std::iter::Peekable;

/// An attribute for the given struct, enum, field, etc
#[derive(Debug, Clone)]
pub struct Attribute {
    /// The group of tokens of the attribute. You can parse this to get your custom attributes.
    pub tokens: Group,
}

impl Attribute {
    pub(crate) fn try_take(
        input: &mut Peekable<impl Iterator<Item = TokenTree>>,
    ) -> Result<Vec<Self>> {
        let mut result = Vec::new();

        while consume_punct_if(input, '#').is_some() {
            match input.peek() {
                Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket => {
                    let group = assume_group(input.next());
                    result.push(Attribute { tokens: group });
                }
                Some(TokenTree::Group(g)) => {
                    return Err(Error::InvalidRustSyntax {
                        span: g.span(),
                        expected: format!("[] bracket, got {:?}", g.delimiter()),
                    });
                }
                Some(TokenTree::Punct(p)) if p.as_char() == '#' => {
                    // sometimes with empty lines of doc comments, we get two #'s in a row
                    // Just ignore this
                }
                token => return Error::wrong_token(token, "[] group or next # attribute"),
            }
        }
        Ok(result)
    }
}

#[test]
fn test_attributes_try_take() {
    use crate::token_stream;

    let stream = &mut token_stream("struct Foo;");
    assert!(Attribute::try_take(stream).unwrap().is_empty());
    match stream.next().unwrap() {
        TokenTree::Ident(i) => assert_eq!(i, "struct"),
        x => panic!("Expected ident, found {:?}", x),
    }

    let stream = &mut token_stream("#[cfg(test)] struct Foo;");
    assert!(!Attribute::try_take(stream).unwrap().is_empty());
    match stream.next().unwrap() {
        TokenTree::Ident(i) => assert_eq!(i, "struct"),
        x => panic!("Expected ident, found {:?}", x),
    }
}

/// Helper trait for [`AttributeAccess`] methods.
///
/// This can be implemented on your own type to make parsing easier.
///
/// Some functions that can make your life easier:
/// - [`utils::parse_tagged_attribute`] is a helper for parsing attributes in the format of `#[prefix(...)]`
///
/// [`AttributeAccess`]: trait.AttributeAccess.html
/// [`utils::parse_tagged_attribute`]: ../utils/fn.parse_tagged_attribute.html
pub trait FromAttribute: Sized {
    /// Try to parse the given group into your own type. Return `Ok(None)` if the parsing failed or if the attribute was not this type.
    fn parse(group: &Group) -> Result<Option<Self>>;
}

/// Bring useful methods to access attributes of an element.
pub trait AttributeAccess {
    /// Returns the first attribute that returns `Some(Self)`. See [`FromAttribute`] for more information.
    ///
    /// **note**: Will immediately return `Err(_)` on the first error `T` returns.
    fn get_attribute<T: FromAttribute>(&self) -> Result<Option<T>>;
}

impl AttributeAccess for Vec<Attribute> {
    fn get_attribute<T: FromAttribute>(&self) -> Result<Option<T>> {
        for attribute in self.iter() {
            if let Some(attribute) = T::parse(&attribute.tokens)? {
                return Ok(Some(attribute));
            }
        }
        Ok(None)
    }
}
