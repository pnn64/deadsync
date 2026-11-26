//! Lexical capture limits while parsing, including unfinished reader input.
use super::{NameBinding, RootText, Text, hash};
use crate::lexer::Lexeme;
use crate::{
    Error, ErrorKind,
    memory::fallible::{self, VecExt},
};
const NONE: u32 = u32::MAX;
const MAX_UPVALUES: u8 = 60;

pub(super) enum Reference {
    Local { register: u16, visible: u8 },
    Upvalue,
    Global(u32),
}
impl Reference {
    pub(super) fn binding(&self) -> NameBinding {
        match *self {
            Self::Local { visible, .. } => NameBinding::local(visible),
            Self::Upvalue => NameBinding::UPVALUE,
            Self::Global(_) => NameBinding::GLOBAL,
        }
    }
}

#[derive(Debug)]
struct Name {
    start: usize,
    len: usize,
    hash: u64,
}
#[derive(Clone, Copy, Debug)]
struct Local {
    symbol: u32,
    // Slot among source-level variables. Logical stack accounting also
    // includes the hidden numeric and generic for loop controls.
    visible: u8,
}
#[derive(Debug)]
struct Scope {
    base: usize,
    head: u32,
    count: u8,
    line: u32,
}
#[derive(Debug)]
struct Capture {
    name: u32,
    next: u32,
}

/// Names borrow offsets in the retained source prefix. Scopes share one local
/// stack; linked capture indices allow parent captures to survive child scopes.
#[derive(Debug)]
pub(super) struct Bindings {
    names: Vec<Name>,
    buckets: Vec<u32>,
    locals: Vec<Local>,
    scopes: Vec<Scope>,
    captures: Vec<Capture>,
    free: u32,
    last_token: Option<u32>,
}
impl Bindings {
    pub(super) fn try_new() -> Result<Self, Error> {
        Ok(Self {
            names: Vec::new(),
            buckets: Vec::new(),
            locals: Vec::new(),
            scopes: fallible::vec([Scope {
                base: 0,
                head: NONE,
                count: 0,
                line: 0,
            }])?,
            captures: Vec::new(),
            free: NONE,
            last_token: None,
        })
    }
}
impl Bindings {
    /// Return to the state of `try_new`, keeping storage.
    pub(super) fn reset(&mut self) -> Result<(), Error> {
        self.names.clear();
        self.buckets.clear();
        self.locals.clear();
        self.captures.clear();
        self.free = NONE;
        self.last_token = None;
        self.scopes.clear();
        self.scopes.try_push(Scope {
            base: 0,
            head: NONE,
            count: 0,
            line: 0,
        })
    }
    pub(super) fn heap_bytes(&self) -> usize {
        self.names.capacity() * size_of::<Name>()
            + self.buckets.capacity() * size_of::<u32>()
            + self.locals.capacity() * size_of::<Local>()
            + self.scopes.capacity() * size_of::<Scope>()
            + self.captures.capacity() * size_of::<Capture>()
    }
    fn lookup(&self, input: &[u8], bytes: &[u8], hash: u64) -> Option<u32> {
        if bytes == b"arg" {
            return Some(0);
        }
        if bytes == b"self" {
            return Some(1);
        }
        if self.buckets.is_empty() {
            return None;
        }
        let mut slot = hash as usize & (self.buckets.len() - 1);
        loop {
            let id = self.buckets[slot];
            if id == 0 {
                return None;
            }
            let name = &self.names[id as usize - 2];
            if name.hash == hash && input[name.start..name.start + name.len] == *bytes {
                return Some(id);
            }
            slot = (slot + 1) & (self.buckets.len() - 1);
        }
    }
    pub(super) fn intern(&mut self, input: &[u8], start: usize, end: usize) -> Result<u32, Error> {
        let bytes = &input[start..end];
        if let Some(id) = self.last_token
            && self.bytes(input, id) == bytes
        {
            return Ok(id);
        }
        let hash = hash(bytes);
        if let Some(id) = self.lookup(input, bytes, hash) {
            self.last_token = Some(id);
            return Ok(id);
        }
        if (self.names.len() + 1) * 2 >= self.buckets.len() {
            let size = (self.buckets.len() * 2).max(16);
            self.buckets.clear();
            self.buckets.try_resize(size, 0)?;
            for (index, name) in self.names.iter().enumerate() {
                let mut slot = name.hash as usize & (self.buckets.len() - 1);
                while self.buckets[slot] != 0 {
                    slot = (slot + 1) & (self.buckets.len() - 1);
                }
                self.buckets[slot] = index as u32 + 2;
            }
        }
        let mut slot = hash as usize & (self.buckets.len() - 1);
        while self.buckets[slot] != 0 {
            slot = (slot + 1) & (self.buckets.len() - 1);
        }
        let id = self.names.len() as u32 + 2;
        self.names.try_push(Name {
            start,
            len: bytes.len(),
            hash,
        })?;
        self.buckets[slot] = id;
        self.last_token = Some(id);
        Ok(id)
    }
    pub(super) fn name(&self, input: &[u8], bytes: &[u8]) -> u32 {
        // The lexer usually just interned the name that the grammar resolves.
        if let Some(id) = self.last_token
            && self.bytes(input, id) == bytes
        {
            return id;
        }
        self.lookup(input, bytes, hash(bytes))
            .expect("lexer interned identifier")
    }
    pub(super) fn bytes<'a>(&self, input: &'a [u8], name: u32) -> &'a [u8] {
        match name {
            0 => b"arg",
            1 => b"self",
            _ => {
                let n = &self.names[name as usize - 2];
                &input[n.start..n.start + n.len]
            }
        }
    }
    pub(super) fn root_text(&self, input: &[u8], symbol: u32) -> Result<RootText, Error> {
        Ok(match symbol {
            0 => RootText::Static(b"arg"),
            1 => RootText::Static(b"self"),
            _ => {
                let name = &self.names[symbol as usize - 2];
                let end = name.start + name.len;
                if let Ok(end) = u32::try_from(end) {
                    RootText::Source {
                        start: name.start as u32,
                        end,
                    }
                } else {
                    RootText::Owned(Text::new(&input[name.start..end])?)
                }
            }
        })
    }
    pub(super) fn symbol_hash(&self, symbol: u32) -> u64 {
        match symbol {
            0 => hash(b"arg"),
            1 => hash(b"self"),
            _ => self.names[symbol as usize - 2].hash,
        }
    }
    pub(super) fn intern_lexeme(&mut self, input: &[u8], name: &Lexeme) -> Result<u32, Error> {
        if let Lexeme::Source { start, end, .. } = *name {
            self.intern(input, start as usize, end as usize)
        } else {
            // Synthesized parameters and heap-backed names already have a symbol.
            Ok(self.name(input, name.bytes(input)))
        }
    }
    pub(super) fn declare(&mut self, input: &[u8], name: &Lexeme) -> Result<(), Error> {
        let symbol = self.intern_lexeme(input, name)?;
        self.declare_symbol(symbol)
    }
    fn declare_symbol(&mut self, symbol: u32) -> Result<(), Error> {
        let visible = self.next_visible();
        self.locals.try_push(Local { symbol, visible })
    }
    fn next_visible(&self) -> u8 {
        self.locals[self.scopes.last().unwrap().base..]
            .last()
            .map_or(0, |local| local.visible + u8::from(local.symbol != NONE))
    }
    pub(super) fn hidden(&mut self, count: usize) -> Result<(), Error> {
        let visible = self.next_visible();
        self.locals.try_resize(
            self.locals.len() + count,
            Local {
                symbol: NONE,
                visible,
            },
        )?;
        Ok(())
    }
    pub(super) fn truncate(&mut self, count: u16) {
        self.locals
            .truncate(self.scopes.last().unwrap().base + usize::from(count));
    }
    pub(super) fn enter(
        &mut self,
        input: &[u8],
        line: u32,
        params: &[Lexeme],
        vararg: bool,
    ) -> Result<(), Error> {
        self.scopes.try_push(Scope {
            base: self.locals.len(),
            head: NONE,
            count: 0,
            line,
        })?;
        for param in params {
            self.declare(input, param)?;
        }
        if vararg {
            self.declare_symbol(0)?;
        }
        Ok(())
    }
    pub(super) fn has_captures(&self) -> bool {
        self.scopes.last().expect("function scope").count != 0
    }

    pub(super) fn leave(&mut self) {
        let scope = self.scopes.pop().expect("nested function");
        self.locals.truncate(scope.base);
        let mut index = scope.head;
        while index != NONE {
            let capture = &mut self.captures[index as usize];
            let next = capture.next;
            capture.next = self.free;
            self.free = index;
            index = next;
        }
    }
    fn captured(&self, scope: usize, name: u32) -> bool {
        let mut index = self.scopes[scope].head;
        while index != NONE {
            let capture = &self.captures[index as usize];
            if capture.name == name {
                return true;
            }
            index = capture.next;
        }
        false
    }
    /// Return the first overflowing function's definition line. Captures must
    /// propagate from the owning scope outwards, as for nested closures in 5.1.
    pub(super) fn reference_symbol(&mut self, name: u32) -> Result<Reference, Error> {
        let mut end = self.locals.len();
        let mut owner = None;
        for index in (0..self.scopes.len()).rev() {
            let start = self.scopes[index].base;
            let local = self.locals[start..end]
                .iter()
                .rposition(|n| n.symbol == name);
            if index + 1 == self.scopes.len()
                && let Some(reg) = local
            {
                return Ok(Reference::Local {
                    register: reg as u16,
                    visible: self.locals[start + reg].visible,
                });
            }
            if local.is_some() || self.captured(index, name) {
                owner = Some(index);
                break;
            }
            end = start;
        }
        let Some(owner) = owner else {
            return Ok(Reference::Global(name));
        }; // Global.
        for scope in owner + 1..self.scopes.len() {
            if self.scopes[scope].count == MAX_UPVALUES {
                return Err(Error::new_static(
                    ErrorKind::Limit,
                    self.scopes[scope].line,
                    "upvalue limit",
                ));
            }
            let capture = Capture {
                name,
                next: self.scopes[scope].head,
            };
            let index = if self.free == NONE {
                let index = self.captures.len() as u32;
                self.captures.try_push(capture)?;
                index
            } else {
                let index = self.free;
                self.free = self.captures[index as usize].next;
                self.captures[index as usize] = capture;
                index
            };
            self.scopes[scope].head = index;
            self.scopes[scope].count += 1;
        }
        Ok(Reference::Upvalue)
    }
}
