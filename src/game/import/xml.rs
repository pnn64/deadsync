//! A tiny, dependency-free XML reader scoped to the well-formed, machine-written
//! documents DeadSync needs to import (ITGmania `Stats.xml`). It is **not** a
//! general-purpose XML parser: it understands elements, attributes, text,
//! self-closing tags, comments, CDATA, the `<?xml ?>` declaration, and the five
//! predefined entities plus numeric character references. That is sufficient for
//! ITGmania's output and keeps us from pulling in a heavyweight XML dependency.

/// A parsed XML element node.
#[derive(Debug, Clone, Default)]
pub struct XmlNode {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<XmlNode>,
    /// Concatenated direct text content (whitespace-trimmed at the edges).
    pub text: String,
}

impl XmlNode {
    /// Returns the value of an attribute by name, if present.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Returns the first direct child element with the given tag.
    pub fn child(&self, tag: &str) -> Option<&XmlNode> {
        self.children.iter().find(|c| c.tag == tag)
    }

    /// Iterates direct child elements with the given tag.
    pub fn children_named<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a XmlNode> + 'a {
        self.children.iter().filter(move |c| c.tag == tag)
    }

    /// Trimmed text of the first direct child with the given tag, or `""`.
    pub fn child_text(&self, tag: &str) -> &str {
        self.child(tag).map(|c| c.text.as_str()).unwrap_or("")
    }

    /// Parses the text of a direct child into `T`, returning `None` when the
    /// child is absent, empty, or fails to parse.
    pub fn child_parse<T: std::str::FromStr>(&self, tag: &str) -> Option<T> {
        let t = self.child_text(tag).trim();
        if t.is_empty() {
            None
        } else {
            t.parse::<T>().ok()
        }
    }
}

#[derive(Debug)]
pub enum XmlError {
    Unterminated(&'static str),
    Malformed(&'static str),
    NoRoot,
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unterminated(what) => write!(f, "unterminated {what}"),
            Self::Malformed(what) => write!(f, "malformed {what}"),
            Self::NoRoot => write!(f, "no root element"),
        }
    }
}

impl std::error::Error for XmlError {}

/// Parses an XML document and returns its single root element.
pub fn parse(input: &str) -> Result<XmlNode, XmlError> {
    let bytes = input.as_bytes();
    let mut p = Parser { b: bytes, i: 0 };
    p.skip_prolog()?;
    let root = p.parse_element()?;
    Ok(root)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    #[inline]
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    #[inline]
    fn starts_with(&self, s: &str) -> bool {
        self.b[self.i..].starts_with(s.as_bytes())
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
        if let Some(pos) = find_sub(&self.b[self.i..], end.as_bytes()) {
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
                        if let Some(pos) = find_sub(&self.b[self.i..], b"]]>") {
                            text.push_str(&String::from_utf8_lossy(&self.b[start..start + pos]));
                            self.i += pos + 3;
                        } else {
                            return Err(XmlError::Unterminated("CDATA"));
                        }
                    } else if self.starts_with("</") {
                        self.i += 2;
                        let _close = self.read_name();
                        self.skip_ws();
                        if self.peek() == Some(b'>') {
                            self.i += 1;
                        } else {
                            return Err(XmlError::Malformed("end tag"));
                        }
                        node.text = text.trim().to_string();
                        return Ok(node);
                    } else {
                        let child = self.parse_element()?;
                        node.children.push(child);
                    }
                }
                Some(_) => {
                    // Text run up to the next '<'.
                    let start = self.i;
                    while let Some(c) = self.peek() {
                        if c == b'<' {
                            break;
                        }
                        self.i += 1;
                    }
                    let raw = &self.b[start..self.i];
                    text.push_str(&decode_entities(&String::from_utf8_lossy(raw)));
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
        String::from_utf8_lossy(&self.b[start..self.i]).into_owned()
    }

    fn read_attr_value(&mut self) -> Result<String, XmlError> {
        let quote = match self.peek() {
            Some(q @ (b'"' | b'\'')) => q,
            _ => return Err(XmlError::Malformed("attribute value")),
        };
        self.i += 1;
        let start = self.i;
        while let Some(c) = self.peek() {
            if c == quote {
                let raw = &self.b[start..self.i];
                self.i += 1; // closing quote
                return Ok(decode_entities(&String::from_utf8_lossy(raw)));
            }
            self.i += 1;
        }
        Err(XmlError::Unterminated("attribute value"))
    }
}

fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Decodes the five predefined XML entities and numeric character references.
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(semi) = s[i + 1..].find(';') {
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
        }
        // Not an entity we recognize: copy the byte as a char.
        let ch_len = utf8_char_len(bytes[i]);
        let end = (i + ch_len).min(s.len());
        out.push_str(&s[i..end]);
        i = end;
    }
    out
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
fn utf8_char_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_with_attrs_and_text() {
        let xml = r#"<?xml version="1.0"?>
        <!-- a comment -->
        <Stats>
          <SongScores>
            <Song Dir="Songs/Pack/Song &amp; Friends/">
              <Steps StepsType="dance-single" Difficulty="Hard">
                <HighScoreList>
                  <NumTimesPlayed>3</NumTimesPlayed>
                  <HighScore>
                    <Grade>Grade_Tier01</Grade>
                    <PercentDP>0.9912</PercentDP>
                  </HighScore>
                </HighScoreList>
              </Steps>
            </Song>
          </SongScores>
        </Stats>"#;
        let root = parse(xml).expect("parse");
        assert_eq!(root.tag, "Stats");
        let song = root
            .child("SongScores")
            .and_then(|s| s.child("Song"))
            .expect("song");
        assert_eq!(song.attr("Dir"), Some("Songs/Pack/Song & Friends/"));
        let steps = song.child("Steps").expect("steps");
        assert_eq!(steps.attr("StepsType"), Some("dance-single"));
        assert_eq!(steps.attr("Difficulty"), Some("Hard"));
        let hs = steps
            .child("HighScoreList")
            .and_then(|l| l.child("HighScore"))
            .expect("hs");
        assert_eq!(hs.child_text("Grade"), "Grade_Tier01");
        assert_eq!(hs.child_parse::<f64>("PercentDP"), Some(0.9912));
    }

    #[test]
    fn handles_self_closing_and_entities() {
        let xml = r#"<root><W1>5</W1><Empty/><Note text="a &lt; b &#65;"/></root>"#;
        let root = parse(xml).expect("parse");
        assert_eq!(root.child_parse::<u32>("W1"), Some(5));
        assert!(root.child("Empty").is_some());
        assert_eq!(root.child("Note").unwrap().attr("text"), Some("a < b A"));
    }

    #[test]
    fn counts_repeated_children() {
        let xml = "<l><h>1</h><h>2</h><h>3</h></l>";
        let root = parse(xml).expect("parse");
        assert_eq!(root.children_named("h").count(), 3);
    }
}
