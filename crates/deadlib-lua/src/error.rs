//! Source, bytecode, and runtime diagnostics.

use crate::Value;
use std::fmt;

/// The stage that reported an error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    /// Invalid Lua source.
    Syntax,
    /// A compiler or runtime resource limit was exceeded.
    Limit,
    /// An invalid bytecode operand or control flow edge.
    Bytecode,
    /// A Lua execution error.
    Runtime,
    /// A language construct that is not implemented yet.
    Unsupported,
}

/// An engine error with a one-based source line, when available.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    kind: ErrorKind,
    line: u32,
    message: Box<str>,
    value: Option<Value>,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, line: u32, message: impl Into<Box<str>>) -> Self {
        Self {
            kind,
            line,
            message: message.into(),
            value: None,
        }
    }

    pub(crate) fn raised(value: Value, message: impl Into<Box<str>>) -> Self {
        Self {
            kind: ErrorKind::Runtime,
            line: 0,
            message: message.into(),
            value: Some(value),
        }
    }

    pub(crate) fn at_line(mut self, line: u32) -> Self {
        if self.line == 0 {
            self.line = line;
        }
        self
    }

    pub(crate) fn with_value(mut self, value: Value) -> Self {
        self.value = Some(value);
        self
    }

    /// Return an explicitly raised Lua error value, owned by its originating VM.
    pub fn value(&self) -> Option<Value> {
        self.value
    }

    /// Return the stage that reported this error.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Return the one-based source line, or zero if unavailable.
    pub fn line(&self) -> u32 {
        self.line
    }

    /// Return the diagnostic text without its source location.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line != 0 {
            write!(f, "line {}: ", self.line)?;
        }
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}
