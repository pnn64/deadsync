//! A standalone Lua compiler and register interpreter written in safe Rust.
//!
//! This crate is under construction. See the README for its current language coverage.
//! Compilation does not depend on a VM, and a compiled program can run in multiple VMs.
//!
//! ```
//! use deadlib_lua::{compile, Value, Vm};
//! let program = compile(b"local x = 6; return x * 7")?;
//! let mut vm = Vm::new();
//! assert_eq!(vm.execute(&program)?, &[Value::Number(42.0)]);
//! # Ok::<(), deadlib_lua::Error>(())
//! ```

#![forbid(unsafe_code)]

mod arena;
mod bytecode;
mod compiler;
mod error;
mod format;
mod lexer;
mod optimizer;
mod parser;
mod pattern;
mod table;
mod value;
mod vm;

#[doc(inline)]
pub use bytecode::Program;
#[doc(inline)]
pub use error::{Error, ErrorKind};
#[doc(inline)]
pub use value::{FunctionId, StringId, TableId, ThreadId, Value};
#[doc(inline)]
pub use vm::{HeapStats, Root, Vm};

/// Compile Lua 5.1 source into verified, VM-independent bytecode.
///
/// # Errors
/// Returns a syntax or limit error for invalid source or unsupported constructs.
pub fn compile(source: &[u8]) -> Result<Program, Error> {
    compile_named(source, b"=(deadlib-lua)")
}

/// Compile Lua 5.1 source with a chunk name for errors and stack locations.
///
/// A leading `=` supplies a literal display name; `@` introduces a filename.
/// Other names are displayed as Lua source excerpts.
///
/// # Errors
/// Returns syntax or limit errors for invalid source.
pub fn compile_named(source: &[u8], name: &[u8]) -> Result<Program, Error> {
    compiler::compile(parser::parse(source)?, name)
}
