//! Copyable register values and VM-owned object identities.

/// The identity of an interned byte string in its originating VM.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StringId(pub(crate) u64);

/// The identity of a table in its originating VM.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TableId(pub(crate) u64);

/// The identity of a Lua closure in its originating VM.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FunctionId(pub(crate) u64);

/// The identity of a Lua thread in its originating VM, including the main thread.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThreadId(pub(crate) u64);

/// The identity of full userdata in its originating VM.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UserdataId(pub(crate) u64);

/// A Lua register value. Object handles belong to the VM that produced them.
///
/// Numbers use IEEE 754 binary64, including signed zero, infinities, and NaNs.
/// Copying a value does not keep its object alive; use [`crate::Vm::pin`] for
/// values retained by the host across executions or garbage collections.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// Absence of a value.
    Nil,
    /// A boolean.
    Boolean(bool),
    /// A Lua 5.1 number.
    Number(f64),
    /// An interned, arbitrary byte string.
    String(StringId),
    /// A table identity.
    Table(TableId),
    /// A Lua closure identity.
    Function(FunctionId),
    /// A coroutine or main-thread identity.
    Thread(ThreadId),
    /// A full userdata identity with VM-owned byte storage.
    Userdata(UserdataId),
    /// A native function's numeric dispatch ID.
    Native(u64),
}

impl Value {
    /// Test Lua truthiness; only `nil` and `false` are false.
    pub fn is_truthy(self) -> bool {
        !matches!(self, Self::Nil | Self::Boolean(false))
    }

    /// Return the Lua type name.
    pub fn type_name(self) -> &'static str {
        match self {
            Self::Nil => "nil",
            Self::Boolean(_) => "boolean",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::Table(_) => "table",
            Self::Function(_) | Self::Native(_) => "function",
            Self::Thread(_) => "thread",
            Self::Userdata(_) => "userdata",
        }
    }
}
