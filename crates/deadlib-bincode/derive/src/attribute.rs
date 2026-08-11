use virtue::prelude::*;
use virtue::utils::{parse_tagged_attribute, ParsedAttribute};

pub struct ContainerAttributes {
    pub crate_name: String,
    pub bounds: Option<(String, Literal)>,
    pub decode_bounds: Option<(String, Literal)>,
    pub decode_context: Option<(String, Literal)>,
    pub borrow_decode_bounds: Option<(String, Literal)>,
    pub encode_bounds: Option<(String, Literal)>,
}

impl Default for ContainerAttributes {
    fn default() -> Self {
        Self {
            crate_name: "::bincode".to_string(),
            bounds: None,
            decode_bounds: None,
            decode_context: None,
            encode_bounds: None,
            borrow_decode_bounds: None,
        }
    }
}

impl FromAttribute for ContainerAttributes {
    fn parse(group: &Group) -> Result<Option<Self>> {
        let attributes = match parse_tagged_attribute(group, "bincode")? {
            Some(body) => body,
            None => return Ok(None),
        };
        let mut result = Self::default();
        for attribute in attributes {
            match attribute {
                ParsedAttribute::Property(key, val) if key.to_string() == "crate" => {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        result.crate_name = val_string[1..val_string.len() - 1].to_string();
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                }
                ParsedAttribute::Property(key, val) if key.to_string() == "bounds" => {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        result.bounds =
                            Some((val_string[1..val_string.len() - 1].to_string(), val));
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                }
                ParsedAttribute::Property(key, val) if key.to_string() == "decode_bounds" => {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        result.decode_bounds =
                            Some((val_string[1..val_string.len() - 1].to_string(), val));
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                }
                ParsedAttribute::Property(key, val) if key.to_string() == "decode_context" => {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        result.decode_context =
                            Some((val_string[1..val_string.len() - 1].to_string(), val));
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                }
                ParsedAttribute::Property(key, val) if key.to_string() == "encode_bounds" => {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        result.encode_bounds =
                            Some((val_string[1..val_string.len() - 1].to_string(), val));
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                }
                ParsedAttribute::Property(key, val)
                    if key.to_string() == "borrow_decode_bounds" =>
                {
                    let val_string = val.to_string();
                    if val_string.starts_with('"') && val_string.ends_with('"') {
                        result.borrow_decode_bounds =
                            Some((val_string[1..val_string.len() - 1].to_string(), val));
                    } else {
                        return Err(Error::custom_at("Should be a literal str", val.span()));
                    }
                }
                ParsedAttribute::Tag(i) => {
                    return Err(Error::custom_at("Unknown field attribute", i.span()))
                }
                ParsedAttribute::Property(key, _) => {
                    return Err(Error::custom_at("Unknown field attribute", key.span()))
                }
            }
        }
        Ok(Some(result))
    }
}

pub struct FieldAttributes;

impl FromAttribute for FieldAttributes {
    fn parse(group: &Group) -> Result<Option<Self>> {
        let attributes = match parse_tagged_attribute(group, "bincode")? {
            Some(body) => body,
            None => return Ok(None),
        };
        if let Some(attribute) = attributes.into_iter().next() {
            let span = match attribute {
                ParsedAttribute::Tag(key) | ParsedAttribute::Property(key, _) => key.span(),
            };
            return Err(Error::custom_at("Unsupported field attribute", span));
        }
        Ok(Some(Self))
    }
}
