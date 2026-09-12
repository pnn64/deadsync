// Frozen from 29b11efb8 (0.5.1157); shared unchanged node/error types and test visibility.
use super::{XmlError, XmlNode};

pub(crate) fn parse(input: &str) -> Result<XmlNode, XmlError> {
    let mut p = Parser { input, i: 0 };
    p.skip_prolog()?;
    let root = p.parse_element()?;
    Ok(root)
}

struct Parser<'a> {
    // ASCII XML delimiters bound spans in this already-validated UTF-8 input.
    input: &'a str,
    i: usize,
}

impl Parser<'_> {
    #[inline]
    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.i).copied()
    }

    #[inline]
    fn starts_with(&self, s: &str) -> bool {
        self.input[self.i..].starts_with(s)
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    /// Skips XML declaration, comments, processing instructions, DOCTYPE and
    /// surrounding whitespace until the first real element start tag.
    fn skip_prolog(&mut self) -> Result<(), XmlError> {
        loop {
            self.skip_ws();
            if self.starts_with("<?") {
                self.skip_until("?>", "processing instruction")?;
            } else if self.starts_with("<!--") {
                self.skip_until("-->", "comment")?;
            } else if self.starts_with("<!") {
                // DOCTYPE or similar: skip to the next '>'.
                self.skip_until(">", "declaration")?;
            } else {
                return Ok(());
            }
        }
    }

    fn skip_until(&mut self, end: &str, what: &'static str) -> Result<(), XmlError> {
        if let Some(pos) = self.input[self.i..].find(end) {
            self.i += pos + end.len();
            Ok(())
        } else {
            Err(XmlError::Unterminated(what))
        }
    }

    /// Parses one element starting at a `<` that introduces a normal tag.
    fn parse_element(&mut self) -> Result<XmlNode, XmlError> {
        if self.peek() != Some(b'<') {
            return Err(XmlError::NoRoot);
        }
        self.i += 1; // consume '<'

        let tag = self.read_name();
        if tag.is_empty() {
            return Err(XmlError::Malformed("tag name"));
        }
        let mut node = XmlNode {
            tag,
            ..Default::default()
        };

        // Attributes.
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'/') => {
                    // Self-closing.
                    self.i += 1;
                    if self.peek() == Some(b'>') {
                        self.i += 1;
                        return Ok(node);
                    }
                    return Err(XmlError::Malformed("self-closing tag"));
                }
                Some(b'>') => {
                    self.i += 1;
                    break;
                }
                Some(_) => {
                    let name = self.read_name();
                    if name.is_empty() {
                        return Err(XmlError::Malformed("attribute name"));
                    }
                    self.skip_ws();
                    if self.peek() != Some(b'=') {
                        // Valueless attribute; record empty and continue.
                        node.attrs.push((name, String::new()));
                        continue;
                    }
                    self.i += 1; // '='
                    self.skip_ws();
                    let value = self.read_attr_value()?;
                    node.attrs.push((name, value));
                }
                None => return Err(XmlError::Unterminated("start tag")),
            }
        }

        // Content until matching end tag.
        let mut text = String::new();
        loop {
            match self.peek() {
                None => return Err(XmlError::Unterminated("element")),
                Some(b'<') => {
                    if self.starts_with("<!--") {
                        self.skip_until("-->", "comment")?;
                    } else if self.starts_with("<![CDATA[") {
                        self.i += "<![CDATA[".len();
                        let start = self.i;
                        if let Some(pos) = self.input[self.i..].find("]]>") {
                            text.push_str(&self.input[start..start + pos]);
                            self.i += pos + 3;
                        } else {
                            return Err(XmlError::Unterminated("CDATA"));
                        }
                    } else if self.starts_with("</") {
                        self.i += 2;
                        self.skip_name();
                        self.skip_ws();
                        if self.peek() == Some(b'>') {
                            self.i += 1;
                        } else {
                            return Err(XmlError::Malformed("end tag"));
                        }
                        trim_string_in_place(&mut text);
                        node.text = text;
                        return Ok(node);
                    } else {
                        let child = self.parse_element()?;
                        node.children.push(child);
                    }
                }
                Some(_) => {
                    // Text run up to the next '<'.
                    let start = self.i;
                    self.i += self.input[start..]
                        .find('<')
                        .unwrap_or(self.input.len() - start);
                    append_decoded_entities(&mut text, &self.input[start..self.i]);
                }
            }
        }
    }

    fn read_name(&mut self) -> String {
        let start = self.i;
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() || c == b'>' || c == b'/' || c == b'=' {
                break;
            }
            self.i += 1;
        }
        self.input[start..self.i].to_owned()
    }

    fn skip_name(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() || c == b'>' || c == b'/' || c == b'=' {
                break;
            }
            self.i += 1;
        }
    }

    fn read_attr_value(&mut self) -> Result<String, XmlError> {
        let quote = match self.peek() {
            Some(q @ (b'"' | b'\'')) => q,
            _ => return Err(XmlError::Malformed("attribute value")),
        };
        self.i += 1;
        let start = self.i;
        let Some(len) = self.input[start..].find(char::from(quote)) else {
            return Err(XmlError::Unterminated("attribute value"));
        };
        self.i = start + len + 1; // closing quote
        Ok(decode_entities(&self.input[start..start + len]))
    }
}

fn trim_string_in_place(text: &mut String) {
    let trimmed = text.trim();
    let start = trimmed.as_ptr() as usize - text.as_ptr() as usize;
    let end = start + trimmed.len();
    text.truncate(end);
    if start != 0 {
        drop(text.drain(..start));
    }
}

/// Decodes the five predefined XML entities and numeric character references.
fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    append_decoded_entities(&mut out, s);
    out
}

pub(super) fn append_decoded_entities(out: &mut String, s: &str) {
    if !s.contains('&') {
        out.push_str(s);
        return;
    }
    out.reserve(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&'
            && let Some(semi) = s[i + 1..].find(';')
        {
            let entity = &s[i + 1..i + 1 + semi];
            let decoded = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => decode_numeric_entity(entity),
            };
            if let Some(ch) = decoded {
                out.push(ch);
                i += semi + 2;
                continue;
            }
        }
        // Not an entity we recognize: copy the byte as a char.
        let ch_len = utf8_char_len(bytes[i]);
        let end = (i + ch_len).min(s.len());
        out.push_str(&s[i..end]);
        i = end;
    }
}

fn decode_numeric_entity(entity: &str) -> Option<char> {
    let rest = entity.strip_prefix('#')?;
    let code = if let Some(hex) = rest.strip_prefix(['x', 'X']) {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        rest.parse::<u32>().ok()?
    };
    char::from_u32(code)
}

#[inline]
const fn utf8_char_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}
