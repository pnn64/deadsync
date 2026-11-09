//! A standalone Lua compiler and register interpreter written in Rust.
//!
//! The embedding interface is Rust-only. See the README for supported capabilities.
//! Compilation does not depend on a VM, and a compiled program can run in multiple VMs.
//!
//! ```
//! use deadlib_lua::{compile, Value, Vm};
//! let program = compile(b"local x = 6; return x * 7")?;
//! let mut vm = Vm::new();
//! assert_eq!(vm.execute(&program)?, &[Value::Number(42.0)]);
//! # Ok::<(), deadlib_lua::Error>(())
//! ```

#![deny(unsafe_code)]

#[cfg(test)]
#[allow(dead_code)]
mod reference_test;

#[cfg(test)]
#[path = "../tests/support/allocation_probe.rs"]
mod allocation_probe;
mod arena;
mod bytecode;
mod cache;
mod chunk51;
mod compiler;
mod error;
mod format;
mod lexer;
mod locale;
#[allow(unsafe_code)]
mod memory;
mod numeric;
mod optimizer;
mod parser;
mod pattern;
mod table;
mod value;
mod vm;

#[doc(inline)]
pub use bytecode::Program;
#[doc(inline)]
pub use cache::{CacheStatus, CachedProgram, ProgramCache};
#[doc(inline)]
pub use error::{Error, ErrorKind};
#[doc(inline)]
pub use value::{FunctionId, ParserPrototypeId, StringId, TableId, ThreadId, UserdataId, Value};
#[doc(inline)]
pub use vm::{
    GcOperation, HeapStats, HostCall, HostFunction, HostThreadState, MemoryUsage,
    NativeBuiltinBody, NativeCallStatus, NativeDebugFrame, NativeDebugInfo, NativeDebugString,
    NativeHook, NativeHookEvent, NativeHookInfo, NativeLoadStatus, NativeStack, NativeThreadStack,
    NativeThreadStatus, Root, Vm,
};

/// Compile Lua 5.1 source or a native chunk into verified, VM-independent bytecode.
///
/// # Errors
/// Returns a syntax or limit error for invalid input. Native chunks must match
/// this target's Lua 5.1 ABI header. Allocation failure returns a memory error.
pub fn compile(source: &[u8]) -> Result<Program, Error> {
    compile_named(source, b"=(deadlib-lua)")
}

/// Compile Lua 5.1 source or a native chunk with a diagnostic chunk name.
///
/// A leading `=` supplies a literal display name; `@` introduces a filename.
/// Other names are displayed as Lua source excerpts.
/// Native chunks retain their embedded source names for execution/debugging;
/// the supplied name identifies loading errors.
///
/// # Errors
/// Returns syntax or limit errors for invalid source, or a memory error if
/// compilation cannot allocate its working storage or the resulting program.
pub fn compile_named(source: &[u8], name: &[u8]) -> Result<Program, Error> {
    if source.first() == Some(&27) {
        return chunk51::compile(source, name, 0);
    }
    compiler::compile(parser::parse(source)?, name)
}
