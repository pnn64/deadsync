// Frozen from 78094fd5c (0.5.1651).
use crate::script::ScriptCommand;
use smallvec::SmallVec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptToken<'a> {
    command: ScriptCommand<'a>,
    args: SmallVec<[&'a str; 6]>,
}

impl<'a> ScriptToken<'a> {
    #[must_use]
    pub const fn command(&self) -> ScriptCommand<'a> {
        self.command
    }

    #[must_use]
    pub fn args(&self) -> &[&'a str] {
        &self.args
    }
}

#[inline(always)]
fn split_script_call_args(raw: &str) -> SmallVec<[&str; 6]> {
    let mut out = SmallVec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = 0u8;
    let bytes = raw.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let b = bytes[idx];
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
            idx += 1;
            continue;
        }
        match b {
            b'"' | b'\'' => {
                quote = b;
            }
            b'(' | b'{' | b'[' => {
                depth += 1;
            }
            b')' | b'}' | b']' => {
                depth = depth.saturating_sub(1);
            }
            b',' if depth == 0 => {
                let part = raw[start..idx].trim();
                if !part.is_empty() {
                    out.push(part);
                }
                start = idx + 1;
            }
            _ => {}
        }
        idx += 1;
    }
    let tail = raw[start..].trim();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

#[inline(always)]
#[must_use]
pub fn split_script_token(token: &str) -> Option<ScriptToken<'_>> {
    let parts = split_script_call_args(token.trim());
    if parts.is_empty() {
        return None;
    }
    let command = ScriptCommand::from(parts[0]);
    let args = parts.into_iter().skip(1).collect();
    Some(ScriptToken { command, args })
}
