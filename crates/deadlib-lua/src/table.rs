//! Array storage and indexed hash entries with deletion-safe linear iteration.

mod native51;
pub(crate) mod sort;
use crate::memory::fallible::SliceExt;
use crate::memory::{Buffer, Memory, Unique};
use crate::{
    Error, ErrorKind, FunctionId, ParserPrototypeId, StringId, TableId, ThreadId, UserdataId, Value,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Clone, Copy, Debug)]
enum Key {
    Vacant,
    Boolean(bool),
    Number(u64),
    String(StringId),
    Table(TableId),
    Function(FunctionId),
    Native(u64),
    Thread(ThreadId),
    Userdata(UserdataId),
    LightUserdata(u64),
    ParserPrototype(ParserPrototypeId),
    DeadString(StringId),
    DeadTable(TableId),
    DeadFunction(FunctionId),
    DeadNative(u64),
    DeadThread(ThreadId),
    DeadUserdata(UserdataId),
    DeadParserPrototype(ParserPrototypeId),
}
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::Vacant, Self::Vacant) => true,
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b || (a << 1 == 0 && b << 1 == 0),
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Table(a), Self::Table(b)) => a == b,
            (Self::Function(a), Self::Function(b)) => a == b,
            (Self::Native(a), Self::Native(b)) => a == b,
            (Self::Thread(a), Self::Thread(b)) => a == b,
            (Self::Userdata(a), Self::Userdata(b)) => a == b,
            (Self::LightUserdata(a), Self::LightUserdata(b)) => a == b,
            (Self::ParserPrototype(a), Self::ParserPrototype(b)) => a == b,
            (Self::DeadString(a), Self::DeadString(b)) => a == b,
            (Self::DeadTable(a), Self::DeadTable(b)) => a == b,
            (Self::DeadFunction(a), Self::DeadFunction(b)) => a == b,
            (Self::DeadNative(a), Self::DeadNative(b)) => a == b,
            (Self::DeadThread(a), Self::DeadThread(b)) => a == b,
            (Self::DeadUserdata(a), Self::DeadUserdata(b)) => a == b,
            (Self::DeadParserPrototype(a), Self::DeadParserPrototype(b)) => a == b,
            _ => false,
        }
    }
}
impl Eq for Key {}

impl Key {
    fn from_value(value: Value) -> Option<Self> {
        Some(match value {
            Value::Nil => return None,
            Value::Boolean(v) => Self::Boolean(v),
            Value::Number(v) if v.is_nan() => return None,
            Value::Number(v) => Self::Number(v.to_bits()),
            Value::String(v) => Self::String(v),
            Value::Table(v) => Self::Table(v),
            Value::Function(v) => Self::Function(v),
            Value::Native(v) => Self::Native(v),
            Value::Thread(v) => Self::Thread(v),
            Value::Userdata(v) => Self::Userdata(v),
            Value::LightUserdata(v) => Self::LightUserdata(v),
            Value::ParserPrototype(v) => Self::ParserPrototype(v),
        })
    }
    fn value(self) -> Value {
        match self {
            Self::Vacant
            | Self::DeadString(_)
            | Self::DeadTable(_)
            | Self::DeadFunction(_)
            | Self::DeadNative(_)
            | Self::DeadThread(_)
            | Self::DeadUserdata(_)
            | Self::DeadParserPrototype(_) => Value::Nil,
            Self::Boolean(v) => Value::Boolean(v),
            Self::Number(v) => Value::Number(f64::from_bits(v)),
            Self::String(v) => Value::String(v),
            Self::Table(v) => Value::Table(v),
            Self::Function(v) => Value::Function(v),
            Self::Native(v) => Value::Native(v),
            Self::Thread(v) => Value::Thread(v),
            Self::Userdata(v) => Value::Userdata(v),
            Self::LightUserdata(v) => Value::LightUserdata(v),
            Self::ParserPrototype(v) => Value::ParserPrototype(v),
        }
    }
    fn identity(self) -> (u64, u64) {
        match self {
            Self::Vacant => (0, 0),
            Self::Boolean(v) => (u64::from(v), 1),
            Self::Number(v) => (if v << 1 == 0 { 0 } else { v }, 2),
            Self::String(v) => (v.0, 3),
            Self::Table(v) => (v.0, 4),
            Self::Function(v) => (v.0, 5),
            Self::Native(v) => (v, 6),
            Self::Thread(v) => (v.0, 7),
            Self::Userdata(v) => (v.0, 8),
            Self::LightUserdata(v) => (v, 9),
            Self::ParserPrototype(v) => (v.0, 10),
            Self::DeadString(v) => (v.0, 0x83),
            Self::DeadTable(v) => (v.0, 0x84),
            Self::DeadFunction(v) => (v.0, 0x85),
            Self::DeadNative(v) => (v, 0x86),
            Self::DeadThread(v) => (v.0, 0x87),
            Self::DeadUserdata(v) => (v.0, 0x88),
            Self::DeadParserPrototype(v) => (v.0, 0x8a),
        }
    }
    fn dead(self) -> Option<Self> {
        Some(match self {
            Self::String(v) => Self::DeadString(v),
            Self::Table(v) => Self::DeadTable(v),
            Self::Function(v) => Self::DeadFunction(v),
            Self::Native(v) => Self::DeadNative(v),
            Self::Thread(v) => Self::DeadThread(v),
            Self::Userdata(v) => Self::DeadUserdata(v),
            Self::ParserPrototype(v) => Self::DeadParserPrototype(v),
            _ => return None,
        })
    }
    fn matches_next(self, key: Self) -> bool {
        self == key || key.dead() == Some(self)
    }
    /// A multiplication carries bits only upward, so the high half is first
    /// folded into the low half: a number's varying bits sit near the top of
    /// its word, and without the fold small integers and simple fractions all
    /// leave the same low bits. The product's upper half, folded down again,
    /// then depends on every identity bit. Slot order is never observable:
    /// traversal follows insertion order.
    #[inline]
    fn hash(self) -> usize {
        let (bits, tag) = self.identity();
        let bits = bits ^ tag.rotate_left(59);
        let h = (bits ^ (bits >> 32)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        (h ^ (h >> 32)) as usize
    }
}

pub(crate) fn array_index(value: Value) -> Option<usize> {
    let Value::Number(n) = value else {
        return None;
    };
    let index = n as u32;
    (index != 0 && f64::from(index) == n).then_some(index.wrapping_sub(1) as usize)
}

fn array_range(first: i64, last: i64) -> Option<std::ops::Range<usize>> {
    let start = u32::try_from(first).ok()?.checked_sub(1)? as usize;
    let end = u32::try_from(last).ok()? as usize;
    Some(start..end)
}

fn array_scalar(value: Value) -> bool {
    matches!(
        value,
        Value::Nil | Value::Boolean(_) | Value::Number(_) | Value::LightUserdata(_)
    )
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    key: Key,
    value: Value,
    // Lua 5.1 collision chains are separate from the identity lookup index.
    native_hash: u32,
    next: u32,
}

// A slot word holds a one-based entry index. While the slot array is small
// enough for that index to fit in `INDEX_BITS`, the bits above it hold part of
// the key's hash, so a probe passes most other keys without loading their
// entries. Larger tables use the whole word for the index.
const INDEX_BITS: u32 = if cfg!(test) { 10 } else { 24 };
const INDEX_MASK: u32 = (1 << INDEX_BITS) - 1;
const TAGGED_SLOTS: usize = 1 << INDEX_BITS;

pub(crate) const ARRAY_WATCH_BLOCK: usize = 32;
pub(crate) const ARRAY_WATCH_LIMIT: usize = (u8::MAX as usize - 1) * ARRAY_WATCH_BLOCK;
pub(crate) const ARRAY_WATCH_BLOCKS: usize = 2;
pub(crate) const LUA51_MAX_ARRAY: usize = 1 << 26;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArrayChange {
    Unwatched,
    Clean,
    Dirty {
        blocks: [usize; ARRAY_WATCH_BLOCKS],
        len: usize,
    },
}

#[derive(Debug)]
pub(crate) struct InvalidNextKey;

enum ArrayStorage {
    Heap {
        values: Buffer<Value>,
        // Every slot equals its one-based index. Any writable view clears it.
        is_sequence: bool,
    },
    // A dense native triplet needs no separate allocation for its values.
    Triplet {
        memory: Memory,
        values: [Value; 3],
    },
    SharedScalars {
        memory: Memory,
        values: &'static [Value],
        sorted: bool,
    },
}

// Bound the process-wide backing to 64 KiB.
const SEQUENCE_LIMIT: usize = 64 * 1024 / size_of::<Value>();
static ARRAY_SEQUENCE: [Value; SEQUENCE_LIMIT] = {
    let mut values = [Value::Nil; SEQUENCE_LIMIT];
    let mut index = 0;
    while index < values.len() {
        values[index] = Value::Number((index + 1) as f64);
        index += 1;
    }
    values
};

impl std::fmt::Debug for ArrayStorage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&**self, formatter)
    }
}

impl Default for ArrayStorage {
    fn default() -> Self {
        Self::heap(Buffer::default())
    }
}

impl ArrayStorage {
    fn heap(values: Buffer<Value>) -> Self {
        Self::Heap {
            values,
            is_sequence: false,
        }
    }

    fn memory(&self) -> &Memory {
        match self {
            Self::Heap { values, .. } => values.memory(),
            Self::Triplet { memory, .. } | Self::SharedScalars { memory, .. } => memory,
        }
    }

    fn heap_mut(&mut self) -> &mut Buffer<Value> {
        let Self::Heap {
            values,
            is_sequence,
        } = self
        else {
            unreachable!("array must have writable heap storage");
        };
        *is_sequence = false;
        values
    }

    fn allocated_capacity(&self) -> usize {
        match self {
            Self::Heap { values, .. } => values.capacity(),
            Self::Triplet { .. } | Self::SharedScalars { .. } => 0,
        }
    }

    fn try_make_writable(&mut self, capacity: usize) -> Result<(), Error> {
        if matches!(self, Self::SharedScalars { .. }) {
            return self.copy_shared_scalars(capacity);
        }
        Ok(())
    }

    #[cold]
    #[inline(never)]
    fn copy_shared_scalars(&mut self, capacity: usize) -> Result<(), Error> {
        let Self::SharedScalars { memory, values, .. } = self else {
            unreachable!("only shared arrays require a copy");
        };
        let mut copy = Buffer::new_in(memory.clone());
        copy.try_reserve_exact(values.len().max(capacity))?;
        copy.extend_from_slice(values);
        *self = Self::heap(copy);
        Ok(())
    }
}

impl std::ops::Deref for ArrayStorage {
    type Target = [Value];

    fn deref(&self) -> &[Value] {
        match self {
            Self::Heap { values, .. } => values,
            Self::Triplet { values, .. } => values,
            Self::SharedScalars { values, .. } => values,
        }
    }
}

impl std::ops::DerefMut for ArrayStorage {
    fn deref_mut(&mut self) -> &mut [Value] {
        match self {
            Self::Heap {
                values,
                is_sequence,
            } => {
                *is_sequence = false;
                values
            }
            Self::Triplet { values, .. } => values,
            Self::SharedScalars { .. } => unreachable!("array must be made writable first"),
        }
    }
}

#[derive(Debug)]
struct UnpublishedRows {
    key: StringId,
    root: TableId,
    observed: AtomicBool,
}

impl UnpublishedRows {
    fn unread(&self) -> bool {
        !self.observed.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum NumberRows {
    Triplets,
    Nested,
}

#[derive(Debug, Default)]
struct UnpublishedFields {
    triplets: Option<UnpublishedRows>,
    nested: Option<UnpublishedRows>,
}

#[derive(Debug)]
struct HashPart {
    entries: Buffer<Entry>,
    slots: Buffer<u32>,
    native_dead_keys: u32,
    last_free: u32,
    // In ordinary tables, a positive integer in the hash can migrate during
    // an array append. Deletions leave this hint conservative until rebuild.
    may_have_array_keys: bool,
    // Two one-based indices. Keys are checked before using either hint.
    next_hash_hint: AtomicU64,
    unpublished_rows: Option<Unique<UnpublishedFields>>,
}

impl HashPart {
    #[inline]
    fn needs_rebuild(&self, entries: usize) -> bool {
        entries * 4 >= self.slots.len() * 3
    }

    fn array_key(entry: &Entry) -> bool {
        matches!(entry.key, Key::Number(bits)
            if array_index(Value::Number(f64::from_bits(bits))).is_some())
    }

    #[inline]
    fn read_value(&self, entry: &Entry) -> Value {
        if let Value::Table(root) = entry.value
            && let Some(fields) = &self.unpublished_rows
            && let Key::String(key) = entry.key
        {
            for watch in [fields.triplets.as_ref(), fields.nested.as_ref()]
                .into_iter()
                .flatten()
            {
                if key == watch.key && root == watch.root {
                    watch.observed.store(true, Ordering::Relaxed);
                }
            }
        }
        entry.value
    }
}

static EMPTY_HASH: HashPart = HashPart {
    entries: Buffer::empty_global(),
    slots: Buffer::empty_global(),
    native_dead_keys: 0,
    last_free: 0,
    may_have_array_keys: false,
    next_hash_hint: AtomicU64::new(0),
    unpublished_rows: None,
};

/// The physical table layout at a parser replay boundary. The VM validates
/// string identities before publishing it in another table.
#[derive(Debug)]
pub(crate) struct ParserTableLayout {
    array: Box<[Value]>,
    entries: Box<[Entry]>,
    slots: Box<[u32]>,
    last_free: u32,
    fields_version: u32,
}

impl ParserTableLayout {
    pub(crate) fn heap_bytes(&self) -> usize {
        std::mem::size_of_val(&*self.array)
            + std::mem::size_of_val(&*self.entries)
            + std::mem::size_of_val(&*self.slots)
    }

    pub(crate) fn string_ids(&self) -> impl Iterator<Item = StringId> + Clone + '_ {
        self.entries.iter().filter_map(|entry| match entry.key {
            Key::String(id) => Some(id),
            _ => None,
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct Table {
    array: ArrayStorage,
    hash: Option<Unique<HashPart>>,
    native51: bool,
    // Arena identities have a nonzero generation; zero denotes no metatable.
    // The private attachment below uses the same optional-identity encoding.
    metatable: u64,
    // Private embedding state, traced independently of Lua weak-table modes.
    host_state: u64,
    // Counts changes to keyed entries; see `fields_version`.
    fields_version: u32,
    // A raw absence can only be invalidated by making a string key live.
    string_keys_added: u32,
    // Zero invalidates the watch, one is clean, and higher values identify
    // the first dirty block plus two.
    array_watch: u8,
    pub trace: TableTrace,
    // The second block is meaningful only while the first block is dirty.
    array_watch_second: u8,
    // A reader replay can restore this after applying its own events.
    // Mutations between reader boundaries clear it.
    parser_pristine: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TableTrace {
    #[default]
    Idle,
    Pending(bool, bool),
    Restart,
}

impl Table {
    pub(crate) fn new_in(memory: Memory) -> Self {
        Self {
            array: ArrayStorage::heap(Buffer::new_in(memory)),
            hash: None,
            native51: false,
            metatable: 0,
            host_state: 0,
            fields_version: 0,
            string_keys_added: 0,
            array_watch: 0,
            trace: TableTrace::Idle,
            array_watch_second: 0,
            parser_pristine: false,
        }
    }
    #[cfg(test)]
    pub fn with_capacity(array: usize, records: usize) -> Result<Self, Error> {
        Self::with_capacity_in(array, records, Memory::default())
    }
    pub fn with_capacity_in(array: usize, records: usize, memory: Memory) -> Result<Self, Error> {
        let failed = Error::memory;
        let mut table = Self::new_in(memory);
        table
            .array
            .heap_mut()
            .try_reserve_exact(array)
            .map_err(|_| failed())?;
        table.array.heap_mut().resize(array, Value::Nil);
        if records > (1 << 26) {
            return Err(Error::new(ErrorKind::Runtime, 0, "table overflow"));
        }
        let slots = if records == 0 {
            0
        } else {
            records
                .checked_mul(4)
                .and_then(|n| (n / 3 + 1).max(8).checked_next_power_of_two())
                .ok_or_else(failed)?
        };
        if records != 0 {
            let part = table.ensure_hash_part()?;
            part.entries
                .try_reserve_exact(records)
                .map_err(|_| failed())?;
            part.slots.try_reserve_exact(slots).map_err(|_| failed())?;
            part.slots.resize(slots, 0);
        }
        Ok(table)
    }
    pub(crate) fn with_scalar_array(
        scalars: &[Value],
        array: usize,
        native51: bool,
        memory: Memory,
    ) -> Result<Self, Error> {
        let mut table = Self::new_in(memory);
        let slots = table.array.heap_mut();
        slots.try_reserve_exact(array.max(scalars.len()))?;
        slots.extend_from_slice(scalars);
        slots.resize(array.max(scalars.len()), Value::Nil);
        table.native51 = native51;
        table.fields_version = u32::from(native51);
        Ok(table)
    }
    /// Copy the storage of a fresh table built entirely from bound constants.
    pub(crate) fn copy_literal(&self) -> Result<Self, Error> {
        fn copy<T: Copy>(values: &[T], memory: &Memory) -> Result<Buffer<T>, Error> {
            let mut buffer = Buffer::new_in(memory.clone());
            buffer.try_reserve_exact(values.len())?;
            buffer.extend_from_slice(values);
            Ok(buffer)
        }

        debug_assert!(self.metatable().is_none() && self.host_state().is_none());
        let memory = self.array.memory();
        let array = ArrayStorage::heap(copy(&self.array, memory)?);
        let hash = if let Some(part) = &self.hash {
            debug_assert!(part.unpublished_rows.is_none());
            Some(Unique::try_new_in(
                HashPart {
                    entries: copy(&part.entries, memory)?,
                    slots: copy(&part.slots, memory)?,
                    native_dead_keys: part.native_dead_keys,
                    last_free: part.last_free,
                    may_have_array_keys: part.may_have_array_keys,
                    next_hash_hint: AtomicU64::new(part.next_hash_hint.load(Ordering::Relaxed)),
                    unpublished_rows: None,
                },
                memory.clone(),
            )?)
        } else {
            None
        };
        Ok(Self {
            array,
            hash,
            native51: self.native51,
            fields_version: self.fields_version,
            string_keys_added: self.string_keys_added,
            ..Self::default()
        })
    }

    pub(crate) fn from_literal_array(values: Buffer<Value>, native51: bool) -> Self {
        Self {
            array: ArrayStorage::heap(values),
            native51,
            fields_version: u32::from(native51),
            ..Self::default()
        }
    }

    pub(crate) fn with_dense_sequence(len: usize, native51: bool, memory: Memory) -> Option<Self> {
        Some(Self {
            array: ArrayStorage::SharedScalars {
                memory,
                values: ARRAY_SEQUENCE.get(..len)?,
                sorted: true,
            },
            native51,
            fields_version: u32::from(native51),
            ..Self::default()
        })
    }
    // Fresh native tables retain distinct identities while their common
    // scalar values can share immutable storage until the first write.
    pub(crate) fn with_shared_number_array(numbers: &[f64], memory: &Memory) -> Option<Self> {
        const ZERO: u64 = 0.0f64.to_bits();
        const ONE: u64 = 1.0f64.to_bits();
        static ZEROS: [Value; 4] = [Value::Number(0.); 4];
        static ONES: [Value; 4] = [Value::Number(1.); 4];
        static QUATERNION: [Value; 4] = [ZEROS[0], ZEROS[0], ZEROS[0], ONES[0]];
        static TRANSPARENT_WHITE: [Value; 4] = [ONES[0], ONES[0], ONES[0], ZEROS[0]];
        let numbers: &[f64; 4] = numbers.try_into().ok()?;
        let (values, sorted) = match numbers.map(f64::to_bits) {
            [ZERO, ZERO, ZERO, ZERO] => (&ZEROS, true),
            [ONE, ONE, ONE, ONE] => (&ONES, true),
            [ZERO, ZERO, ZERO, ONE] => (&QUATERNION, true),
            [ONE, ONE, ONE, ZERO] => (&TRANSPARENT_WHITE, false),
            _ => return None,
        };
        Some(Self {
            array: ArrayStorage::SharedScalars {
                memory: memory.clone(),
                values,
                sorted,
            },
            native51: true,
            fields_version: 1,
            ..Self::default()
        })
    }

    #[inline]
    fn hash_part(&self) -> &HashPart {
        self.hash.as_deref().unwrap_or(&EMPTY_HASH)
    }

    #[inline]
    fn hash_part_mut(&mut self) -> &mut HashPart {
        self.hash.as_deref_mut().expect("keyed table storage")
    }

    fn ensure_hash_part(&mut self) -> Result<&mut HashPart, Error> {
        if self.hash.is_none() {
            let memory = self.array.memory().clone();
            let part = HashPart {
                entries: Buffer::new_in(memory.clone()),
                slots: Buffer::new_in(memory.clone()),
                native_dead_keys: 0,
                last_free: 0,
                may_have_array_keys: false,
                next_hash_hint: AtomicU64::new(0),
                unpublished_rows: None,
            };
            self.hash = Some(Unique::try_new_in(part, memory)?);
        }
        Ok(self.hash_part_mut())
    }

    // Only the VM's fresh number-row constructor may register a root. A
    // Lua-visible read freezes that graph even when the script drops its alias.
    pub(crate) fn register_unpublished_rows(
        &mut self,
        key: StringId,
        root: TableId,
        kind: NumberRows,
    ) -> Result<(), Error> {
        let watch = UnpublishedRows {
            key,
            root,
            observed: AtomicBool::new(false),
        };
        let memory = self.array.memory().clone();
        let part = self.hash_part_mut();
        if part.unpublished_rows.is_none() {
            part.unpublished_rows = Some(Unique::try_new_in(UnpublishedFields::default(), memory)?);
        }
        let fields = part.unpublished_rows.as_mut().unwrap();
        match kind {
            NumberRows::Triplets => fields.triplets = Some(watch),
            NumberRows::Nested => fields.nested = Some(watch),
        }
        Ok(())
    }

    pub(crate) fn unpublished_rows(&self, key: StringId, kind: NumberRows) -> Option<TableId> {
        let fields = self.hash.as_ref()?.unpublished_rows.as_ref()?;
        let watch = match kind {
            NumberRows::Triplets => fields.triplets.as_ref(),
            NumberRows::Nested => fields.nested.as_ref(),
        }?;
        if key != watch.key || !watch.unread() || self.metatable().is_some() {
            return None;
        }
        let (_, index) = self.slot(Key::String(key))?;
        let entry = &self.hash_part().entries[index?];
        (entry.value == Value::Table(watch.root)).then_some(watch.root)
    }

    pub fn metatable(&self) -> Option<TableId> {
        (self.metatable != 0).then_some(TableId(self.metatable))
    }
    pub fn set_metatable(&mut self, table: Option<TableId>) {
        self.parser_pristine = false;
        self.metatable = table.map_or(0, |id| id.0);
    }
    pub fn host_state(&self) -> Option<TableId> {
        (self.host_state != 0).then_some(TableId(self.host_state))
    }
    pub fn set_host_state(&mut self, table: Option<TableId>) {
        self.parser_pristine = false;
        self.host_state = table.map_or(0, |id| id.0);
    }
    /// A counter that changes whenever an entry outside the array part is
    /// added, replaced or removed. While it is unchanged, a raw read of any
    /// string key returns what it returned before.
    #[inline]
    pub fn fields_version(&self) -> u32 {
        self.fields_version
    }
    pub(crate) fn string_keys_added(&self) -> u32 {
        self.string_keys_added
    }
    #[inline]
    fn note_string_key_added(&mut self) {
        self.string_keys_added = self.string_keys_added.saturating_add(1);
    }
    #[inline]
    fn touch_fields(&mut self) {
        self.advance_fields_version(1);
    }
    pub(crate) fn advance_fields_version(&mut self, writes: u32) {
        self.parser_pristine = false;
        self.fields_version = self.fields_version.wrapping_add(writes);
    }
    pub(crate) fn array_change(&self) -> ArrayChange {
        match self.array_watch {
            0 => ArrayChange::Unwatched,
            1 => ArrayChange::Clean,
            first => {
                let second = self.array_watch_second;
                ArrayChange::Dirty {
                    blocks: [
                        usize::from(first - 2),
                        usize::from(second.saturating_sub(2)),
                    ],
                    len: 1 + usize::from(second != 0),
                }
            }
        }
    }
    pub(crate) fn watch_array(&mut self) {
        self.array_watch = 1;
        self.array_watch_second = 0;
    }
    #[inline]
    fn touch_array(&mut self) {
        self.parser_pristine = false;
        self.array_watch = 0;
    }
    #[inline]
    fn touch_array_at(&mut self, index: usize) {
        self.parser_pristine = false;
        let first = self.array_watch;
        if first != 0 {
            let Ok(block) = u8::try_from(index / ARRAY_WATCH_BLOCK + 2) else {
                self.touch_array();
                return;
            };
            if first == 1 {
                self.array_watch = block;
            } else if first != block {
                let second = self.array_watch_second;
                if second == 0 {
                    self.array_watch_second = block;
                } else if second != block {
                    self.touch_array();
                }
            }
        }
    }
    pub fn heap_bytes(&self) -> usize {
        self.array.allocated_capacity() * size_of::<Value>()
            + self.hash.as_ref().map_or(0, |part| {
                part.heap_bytes()
                    + part.entries.capacity() * size_of::<Entry>()
                    + part.slots.capacity() * size_of::<u32>()
                    + usize::from(part.unpublished_rows.is_some()) * size_of::<UnpublishedFields>()
            })
    }
    pub(crate) fn parser_pristine(&self) -> bool {
        self.parser_pristine
    }

    pub(crate) fn mark_parser_pristine(&mut self) {
        self.parser_pristine = true;
    }

    pub(crate) fn clear_parser_pristine(&mut self) {
        self.parser_pristine = false;
    }

    pub(crate) fn capture_parser_layout(&self, limit: usize) -> Option<ParserTableLayout> {
        let part = self.hash_part();
        let bytes =
            size_of_val(&*self.array) + size_of_val(&*part.entries) + size_of_val(&*part.slots);
        if bytes > limit
            || !self.array.iter().copied().all(array_scalar)
            || !part.entries.iter().all(|entry| {
                matches!(
                    entry.key,
                    Key::Vacant | Key::Boolean(_) | Key::Number(_) | Key::String(_)
                ) && array_scalar(entry.value)
            })
        {
            return None;
        }
        Some(ParserTableLayout {
            array: self.array.try_boxed().ok()?,
            entries: part.entries.try_boxed().ok()?,
            slots: part.slots.try_boxed().ok()?,
            last_free: part.last_free,
            fields_version: self.fields_version,
        })
    }

    pub(crate) fn install_parser_layout(
        &mut self,
        layout: &ParserTableLayout,
    ) -> Result<(), Error> {
        debug_assert!(self.parser_pristine);
        // If a reservation fails, the original contents remain available
        // for ordinary replay. Commit only after all storage is ready.
        let array = self.array.heap_mut();
        array.try_reserve(layout.array.len().saturating_sub(array.len()))?;
        if !layout.slots.is_empty() {
            let part = self.ensure_hash_part()?;
            part.entries
                .try_reserve(layout.entries.len().saturating_sub(part.entries.len()))?;
            part.slots
                .try_reserve(layout.slots.len().saturating_sub(part.slots.len()))?;
            part.entries.clear();
            part.entries.extend_from_slice(&layout.entries);
            part.may_have_array_keys = part.entries.iter().any(HashPart::array_key);
            part.slots.clear();
            part.slots.extend_from_slice(&layout.slots);
            part.last_free = layout.last_free;
            part.native_dead_keys = 0;
            part.next_hash_hint.store(0, Ordering::Relaxed);
        } else {
            self.hash = None;
        }
        let array = self.array.heap_mut();
        array.clear();
        array.extend_from_slice(&layout.array);
        self.fields_version = layout.fields_version;
        self.note_string_key_added();
        self.invalidate_trace();
        self.touch_array();
        self.mark_parser_pristine();
        Ok(())
    }
    pub fn pairs(&self) -> impl Iterator<Item = (Value, Value)> + '_ {
        self.array
            .iter()
            .enumerate()
            .filter(|(_, v)| **v != Value::Nil)
            .map(|(i, v)| (Value::Number((i + 1) as f64), *v))
            .chain(
                self.hash_part()
                    .entries
                    .iter()
                    .filter(|e| e.value != Value::Nil)
                    .map(|e| (e.key.value(), e.value)),
            )
    }
    /// Return array values and live hash pairs with their offsets within the slice.
    /// Holes and tombstones still count toward the budget.
    /// The low cursor bit selects array/hash storage; the other bits are an index.
    pub fn trace_slice(
        &self,
        cursor: usize,
        budget: usize,
    ) -> (
        Option<usize>,
        &[Value],
        impl Iterator<Item = (usize, Value, Value)> + '_,
    ) {
        let start = cursor >> 1;
        let (array, entries, next) = if cursor & 1 == 0 {
            let end = start.saturating_add(budget).min(self.array.len());
            let hash_end = if end == self.array.len() {
                budget
                    .saturating_sub(end - start)
                    .min(self.hash_part().entries.len())
            } else {
                0
            };
            let next = if end < self.array.len() {
                Some(end << 1)
            } else {
                (hash_end < self.hash_part().entries.len()).then_some((hash_end << 1) | 1)
            };
            (
                &self.array[start..end],
                &self.hash_part().entries[..hash_end],
                next,
            )
        } else {
            let end = start
                .saturating_add(budget)
                .min(self.hash_part().entries.len());
            (
                &self.array[..0],
                &self.hash_part().entries[start..end],
                (end < self.hash_part().entries.len()).then_some((end << 1) | 1),
            )
        };
        (
            next,
            array,
            entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.value != Value::Nil)
                .map(|(offset, e)| (offset, e.key.value(), e.value)),
        )
    }
    fn invalidate_trace(&mut self) {
        if self.trace != TableTrace::Idle {
            self.trace = TableTrace::Restart;
        }
    }
    // The tag an entry of this hash carries in its slot word, and the mask
    // that selects the entry index from a word.
    #[inline]
    fn slot_tag(slots: usize, hash: usize) -> (u32, u32) {
        if slots <= TAGGED_SLOTS {
            // Slot selection uses fewer than `INDEX_BITS` low hash bits here,
            // so the bits from `INDEX_BITS` up are independent of the slot.
            (((hash >> INDEX_BITS) as u32) << INDEX_BITS, INDEX_MASK)
        } else {
            (0, u32::MAX)
        }
    }
    fn slot(&self, key: Key) -> Option<(usize, Option<usize>)> {
        if self.hash_part().slots.is_empty() {
            return None;
        }
        self.slot_hashed(key, key.hash())
    }
    fn slot_hashed(&self, key: Key, hash: usize) -> Option<(usize, Option<usize>)> {
        let Some(part) = &self.hash else {
            return None;
        };
        let mask = part.slots.len().checked_sub(1)?;
        let (tag, index) = Self::slot_tag(part.slots.len(), hash);
        let mut slot = hash & mask;
        loop {
            let word = part.slots[slot];
            if word == 0 {
                return Some((slot, None));
            }
            if word & !index == tag {
                let entry = (word & index) as usize - 1;
                if part.entries[entry].key == key {
                    return Some((slot, Some(entry)));
                }
            }
            slot = (slot + 1) & mask;
        }
    }
    // All storage was reserved before tombstones or traversal positions change.
    fn rebuild(&mut self, slots: usize, live: usize) {
        if live < self.hash_part().entries.len() {
            self.hash_part_mut()
                .entries
                .retain(|entry| entry.value != Value::Nil);
            self.invalidate_trace();
        }
        let part = self.hash_part_mut();
        part.slots.resize(slots, 0);
        part.slots.fill(0);
        part.may_have_array_keys = false;
        for (i, entry) in part.entries.iter().enumerate() {
            part.may_have_array_keys |= HashPart::array_key(entry);
            let hash = entry.key.hash();
            let mut slot = hash & (part.slots.len() - 1);
            while part.slots[slot] != 0 {
                slot = (slot + 1) & (part.slots.len() - 1);
            }
            let index = u32::try_from(i + 1).expect("Lua table exhausted addressable entries");
            part.slots[slot] = index | Self::slot_tag(slots, hash).0;
        }
    }
    pub(crate) fn is_scalar_array_only(&self) -> bool {
        self.hash_part().entries.is_empty()
            && (matches!(self.array, ArrayStorage::SharedScalars { .. })
                || (self.array.len() <= 4 && self.array.iter().copied().all(array_scalar)))
    }

    pub(crate) fn array_values(&self) -> &[Value] {
        &self.array
    }

    // Callers only use this with fresh or unread scalar arrays.
    pub(crate) fn initialize_number_array(&mut self, values: &[f64]) {
        debug_assert_eq!(self.array.len(), values.len());
        for (slot, &number) in self.array.iter_mut().zip(values) {
            *slot = Value::Number(number);
        }
    }

    // The caller proved that Lua has never received the row or its owner array.
    pub(crate) fn overwrite_private_triplet(&mut self, numbers: [f64; 3]) -> bool {
        let ArrayStorage::Triplet { values, .. } = &mut self.array else {
            return false;
        };
        *values = numbers.map(Value::Number);
        true
    }

    fn take_hash(&mut self, key: Key) -> Option<Value> {
        let (_, Some(i)) = self.slot(key)? else {
            return None;
        };
        let old = std::mem::replace(&mut self.hash_part_mut().entries[i].value, Value::Nil);
        if old != Value::Nil {
            self.invalidate_trace();
            self.touch_fields();
        }
        (old != Value::Nil).then_some(old)
    }
    // Equal finite numbers have identical bits after excluding negative zero,
    // so an unstable numeric sort cannot expose a changed tie permutation.
    pub(crate) fn scalar_sort_values(&mut self, len: usize, numbers: bool) -> Option<&mut [Value]> {
        if self.metatable().is_some() || matches!(self.array, ArrayStorage::SharedScalars { .. }) {
            return None;
        }
        let values = self.array.get(..len)?;
        if !values.iter().all(|value| match value {
            Value::Number(_) => true,
            Value::Boolean(_) | Value::Nil => !numbers,
            _ => false,
        }) {
            return None;
        }
        self.touch_array();
        Some(&mut self.array[..len])
    }
    pub(crate) fn sort_finite_numbers(&mut self, len: usize) -> bool {
        if let ArrayStorage::SharedScalars { values, sorted, .. } = &self.array {
            return *sorted && len <= values.len();
        }
        let Some(values) = self.array.get(..len) else {
            return false;
        };
        if !values.iter().all(|value| {
            matches!(value, Value::Number(n)
            if n.is_finite() && (*n != 0.0 || !n.is_sign_negative()))
        }) {
            return false;
        }
        self.touch_array();
        self.array[..len].sort_unstable_by(|left, right| {
            let (Value::Number(a), Value::Number(b)) = (*left, *right) else {
                unreachable!("validated numeric array")
            };
            a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
        });
        true
    }
    pub fn get(&self, key: Value) -> Value {
        if let Some(i) = array_index(key)
            && let Some(value) = self.array.get(i)
        {
            return *value;
        }
        let Some(key) = Key::from_value(key) else {
            return Value::Nil;
        };
        self.get_key(key)
    }
    #[inline]
    pub(crate) fn get_integer(&self, index: i64) -> Value {
        // Array keys use the same one-based u32 range as array_index.
        let offset = (index as u64).wrapping_sub(1);
        if offset < u64::from(u32::MAX)
            && let Some(value) = self.array.get(offset as usize)
        {
            return *value;
        }
        self.get_key(Key::Number((index as f64).to_bits()))
    }
    /// Return the inclusive range of one-based indices when it fits in the array.
    pub(crate) fn array_slice(&self, first: i64, last: i64) -> Option<&[Value]> {
        self.array.get(array_range(first, last)?)
    }

    pub(crate) fn clear_array_slots(&mut self, start: usize, mut mask: u64) {
        self.touch_array();
        if mask == u64::MAX {
            self.array[start..start + 64].fill(Value::Nil);
            return;
        }
        while mask != 0 {
            let index = start + mask.trailing_zeros() as usize;
            self.array[index] = Value::Nil;
            mask &= mask - 1;
        }
    }
    /// Insert at the start of a scalar array range or remove its first value.
    /// Existing scalar slots can be overwritten without allocation or GC work.
    pub(crate) fn shift_scalar_array(
        &mut self,
        first: i64,
        last: i64,
        insert: Option<Value>,
    ) -> Option<Value> {
        if matches!(self.array, ArrayStorage::SharedScalars { .. }) {
            return None;
        }
        let array = self.array.get_mut(array_range(first, last)?)?;
        let previous = *array.first()?;
        if insert.is_some_and(|value| !array_scalar(value))
            || !array.iter().copied().all(array_scalar)
        {
            return None;
        }
        let end = array.len() - 1;
        if let Some(value) = insert {
            array.copy_within(..end, 1);
            array[0] = value;
        } else {
            array.copy_within(1.., 0);
            array[end] = Value::Nil;
        }
        self.touch_array();
        Some(previous)
    }
    /// String keys are already interned; field access need not classify them
    /// as a possible array index or reconstruct a generic Lua table key.
    #[inline]
    pub(crate) fn get_string(&self, key: StringId) -> Value {
        self.get_key(Key::String(key))
    }

    /// A remembered entry is only a position hint. Check the key again and
    /// load the current value so overwrites, removal and compaction remain visible.
    #[inline]
    pub(crate) fn get_string_cached(&self, key: StringId, hint: &mut u32) -> Value {
        let part = self.hash_part();
        if part.entries.is_empty() {
            return Value::Nil;
        }
        if let Some(entry) = part.entries.get(hint.wrapping_sub(1) as usize)
            && matches!(entry.key, Key::String(id) if id == key)
            && entry.value != Value::Nil
        {
            return part.read_value(entry);
        }
        self.find_string_entry(Key::String(key), hint)
    }

    #[cold]
    fn find_string_entry(&self, key: Key, hint: &mut u32) -> Value {
        if let Some((_, Some(index))) = self.slot(key) {
            let part = self.hash_part();
            let value = part.read_value(&part.entries[index]);
            if value != Value::Nil {
                *hint = (index + 1) as u32;
            }
            value
        } else {
            Value::Nil
        }
    }
    #[inline]
    fn get_key(&self, key: Key) -> Value {
        let Some(part) = &self.hash else {
            return Value::Nil;
        };
        if part.slots.is_empty() {
            return Value::Nil;
        }
        let mask = part.slots.len() - 1;
        let hash = key.hash();
        let (tag, index) = Self::slot_tag(part.slots.len(), hash);
        let mut slot = hash & mask;
        loop {
            let word = part.slots[slot];
            if word == 0 {
                return Value::Nil;
            }
            if word & !index == tag {
                let entry = &part.entries[(word & index) as usize - 1];
                if entry.key == key {
                    return part.read_value(entry);
                }
            }
            slot = (slot + 1) & mask;
        }
    }
    fn checked_key(key: Value) -> Result<Key, Error> {
        Key::from_value(key).ok_or_else(|| {
            Error::new(
                ErrorKind::Runtime,
                0,
                if key == Value::Nil {
                    "table index is nil"
                } else {
                    "table index is NaN"
                },
            )
        })
    }
    pub fn validate_set_key(key: Value) -> Result<(), Error> {
        Self::checked_key(key).map(|_| ())
    }
    /// Write a scalar to the array without allocation or new collector edges.
    #[inline]
    pub(crate) fn set_array_scalar_in_place(&mut self, key: Value, value: Value) -> bool {
        if !array_scalar(value) {
            return false;
        }
        let Some(index) = array_index(key) else {
            return false;
        };
        self.set_array_scalar_at(index, value)
    }

    #[inline]
    fn set_array_scalar_at(&mut self, index: usize, value: Value) -> bool {
        if self.replace_array_value(index, value) {
            true
        } else if index == self.array.len()
            && index < self.array.allocated_capacity()
            && !self.native51
            && !self.hash_part().may_have_array_keys
            && value != Value::Nil
        {
            self.array.heap_mut().push(value);
            self.touch_array();
            true
        } else {
            false
        }
    }

    /// Write a non-reference key and value once the caller resolves metamethods.
    #[inline]
    pub(crate) fn set_scalar_in_place(&mut self, key: Value, value: Value) -> bool {
        if !array_scalar(value) {
            return false;
        }
        let array = array_index(key);
        if let Some(index) = array {
            if self.set_array_scalar_at(index, value) {
                return true;
            }
            // Growing or materializing the array can move existing hash keys.
            if index <= self.array.len() {
                return false;
            }
        }
        if self.native51 {
            return false;
        }
        let key = match key {
            Value::Number(n) if !n.is_nan() => Key::Number(n.to_bits()),
            Value::Boolean(b) => Key::Boolean(b),
            Value::LightUserdata(p) => Key::LightUserdata(p),
            _ => return false,
        };
        self.set_reserved_scalar_hash(key, value, array.is_some())
    }

    #[inline(never)]
    fn set_reserved_scalar_hash(&mut self, key: Key, value: Value, array_key: bool) -> bool {
        let hash = key.hash();
        let Some((slot, entry)) = self.slot_hashed(key, hash) else {
            return value == Value::Nil;
        };
        if let Some(entry) = entry {
            self.hash_part_mut().entries[entry].value = value;
            self.touch_fields();
            return true;
        }
        if value == Value::Nil {
            return true;
        }
        let part = self.hash_part_mut();
        let next = part.entries.len() + 1;
        if next > part.entries.capacity() || part.needs_rebuild(next) {
            return false;
        }
        let Ok(index) = u32::try_from(next) else {
            return false;
        };
        part.may_have_array_keys |= array_key;
        self.append_hash_entry(key, value, slot, index, hash);
        true
    }
    pub(crate) fn can_replace_array_value(&self, index: usize) -> bool {
        !matches!(self.array, ArrayStorage::SharedScalars { .. }) && index < self.array.len()
    }

    #[inline]
    pub(crate) fn replace_array_value(&mut self, index: usize, value: Value) -> bool {
        if matches!(self.array, ArrayStorage::SharedScalars { .. }) {
            return matches!(
                (self.array.get(index), value),
                (Some(Value::Number(old)), Value::Number(new))
                    if old.to_bits() == new.to_bits()
            );
        }
        let Some(slot) = self.array.get_mut(index) else {
            return false;
        };
        *slot = value;
        self.touch_array_at(index);
        true
    }
    /// Give a fresh array its own table identity and immutable scalar storage.
    pub(crate) fn share_dense_sequence(&mut self, end: usize) -> bool {
        let Some(values) = ARRAY_SEQUENCE.get(..end) else {
            return false;
        };
        if self.native51
            || self.metatable != 0
            || self.hash.is_some()
            || self.array.allocated_capacity() != 0
            || !self.array.is_empty()
        {
            return false;
        }
        self.array = ArrayStorage::SharedScalars {
            memory: self.array.memory().clone(),
            values,
            sorted: true,
        };
        self.touch_array();
        true
    }
    /// Plan a contiguous write using the same capacities as successive pushes.
    /// End the run before the first allocation that would start a GC step.
    pub(crate) fn dense_write_span(
        &self,
        start: usize,
        end: usize,
        growth_bytes: usize,
    ) -> Option<(usize, usize)> {
        if self.metatable != 0
            || matches!(self.array, ArrayStorage::SharedScalars { .. })
            || start > self.array.len()
        {
            return None;
        }
        let hash = self.hash_part();
        if !hash.slots.is_empty()
            && (self.native51 || (end > self.array.len() && hash.may_have_array_keys))
        {
            // A numeric hash key can migrate into the newly extended array.
            return None;
        }
        let capacity = self.array.allocated_capacity();
        if end <= self.array.len() {
            return Some((end, capacity));
        }
        let max_capacity = capacity.saturating_add(growth_bytes / size_of::<Value>());
        if self.native51 {
            // With no hash part and a filled prefix, each first key outside
            // the array makes the next power-of-two extent over half full.
            // The ordinary rehash would keep every key in the array.
            if self.hash.is_some()
                || !matches!(self.array, ArrayStorage::Heap { .. })
                || self.array[..start].contains(&Value::Nil)
            {
                return None;
            }
            let mut extent = self.array.len();
            while extent < end {
                let next = (extent + 1).next_power_of_two();
                if next > max_capacity.min(LUA51_MAX_ARRAY) {
                    break;
                }
                extent = next;
            }
            return Some((end.min(extent), capacity.max(extent)));
        }
        let mut reserved = capacity;
        while reserved < end {
            let next = reserved.saturating_mul(2).max(4);
            if next > max_capacity {
                break;
            }
            reserved = next;
        }
        Some((end.min(reserved), reserved))
    }
    pub(crate) fn write_dense_span(
        &mut self,
        start: usize,
        end: usize,
        capacity: usize,
        value_at: impl Fn(usize) -> Value,
    ) -> Result<(), Error> {
        if let Some(slots) = self.array.get_mut(start..end) {
            for (offset, slot) in slots.iter_mut().enumerate() {
                *slot = value_at(start + offset);
            }
            self.touch_array();
            return Ok(());
        }
        let old_extent = self.array.len();
        let array = self.array.heap_mut();
        array.try_reserve_exact(capacity - array.len())?;
        let overwrite_end = array.len().min(end);
        for (offset, slot) in array[start..overwrite_end].iter_mut().enumerate() {
            *slot = value_at(start + offset);
        }
        array.extend_with(end, value_at);
        if self.native51 {
            // Native array extent includes the unfilled tail.
            array.resize(capacity, Value::Nil);
            self.invalidate_trace();
            let resizes = old_extent.leading_zeros() - capacity.leading_zeros();
            self.advance_fields_version(resizes);
        }
        self.touch_array();
        Ok(())
    }
    pub(crate) fn write_dense_indices(
        &mut self,
        start: usize,
        end: usize,
        capacity: usize,
    ) -> Result<(), Error> {
        if let ArrayStorage::Heap {
            values,
            is_sequence,
        } = &mut self.array
            && let Some(template) = ARRAY_SEQUENCE.get(start..end)
            && let Some(slots) = values.get_mut(start..end)
        {
            if *is_sequence {
                return Ok(());
            }
            slots.copy_from_slice(template);
            *is_sequence = start == 0 && end == values.len();
            self.touch_array();
            return Ok(());
        }
        self.write_dense_span(start, end, capacity, |slot| {
            Value::Number((slot as i64 + 1) as f64)
        })
    }

    pub(crate) fn dense_write_growth(&self, capacity: usize) -> usize {
        capacity.saturating_sub(self.array.allocated_capacity()) * size_of::<Value>()
    }
    /// A live string-keyed hash entry already owns its key. Replacing its
    /// scalar value cannot allocate, add a GC edge or change traversal order.
    #[inline]
    pub(crate) fn set_existing_string_scalar(&mut self, key: StringId, value: Value) -> bool {
        if !matches!(
            value,
            Value::Boolean(_) | Value::Number(_) | Value::LightUserdata(_)
        ) {
            return false;
        }
        let Some(part) = self.hash.as_deref_mut() else {
            return false;
        };
        let hints = part.next_hash_hint.load(Ordering::Relaxed);
        for hint in [hints as u32, (hints >> 32) as u32] {
            if let Some(entry) = hint
                .checked_sub(1)
                .and_then(|index| part.entries.get_mut(index as usize))
                && matches!(entry.key, Key::String(id) if id == key)
                && entry.value != Value::Nil
            {
                entry.value = value;
                self.touch_fields();
                return true;
            }
        }
        if let Some((_, Some(index))) = self.slot(Key::String(key))
            && self.hash_part().entries[index].value != Value::Nil
        {
            self.hash_part_mut().entries[index].value = value;
            self.touch_fields();
            self.remember_hash_entry((index + 1) as u32);
            return true;
        }
        false
    }

    #[inline]
    // A warm field layout can be used on any matching table. An insertion,
    // deletion or rehash is detected by validating each physical entry.
    pub(crate) fn overwrite_cached_number_prefix(
        &mut self,
        keys: &[StringId],
        numbers: &[f64],
        hints: &[u32],
    ) -> usize {
        let Some(part) = &mut self.hash else {
            return 0;
        };
        let mut written = 0;
        for ((&key, &number), &hint) in keys.iter().zip(numbers).zip(hints) {
            let Some(entry) = part.entries.get_mut(hint.wrapping_sub(1) as usize) else {
                break;
            };
            if entry.key != Key::String(key) || entry.value == Value::Nil {
                break;
            }
            entry.value = Value::Number(number);
            written += 1;
        }
        if written != 0 {
            // Preserve the same version increments as the individual writes.
            self.advance_fields_version(written as u32);
        }
        written
    }

    pub(crate) fn set_string_scalar_cached(
        &mut self,
        key: StringId,
        value: Value,
        hint: &mut u32,
    ) -> bool {
        self.set_string_scalar_in_place(key, value, hint, false)
    }

    /// Use an already-reserved hash slot. Callers permitting new keys must
    /// have established that the collector needs no write barrier.
    pub(crate) fn set_string_scalar_in_place(
        &mut self,
        key: StringId,
        value: Value,
        hint: &mut u32,
        allow_new: bool,
    ) -> bool {
        if !matches!(
            value,
            Value::Boolean(_) | Value::Number(_) | Value::LightUserdata(_)
        ) {
            return false;
        }
        let Some(part) = self.hash.as_deref_mut() else {
            return false;
        };
        if let Some(entry) = part.entries.get_mut(hint.wrapping_sub(1) as usize)
            && matches!(entry.key, Key::String(id) if id == key)
            && entry.value != Value::Nil
        {
            entry.value = value;
            self.touch_fields();
            return true;
        }
        self.find_and_set_string_scalar(key, value, hint, allow_new)
    }

    #[cold]
    fn find_and_set_string_scalar(
        &mut self,
        key: StringId,
        value: Value,
        hint: &mut u32,
        allow_new: bool,
    ) -> bool {
        let key = Key::String(key);
        let hash = key.hash();
        let Some((slot, index)) = self.slot_hashed(key, hash) else {
            return false;
        };
        if let Some(index) = index {
            if self.hash_part().entries[index].value == Value::Nil {
                return false;
            }
            self.hash_part_mut().entries[index].value = value;
            self.touch_fields();
            *hint = (index + 1) as u32;
            return true;
        }
        let part = self.hash_part();
        let next = part.entries.len() + 1;
        if !allow_new
            || self.native51
            || self.metatable != 0
            || next > part.entries.capacity()
            || part.needs_rebuild(next)
        {
            return false;
        }
        let Ok(index) = u32::try_from(next) else {
            return false;
        };
        self.append_hash_entry(key, value, slot, index, hash);
        *hint = index;
        true
    }

    /// Update a validated string slot without growing the hash part.
    /// The caller must handle the key write barrier during propagation.
    #[inline]
    pub(crate) fn set_cached_global_scalar(
        &mut self,
        key: StringId,
        value: Value,
        hint: &mut u32,
        can_change_liveness: bool,
    ) -> bool {
        if !matches!(
            value,
            Value::Nil | Value::Boolean(_) | Value::Number(_) | Value::LightUserdata(_)
        ) {
            return false;
        }
        let key = Key::String(key);
        let hinted = hint.wrapping_sub(1) as usize;
        let index = if self
            .hash_part()
            .entries
            .get(hinted)
            .is_some_and(|entry| entry.key == key)
        {
            hinted
        } else if let Some((_, Some(index))) = self.slot(key) {
            index
        } else {
            return false;
        };
        let was_nil = self.hash_part().entries[index].value == Value::Nil;
        let now_nil = value == Value::Nil;
        if was_nil || now_nil {
            if !can_change_liveness || (was_nil && self.metatable().is_some()) {
                return false;
            }
            if was_nil && !now_nil {
                self.note_string_key_added();
            }
        }
        let native = self.native51;
        let part = self.hash_part_mut();
        if native && was_nil != now_nil {
            if now_nil {
                part.native_dead_keys += 1;
            } else if part.native_dead_keys != 0 {
                part.native_dead_keys -= 1;
            }
        }
        part.entries[index].value = value;
        self.touch_fields();
        *hint = (index + 1) as u32;
        if was_nil || now_nil {
            self.remember_hash_entry(*hint);
        }
        true
    }

    #[inline]
    fn remember_hash_entry(&self, index: u32) {
        let hints = &self
            .hash
            .as_deref()
            .expect("keyed table storage")
            .next_hash_hint;
        let previous = hints.load(Ordering::Relaxed) as u32;
        hints.store(
            (u64::from(previous) << 32) | u64::from(index),
            Ordering::Relaxed,
        );
    }
    pub fn set(&mut self, key: Value, value: Value) -> Result<(), Error> {
        self.set_with_native_hash(key, value, |_| {
            panic!("new Lua 5.1 string key needs its byte hash")
        })
    }
    pub(crate) fn set_with_native_hash(
        &mut self,
        key: Value,
        value: Value,
        string_hash: impl FnOnce(StringId) -> u32,
    ) -> Result<(), Error> {
        let k = Self::checked_key(key)?;
        if self.native51 {
            return self.native_set(k, value, string_hash);
        }
        let array = array_index(key);
        if let Some(i) = array {
            if i < self.array.len() || (i == self.array.len() && value != Value::Nil) {
                self.array.try_make_writable(i + 1)?;
            }
            if i < self.array.len() {
                self.array[i] = value;
                self.touch_array_at(i);
                return Ok(());
            }
            if i == self.array.len() && value != Value::Nil {
                if !self.hash_part().may_have_array_keys {
                    self.array.heap_mut().try_push(value)?;
                    self.touch_array();
                    return Ok(());
                }
                // Include every consecutive hash value that this insertion
                // will migrate. Reserve before changing any value or trace.
                let mut end = self.array.len() + 1;
                while self
                    .slot(Key::Number(((end + 1) as f64).to_bits()))
                    .and_then(|(_, i)| i)
                    .is_some_and(|i| self.hash_part().entries[i].value != Value::Nil)
                {
                    end += 1;
                }
                let additional = end - self.array.len();
                self.array
                    .heap_mut()
                    .try_reserve(additional)
                    .map_err(|_| Error::memory())?;
                self.array.heap_mut().push(value);
                self.take_hash(k);
                while let Some(v) =
                    self.take_hash(Key::Number(((self.array.len() + 1) as f64).to_bits()))
                {
                    self.array.heap_mut().push(v);
                }
                self.touch_array();
                return Ok(());
            }
        }
        let vacant = match self.slot(k) {
            Some((_, Some(i))) => {
                if matches!(k, Key::String(_))
                    && value != Value::Nil
                    && self.hash_part().entries[i].value == Value::Nil
                {
                    self.note_string_key_added();
                }
                self.hash_part_mut().entries[i].value = value;
                self.touch_fields();
                if matches!(k, Key::String(_)) {
                    self.remember_hash_entry((i + 1) as u32);
                }
                return Ok(());
            }
            Some((slot, None)) => Some(slot),
            None => None,
        };
        if value == Value::Nil {
            return Ok(());
        }
        let rebuild = self
            .hash_part()
            .needs_rebuild(self.hash_part().entries.len() + 1);
        let live = if rebuild {
            self.hash_part()
                .entries
                .iter()
                .filter(|e| e.value != Value::Nil)
                .count()
        } else {
            self.hash_part().entries.len()
        };
        let index = u32::try_from(live + 1)
            .map_err(|_| Error::new(ErrorKind::Runtime, 0, "table overflow"))?;
        let slots = if rebuild && self.hash_part().needs_rebuild(live + 1) {
            self.hash_part()
                .slots
                .len()
                .checked_mul(2)
                .ok_or_else(Error::memory)?
                .max(8)
        } else {
            self.hash_part().slots.len()
        };
        let part = self.ensure_hash_part()?;
        part.entries
            .try_reserve((live + 1).saturating_sub(part.entries.len()))
            .map_err(|_| Error::memory())?;
        part.slots
            .try_reserve(slots.saturating_sub(part.slots.len()))
            .map_err(|_| Error::memory())?;
        if rebuild {
            self.rebuild(slots, live);
        }
        let slot = if rebuild {
            self.slot(k).expect("table rebuild creates slots").0
        } else {
            vacant.expect("nonempty table has a vacant hash slot")
        };
        self.hash_part_mut().may_have_array_keys |= array.is_some();
        self.append_hash_entry(k, value, slot, index, k.hash());
        Ok(())
    }

    fn append_hash_entry(&mut self, key: Key, value: Value, slot: usize, index: u32, hash: usize) {
        let part = self.hash_part_mut();
        part.slots[slot] = index | Self::slot_tag(part.slots.len(), hash).0;
        part.entries.push(Entry {
            key,
            value,
            native_hash: 0,
            next: 0,
        });
        self.touch_fields();
        if matches!(key, Key::String(_)) {
            self.note_string_key_added();
            self.remember_hash_entry(index);
        }
    }
    #[inline]
    pub(crate) fn set_grown_list(&mut self, index: usize, value: Value) {
        self.array[index] = value;
        self.touch_array();
    }
    // All values must be scalar. Otherwise the caller applies write barriers
    // in SETLIST order. A same-size copy leaves the array's shape intact.
    #[inline]
    pub(crate) fn set_scalar_list_in_place(&mut self, start: usize, values: &[Value]) -> bool {
        if matches!(self.array, ArrayStorage::SharedScalars { .. }) {
            return false;
        }
        let Some(slots) = self.array.get_mut(start..start + values.len()) else {
            return false;
        };
        if !values.iter().copied().all(array_scalar) {
            return false;
        }
        slots.copy_from_slice(values);
        if !values.is_empty() {
            self.touch_array();
        }
        true
    }
    pub fn grow_list_array(&mut self, end: usize) -> Result<(), Error> {
        self.array.try_make_writable(end)?;
        let first = self.array.len();
        if end <= first {
            return Ok(());
        }
        if self.native51 {
            return self.native_resize(end, self.hash_part().entries.len());
        }
        // SETLIST includes nil fields in its array extent. Reserve before
        // changing traversal positions or moving any existing hash values.
        self.array
            .heap_mut()
            .try_reserve(end - first)
            .map_err(|_| Error::memory())?;
        self.invalidate_trace();
        self.array.heap_mut().resize(end, Value::Nil);
        self.touch_array();
        if !self.hash_part().entries.is_empty() {
            for index in first..end {
                if let Some(value) = self.take_hash(Key::Number(((index + 1) as f64).to_bits())) {
                    self.array[index] = value;
                }
            }
        }
        Ok(())
    }
    pub fn len(&self) -> usize {
        let mut hi = self.array.len();
        if hi > 0 && self.array[hi - 1] == Value::Nil {
            let mut lo = 0;
            while hi - lo > 1 {
                let mid = (hi + lo) / 2;
                if self.array[mid - 1] == Value::Nil {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            return lo;
        }
        if self.native51 && !self.hash_part().entries.is_empty() {
            // luaH_getn searches past the array part when its last slot is live.
            let mut upper = hi + 1;
            while self.get_key(Key::Number((upper as f64).to_bits())) != Value::Nil {
                hi = upper;
                if upper > i32::MAX as usize / 2 {
                    let mut index = 1;
                    while self.get(Value::Number(index as f64)) != Value::Nil {
                        index += 1;
                    }
                    return index - 1;
                }
                upper *= 2;
            }
            while upper - hi > 1 {
                let middle = (hi + upper) / 2;
                if self.get_key(Key::Number((middle as f64).to_bits())) == Value::Nil {
                    upper = middle;
                } else {
                    hi = middle;
                }
            }
        }
        hi
    }
    pub(crate) fn max_numeric_key(&self) -> f64 {
        let mut maximum = if let Some((Value::Nil, rest)) = self.array.split_last() {
            let live = |value: &Value| !matches!(value, Value::Nil);
            if rest.len() >= 63 && !rest[rest.len() - 7..].iter().any(live) {
                Self::last_array_key(&rest[..rest.len() - 7]) as f64
            } else {
                rest.iter()
                    .rposition(live)
                    .map_or(0.0, |index| (index + 1) as f64)
            }
        } else {
            self.array.len() as f64
        };
        for entry in &self.hash_part().entries {
            if let Key::Number(bits) = entry.key
                && entry.value != Value::Nil
            {
                maximum = maximum.max(f64::from_bits(bits));
            }
        }
        maximum
    }

    #[inline(never)]
    fn last_array_key(array: &[Value]) -> usize {
        let live = |value: &Value| !matches!(value, Value::Nil);
        let (head, blocks) = array.as_rchunks::<16>();
        // Check every tag in a block before finding its last live slot.
        let (start, values) = match blocks
            .iter()
            .rposition(|block| block.iter().fold(false, |found, value| found | live(value)))
        {
            Some(index) => (head.len() + index * 16, blocks[index].as_slice()),
            None => (0, head),
        };
        values
            .iter()
            .rposition(live)
            .map_or(0, |index| start + index + 1)
    }
    #[inline(never)]
    fn first_live_slot<T>(slots: &[T], live: impl Fn(&T) -> bool) -> Option<(usize, &T)> {
        let (head, tail) = slots.split_at(slots.len().min(8));
        if let Some(index) = head.iter().position(&live) {
            return Some((index, &head[index]));
        }
        let (blocks, rest) = tail.as_chunks::<16>();
        // Skip empty blocks before finding the first live slot in order.
        let (start, values) = match blocks
            .iter()
            .position(|block| block.iter().fold(false, |found, value| found | live(value)))
        {
            Some(index) => (head.len() + index * 16, blocks[index].as_slice()),
            None => (slots.len() - rest.len(), rest),
        };
        let index = values.iter().position(live)?;
        Some((start + index, &values[index]))
    }
    pub fn next(&self, key: Value) -> Result<Option<(Value, Value)>, Error> {
        self.next_entry(key)
            .map_err(|_| Error::new(ErrorKind::Runtime, 0, "invalid key to 'next'"))
    }
    pub(crate) fn next_entry(&self, key: Value) -> Result<Option<(Value, Value)>, InvalidNextKey> {
        let (array, hash) = if key == Value::Nil {
            (0, 0)
        } else if let Some(i) = array_index(key).filter(|&i| i < self.array.len()) {
            (i + 1, 0)
        } else {
            let key = Key::from_value(key).ok_or(InvalidNextKey)?;
            let hints = self.hash_part().next_hash_hint.load(Ordering::Relaxed);
            let first = hints as u32 as usize;
            let second = (hints >> 32) as u32 as usize;
            let index = if first != 0
                && self
                    .hash_part()
                    .entries
                    .get(first - 1)
                    .is_some_and(|entry| entry.key.matches_next(key))
            {
                first - 1
            } else if second != 0
                && self
                    .hash_part()
                    .entries
                    .get(second - 1)
                    .is_some_and(|entry| entry.key.matches_next(key))
            {
                second - 1
            } else {
                self.slot(key)
                    .and_then(|(_, index)| index)
                    .or_else(|| {
                        self.native51
                            .then(|| {
                                self.hash_part()
                                    .entries
                                    .iter()
                                    .position(|entry| entry.key.matches_next(key))
                            })
                            .flatten()
                    })
                    .ok_or(InvalidNextKey)?
            };
            (self.array.len(), index + 1)
        };
        if let Some((&value, rest)) = self.array[array..].split_first() {
            if !matches!(value, Value::Nil) {
                return Ok(Some((Value::Number((array + 1) as f64), value)));
            }
            if let Some((index, &value)) =
                Self::first_live_slot(rest, |value| !matches!(value, Value::Nil))
            {
                return Ok(Some((Value::Number((array + index + 2) as f64), value)));
            }
        }
        let Some(part) = &self.hash else {
            return Ok(None);
        };
        let Some(entry) = part.entries.get(hash) else {
            return Ok(None);
        };
        let (index, entry) = if matches!(entry.value, Value::Nil) {
            let tail = &part.entries[hash + 1..];
            let Some((index, entry)) =
                Self::first_live_slot(tail, |entry| !matches!(entry.value, Value::Nil))
            else {
                return Ok(None);
            };
            (hash + 1 + index, entry)
        } else {
            (hash, entry)
        };
        // A sequential cursor uses the primary hint. Discarding the secondary
        // avoids a read-modify-write on each key.
        part.next_hash_hint
            .store((index + 1) as u64, Ordering::Relaxed);
        Ok(Some((entry.key.value(), part.read_value(entry))))
    }
}
#[cfg(test)]
mod allocation_tests {
    use super::*;
    use crate::allocation_probe::probe;
    fn hash_key(n: i32) -> Value {
        Value::Number(-f64::from(n))
    }
    #[test]
    fn reserved_string_slots_require_insert_permission_and_a_plain_table() {
        let mut table = Table::with_capacity(0, 4).unwrap();
        let key = StringId(1);
        let second = StringId(2);
        let mut hint = 0;
        let version = table.fields_version();
        let added = table.string_keys_added();
        assert!(!table.set_string_scalar_in_place(key, Value::Number(1.0), &mut hint, false));
        assert!(table.set_string_scalar_in_place(key, Value::Number(1.0), &mut hint, true));
        assert_eq!(table.fields_version(), version.wrapping_add(1));
        assert_eq!(table.string_keys_added(), added.saturating_add(1));
        assert_eq!(table.get_string(key), Value::Number(1.0));

        table.set_metatable(Some(TableId(1)));
        assert!(!table.set_string_scalar_in_place(second, Value::Number(2.0), &mut hint, true));
        assert!(table.set_string_scalar_in_place(key, Value::Number(2.0), &mut hint, true));
        assert_eq!(table.get_string(key), Value::Number(2.0));
        assert_eq!(table.get_string(second), Value::Nil);
    }

    #[test]
    fn array_watch_tracks_two_distinct_blocks_before_invalidating() {
        let mut table = Table::default();
        table.watch_array();
        table.touch_array_at(ARRAY_WATCH_BLOCK);
        table.touch_array_at(0);
        table.touch_array_at(ARRAY_WATCH_BLOCK + 1);
        assert_eq!(
            table.array_change(),
            ArrayChange::Dirty {
                blocks: [1, 0],
                len: 2
            }
        );

        table.touch_array_at(2 * ARRAY_WATCH_BLOCK);
        table.touch_array_at(0);
        assert_eq!(table.array_change(), ArrayChange::Unwatched);

        table.watch_array();
        table.touch_array_at(ARRAY_WATCH_LIMIT - 1);
        assert_eq!(
            table.array_change(),
            ArrayChange::Dirty {
                blocks: [ARRAY_WATCH_LIMIT / ARRAY_WATCH_BLOCK - 1, 0],
                len: 1,
            }
        );
        table.touch_array_at(ARRAY_WATCH_LIMIT);
        assert_eq!(table.array_change(), ArrayChange::Unwatched);
    }
    #[test]
    fn max_numeric_key_keeps_live_values_across_block_edges() {
        let mut table = Table::with_capacity(129, 0).unwrap();
        for index in [1, 9, 10, 25, 26, 64, 65, 121, 128, 129] {
            let index = f64::from(index);
            let key = Value::Number(index);
            table.set(key, Value::Boolean(false)).unwrap();
            assert_eq!(table.max_numeric_key(), index);
            table.set(key, Value::Nil).unwrap();
            assert_eq!(table.max_numeric_key(), 0.0);
        }
    }
    #[test]
    fn next_keeps_sparse_array_order_before_hash_entries() {
        let mut table = Table::with_capacity(65, 1).unwrap();
        let first = Value::Number(10.0);
        let middle = Value::Number(35.0);
        let last = Value::Number(65.0);
        let hash = Value::Boolean(true);
        let value = Value::Boolean(false);
        for key in [first, middle, last, hash] {
            table.set(key, value).unwrap();
        }
        assert_eq!(table.next_entry(Value::Nil).unwrap(), Some((first, value)));
        table.set(first, Value::Nil).unwrap();
        assert_eq!(table.next_entry(first).unwrap(), Some((middle, value)));
        assert_eq!(table.next_entry(middle).unwrap(), Some((last, value)));
        assert_eq!(table.next_entry(last).unwrap(), Some((hash, value)));
        assert_eq!(table.next_entry(hash).unwrap(), None);
    }
    #[test]
    fn key_equality_preserves_each_variant_and_numeric_signed_zero() {
        let pairs = [
            (Key::Boolean(false), Key::Boolean(true)),
            (
                Key::Number(1.0_f64.to_bits()),
                Key::Number(2.0_f64.to_bits()),
            ),
            (Key::String(StringId(1)), Key::String(StringId(2))),
            (Key::Table(TableId(1)), Key::Table(TableId(2))),
            (Key::Function(FunctionId(1)), Key::Function(FunctionId(2))),
            (Key::Native(1), Key::Native(2)),
            (Key::Thread(ThreadId(1)), Key::Thread(ThreadId(2))),
            (Key::Userdata(UserdataId(1)), Key::Userdata(UserdataId(2))),
            (Key::LightUserdata(1), Key::LightUserdata(2)),
            (
                Key::ParserPrototype(ParserPrototypeId(1)),
                Key::ParserPrototype(ParserPrototypeId(2)),
            ),
        ];
        for (left, right) in pairs {
            assert_eq!(left, left);
            assert_eq!(right, right);
            assert_ne!(left, right, "{left:?} vs {right:?}");
        }

        let zero = Key::Number(0.0_f64.to_bits());
        let negative_zero = Key::Number((-0.0_f64).to_bits());
        assert_eq!(zero, negative_zero);
        assert_eq!(zero.hash(), negative_zero.hash());

        let distinct_tags = pairs.map(|(key, _)| key);
        for (i, left) in distinct_tags.iter().enumerate() {
            for right in &distinct_tags[i + 1..] {
                assert_ne!(left, right, "different key variants must not alias");
            }
        }
    }
    #[test]
    fn array_index_accepts_only_exact_positive_u32_keys() {
        for (number, expected) in [
            (f64::NEG_INFINITY, None),
            (-10.0, None),
            (-0.0, None),
            (0.0, None),
            (0.5, None),
            (1.0 - f64::EPSILON, None),
            (1.0, Some(0)),
            (1.0 + f64::EPSILON, None),
            (1.5, None),
            (2.0, Some(1)),
            (2.5, None),
            (f64::from(u32::MAX - 1), Some(u32::MAX as usize - 2)),
            (f64::from(u32::MAX), Some(u32::MAX as usize - 1)),
            (f64::from(u32::MAX) + 0.5, None),
            (f64::INFINITY, None),
            (f64::NAN, None),
        ] {
            assert_eq!(
                array_index(Value::Number(number)),
                expected,
                "key {number:?}"
            );
        }
        assert_eq!(array_index(Value::Nil), None);
        assert_eq!(array_index(Value::Boolean(true)), None);
    }
    #[test]
    fn integer_lookup_preserves_array_and_hash_keys() {
        let mut table = Table::with_capacity(3, 16).unwrap();
        table
            .set(Value::Number(-0.0), Value::Boolean(true))
            .unwrap();
        for (ordinal, index) in [
            i64::MIN,
            -i64::from(u32::MAX),
            -1,
            1,
            3,
            4,
            i64::from(u32::MAX),
            1 << 32,
            (1 << 32) + 1,
            (1 << 53) + 1,
            i64::MAX,
        ]
        .into_iter()
        .enumerate()
        {
            let value = Value::Number(ordinal as f64);
            table.set(Value::Number(index as f64), value).unwrap();
            assert_eq!(table.get_integer(index), value);
        }
        assert_eq!(table.get_integer(0), Value::Boolean(true));
        assert_eq!(table.get_integer(2), Value::Nil);
        assert_eq!(table.get_integer(5), Value::Nil);
    }
    #[test]
    fn numeric_hash_keys_and_array_keys_preserve_next_order() {
        let mut table = Table::with_capacity(3, 8).unwrap();
        let keys = [
            1.0,
            2.0,
            3.0,
            -0.0,
            -3.0,
            0.5,
            f64::from(u32::MAX),
            f64::INFINITY,
        ];
        for (i, &key) in keys.iter().enumerate() {
            table
                .set(Value::Number(key), Value::Number(i as f64))
                .unwrap();
        }
        assert_eq!(table.get(Value::Number(0.0)), Value::Number(3.0));
        table.set(Value::Number(0.0), Value::Number(30.0)).unwrap();
        assert_eq!(table.get(Value::Number(-0.0)), Value::Number(30.0));
        let mut seen = Vec::new();
        let mut current = Value::Nil;
        while let Some((key, value)) = table.next(current).unwrap() {
            seen.push((key, value));
            current = key;
        }
        let expected: Vec<_> = keys
            .iter()
            .enumerate()
            .map(|(i, &key)| {
                (
                    Value::Number(key),
                    Value::Number(if i == 3 { 30.0 } else { i as f64 }),
                )
            })
            .collect();
        assert_eq!(seen, expected);
        assert_eq!(table.get(Value::Number(f64::NAN)), Value::Nil);
        assert_eq!(
            table
                .set(Value::Number(f64::NAN), Value::Nil)
                .unwrap_err()
                .kind(),
            ErrorKind::Runtime
        );
        assert!(table.next(Value::Number(f64::NAN)).is_err());
    }
    #[test]
    fn next_hint_preserves_mixed_key_order_and_interleaved_cursors() {
        let mut table = Table::with_capacity(2, 8).unwrap();
        table.set(Value::Number(1.), Value::Number(11.)).unwrap();
        table.set(Value::Number(2.), Value::Number(22.)).unwrap();
        let keys = [
            Value::String(StringId((1u64 << 32) | 7)),
            Value::Table(TableId((2u64 << 32) | 3)),
            Value::Number(-0.0),
            Value::Boolean(false),
            Value::Number(-7.5),
        ];
        for (index, &key) in keys.iter().enumerate() {
            table.set(key, Value::Number((index + 30) as f64)).unwrap();
        }

        let expected = table.pairs().collect::<Vec<_>>();
        let mut actual = Vec::new();
        let mut key = Value::Nil;
        while let Some(pair) = table.next_entry(key).unwrap() {
            actual.push(pair);
            key = pair.0;
        }
        assert_eq!(actual, expected);
        assert_eq!(
            table.hash_part().next_hash_hint.load(Ordering::Relaxed) as u32,
            5
        );
        // Another iterator can overwrite the hint; the original cursor still
        // locates its exact key through the ordinary hash lookup.
        assert_eq!(
            table.next(keys[0]).unwrap(),
            Some((keys[1], Value::Number(31.)))
        );
        assert_eq!(
            table.next(Value::Number(0.)).unwrap(),
            Some((keys[3], Value::Number(33.)))
        );
        assert_eq!(
            table.next(Value::Number(2.)).unwrap(),
            Some((keys[0], Value::Number(30.)))
        );
        table
            .hash_part()
            .next_hash_hint
            .store(u64::from(u32::MAX), Ordering::Relaxed);
        assert_eq!(
            table.next(keys[1]).unwrap(),
            Some((keys[2], Value::Number(32.)))
        );
        assert!(table.next(Value::String(StringId(999))).is_err());
        assert!(table.next(Value::Table(TableId((3u64 << 32) | 3))).is_err());
        assert!(table.next(Value::Number(f64::NAN)).is_err());

        let (visited, stats) = probe(1, || {
            let mut current = Value::Nil;
            let mut visited = 0;
            while let Some((next, _)) = table.next_entry(current).unwrap() {
                visited += 1;
                current = next;
            }
            visited
        });
        assert_eq!(visited, expected.len());
        assert_eq!(stats.attempts, 0);
    }
    #[test]
    fn recent_hash_hint_shortcuts_only_exact_live_string_key_overwrites() {
        let a = StringId((1_u64 << 32) | 11);
        let b = StringId((1_u64 << 32) | 12);
        let mut table = Table::with_capacity(0, 4).unwrap();
        assert_eq!(table.hash_part().next_hash_hint.load(Ordering::Relaxed), 0);
        assert!(!table.set_existing_string_scalar(a, Value::Number(1.)));
        table.set(Value::String(a), Value::Number(2.)).unwrap();
        assert_eq!(table.hash_part().next_hash_hint.load(Ordering::Relaxed), 1);
        let (updated, stats) = probe(1, || table.set_existing_string_scalar(a, Value::Number(3.)));
        assert!(updated);
        assert_eq!(stats.attempts, 0);
        assert_eq!(table.get(Value::String(a)), Value::Number(3.));

        table.set(Value::String(b), Value::Number(4.)).unwrap();
        assert!(table.set_existing_string_scalar(a, Value::Number(5.)));
        assert!(table.set_existing_string_scalar(b, Value::Number(4.)));
        table.set(Value::String(a), Value::Number(5.)).unwrap();
        assert!(table.set_existing_string_scalar(a, Value::Number(6.)));
        table.set(Value::String(a), Value::Nil).unwrap();
        assert!(!table.set_existing_string_scalar(a, Value::Number(7.)));
        table.set(Value::String(a), Value::Number(8.)).unwrap();
        table
            .hash_part()
            .next_hash_hint
            .store(u64::from(u32::MAX), Ordering::Relaxed);
        assert!(table.set_existing_string_scalar(a, Value::Number(9.)));
        table.set(Value::String(a), Value::Number(9.)).unwrap();
        assert!(table.set_existing_string_scalar(a, Value::Number(10.)));
        assert_eq!(table.get(Value::String(a)), Value::Number(10.));
        assert_eq!(
            table.next(Value::String(a)).unwrap(),
            Some((Value::String(b), Value::Number(4.)))
        );
    }
    #[test]
    fn scalar_overwrites_find_four_rotating_string_keys_without_allocating() {
        let keys = [11, 12, 13, 14].map(|slot| StringId((1_u64 << 32) | slot));
        let mut table = Table::with_capacity(0, keys.len()).unwrap();
        for &key in &keys {
            table.set(Value::String(key), Value::Number(0.)).unwrap();
        }
        let original_bytes = table.heap_bytes();
        let ((), stats) = probe(1, || {
            for i in 1..=64 {
                for &key in &keys {
                    assert!(table.set_existing_string_scalar(key, Value::Number(i as f64)));
                }
            }
        });
        assert_eq!(stats.attempts, 0);
        assert_eq!(table.heap_bytes(), original_bytes);
        assert_eq!(
            table.pairs().collect::<Vec<_>>(),
            keys.map(|key| (Value::String(key), Value::Number(64.)))
        );
        table.set(Value::String(keys[0]), Value::Nil).unwrap();
        assert!(!table.set_existing_string_scalar(keys[0], Value::Number(65.)));
        assert_eq!(table.get(Value::String(keys[0])), Value::Nil);
    }
    #[test]
    fn next_hint_allows_deleted_controls_until_tombstones_are_compacted() {
        let mut table = Table::with_capacity(0, 5).unwrap();
        for n in 1..=5 {
            table.set(hash_key(n), Value::Number(n as f64)).unwrap();
        }
        assert_eq!(table.next(Value::Nil).unwrap().unwrap().0, hash_key(1));
        assert_eq!(table.next(hash_key(1)).unwrap().unwrap().0, hash_key(2));
        table.set(hash_key(2), Value::Nil).unwrap();
        assert_eq!(
            table.next(hash_key(2)).unwrap(),
            Some((hash_key(3), Value::Number(3.)))
        );
        table.set(hash_key(1), Value::Nil).unwrap();
        assert_eq!(
            table.next(hash_key(1)).unwrap(),
            Some((hash_key(3), Value::Number(3.)))
        );

        // The next insertion compacts tombstones. A stale physical index must
        // not make a removed key valid after it leaves the hash slots.
        table.set(hash_key(6), Value::Number(6.)).unwrap();
        assert!(table.next(hash_key(1)).is_err());
        assert!(table.next(hash_key(2)).is_err());
        assert_eq!(
            table.next(hash_key(3)).unwrap(),
            Some((hash_key(4), Value::Number(4.)))
        );
        let expected = table.pairs().collect::<Vec<_>>();
        let mut current = Value::Nil;
        for &pair in &expected {
            assert_eq!(table.next(current).unwrap(), Some(pair));
            current = pair.0;
        }
        assert_eq!(table.next(current).unwrap(), None);
    }
    #[test]
    fn lookups_agree_with_a_model_across_the_tagged_slot_boundary() {
        fn key(n: u32) -> Value {
            if n % 3 == 0 {
                Value::String(StringId(u64::from(n) | 1 << 32))
            } else {
                hash_key(n as i32)
            }
        }
        let mut table = Table::with_capacity(0, 0).unwrap();
        let mut model = std::collections::HashMap::new();
        // Grow through the largest tagged slot array into untagged words, with
        // deletions so that tombstone compaction rewrites the words as well.
        for n in 0..3000_u32 {
            table.set(key(n), Value::Number(f64::from(n))).unwrap();
            model.insert(n, f64::from(n));
            if n % 7 == 3 {
                table.set(key(n / 2), Value::Nil).unwrap();
                model.remove(&(n / 2));
            }
            if n % 97 == 0 || (700..800).contains(&n) {
                for probe in 0..n + 50 {
                    let expected = model.get(&probe).map_or(Value::Nil, |v| Value::Number(*v));
                    assert_eq!(table.get(key(probe)), expected, "key {probe} after {n}");
                }
            }
        }
        assert!(table.hash_part().slots.len() > TAGGED_SLOTS);
    }
    // The longest distance from an entry's home slot to the slot that holds it.
    fn longest_probe(table: &Table) -> usize {
        let mask = table.hash_part().slots.len() - 1;
        let (_, index) = Table::slot_tag(table.hash_part().slots.len(), 0);
        let mut longest = 0;
        for (slot, &word) in table.hash_part().slots.iter().enumerate() {
            if word != 0 {
                let entry = &table.hash_part().entries[(word & index) as usize - 1];
                let home = entry.key.hash() & mask;
                longest = longest.max(slot.wrapping_sub(home) & mask);
            }
        }
        longest
    }
    #[test]
    fn number_keys_spread_across_hash_slots() {
        // Keys whose bits differ only near the top of the word: small integers
        // and simple fractions, as in sparse arrays and beat-keyed tables.
        let families: [(&str, fn(u32) -> f64); 5] = [
            ("from two", |n| f64::from(n) + 2.0),
            ("negative", |n| -f64::from(n)),
            ("quarters", |n| f64::from(n) / 4.0),
            ("from zero", f64::from),
            ("large", |n| f64::from(n) * 65536.0 + 1e9),
        ];
        for (label, key) in families {
            let mut table = Table::with_capacity(0, 0).unwrap();
            for n in 0..4096 {
                table
                    .set(Value::Number(key(n)), Value::Boolean(true))
                    .unwrap();
            }
            if table.hash_part().slots.is_empty() {
                continue;
            }
            let longest = longest_probe(&table);
            assert!(
                longest < 64,
                "{label}: an entry sits {longest} slots from home"
            );
        }
    }
    fn check_hash(table: &Table, count: i32) {
        for n in 1..=count {
            assert_eq!(table.get(hash_key(n)), Value::Number(f64::from(n)));
        }
    }
    #[test]
    fn hash_reservations_preserve_values_and_pending_traversal_on_failure() {
        for initialized in [false, true] {
            let mut points = 0;
            // A table without keyed storage also reserves its hash header.
            let expected_points = if initialized { 2 } else { 3 };
            for nth in 0..=expected_points {
                let mut table = if initialized {
                    Table::with_capacity(0, 5).unwrap()
                } else {
                    Table::default()
                };
                let count = if initialized { 5 } else { 0 };
                for n in 1..=count {
                    table.set(hash_key(n), Value::Number(f64::from(n))).unwrap();
                }
                table.trace = TableTrace::Pending(false, false);
                let pairs = table.pairs().collect::<Vec<_>>();
                let (result, stats) = probe(nth, || table.set(hash_key(6), Value::Number(42.)));
                if nth == 0 {
                    points = stats.attempts;
                    assert_eq!(points, expected_points);
                    result.unwrap();
                } else {
                    assert!(nth <= points);
                    assert_eq!(stats.rejected, 1);
                    assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                    assert_eq!(table.get(hash_key(6)), Value::Nil);
                    assert_eq!(table.pairs().collect::<Vec<_>>(), pairs);
                    assert_eq!(table.trace, TableTrace::Pending(false, false));
                    table.set(hash_key(6), Value::Number(42.)).unwrap();
                }
                check_hash(&table, count);
                assert_eq!(table.get(hash_key(6)), Value::Number(42.));
            }
            eprintln!("TABLE_MUTATION_FAILURE_POINTS {points}");
        }
    }
    #[test]
    fn a_failed_list_array_growth_keeps_nil_holes_and_existing_hash_values() {
        let mut table = Table::with_capacity(1, 3).unwrap();
        for index in [1, 3, 5, 9] {
            table
                .set(Value::Number(index as f64), Value::Number(index as f64))
                .unwrap();
        }
        table.trace = TableTrace::Pending(false, false);
        let before = table.pairs().collect::<Vec<_>>();
        let (result, stats) = probe(1, || table.grow_list_array(6));
        assert_eq!(stats.rejected, 1);
        assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
        assert_eq!(table.array.len(), 1);
        assert_eq!(table.pairs().collect::<Vec<_>>(), before);
        assert_eq!(table.trace, TableTrace::Pending(false, false));
        table.grow_list_array(6).unwrap();
        for index in [1, 3, 5, 9] {
            assert_eq!(
                table.get(Value::Number(index as f64)),
                Value::Number(index as f64)
            );
        }
        for index in [2, 4, 6] {
            assert_eq!(table.get(Value::Number(index as f64)), Value::Nil);
        }
    }
    #[test]
    fn a_failed_array_migration_keeps_every_hash_entry_and_the_array_length() {
        for nth in 0..=1 {
            let mut table = Table::with_capacity(4, 0).unwrap();
            for n in (1..=4).chain(6..=20) {
                table
                    .set(Value::Number(f64::from(n)), Value::Number(f64::from(n)))
                    .unwrap();
            }
            table.trace = TableTrace::Pending(false, false);
            let pairs = table.pairs().collect::<Vec<_>>();
            let (result, stats) = probe(nth, || table.set(Value::Number(5.), Value::Number(5.)));
            if nth == 0 {
                result.unwrap();
                assert_eq!(stats.attempts, 1);
            } else {
                assert_eq!(stats.rejected, 1);
                assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                assert_eq!(table.array.len(), 4);
                assert_eq!(table.get(Value::Number(5.)), Value::Nil);
                assert_eq!(table.pairs().collect::<Vec<_>>(), pairs);
                assert_eq!(table.trace, TableTrace::Pending(false, false));
                table.set(Value::Number(5.), Value::Number(5.)).unwrap();
            }
            assert_eq!(table.array.len(), 20);
            for n in 1..=20 {
                assert_eq!(
                    table.get(Value::Number(f64::from(n))),
                    Value::Number(f64::from(n))
                );
            }
        }
        eprintln!("TABLE_MUTATION_FAILURE_POINTS 1");
    }
    #[test]
    fn tombstone_compaction_reuses_storage_when_the_allocator_rejects_every_request() {
        let mut table = Table::with_capacity(0, 5).unwrap();
        for n in 1..=5 {
            table.set(hash_key(n), Value::Number(f64::from(n))).unwrap();
        }
        table.set(hash_key(3), Value::Nil).unwrap();
        assert!(table.next(hash_key(3)).is_ok());
        table.trace = TableTrace::Pending(false, false);
        let (result, stats) = probe(1, || table.set(hash_key(6), Value::Number(42.)));
        result.unwrap();
        assert_eq!(stats.attempts, 0);
        assert_eq!(table.trace, TableTrace::Restart);
        assert!(table.next(hash_key(3)).is_err());
        assert_eq!(table.get(hash_key(6)), Value::Number(42.));
        for n in [1, 2, 4, 5] {
            assert_eq!(table.get(hash_key(n)), Value::Number(f64::from(n)));
        }
    }
    #[test]
    fn updates_and_deletions_need_no_allocation() {
        let mut table = Table::default();
        table.set(Value::Number(1.), Value::Number(17.)).unwrap();
        table.set(hash_key(2), Value::Number(25.)).unwrap();
        let (result, stats) = probe(1, || -> Result<(), Error> {
            table.set(Value::Number(1.), Value::Number(42.))?;
            table.set(hash_key(2), Value::Number(42.))?;
            table.set(hash_key(2), Value::Nil)?;
            table.set(hash_key(3), Value::Nil)
        });
        result.unwrap();
        assert_eq!(stats.attempts, 0);
        assert_eq!(table.get(Value::Number(1.)), Value::Number(42.));
        assert_eq!(table.get(hash_key(2)), Value::Nil);
    }
}
