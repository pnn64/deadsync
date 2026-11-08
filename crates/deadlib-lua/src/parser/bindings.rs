//! Lexical capture limits while parsing, including unfinished reader input.
const NONE: u32 = u32::MAX;
const MAX_UPVALUES: u8 = 60;

pub(super) enum Reference {
    Local(u16),
    Upvalue,
    Global(u32),
}

#[derive(Debug)]
struct Name {
    start: usize,
    len: usize,
    hash: u64,
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
    locals: Vec<u32>,
    scopes: Vec<Scope>,
    captures: Vec<Capture>,
    free: u32,
}
impl Default for Bindings {
    fn default() -> Self {
        Self {
            names: Vec::new(),
            buckets: Vec::new(),
            locals: Vec::new(),
            scopes: vec![Scope {
                base: 0,
                head: NONE,
                count: 0,
                line: 0,
            }],
            captures: Vec::new(),
            free: NONE,
        }
    }
}
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100_0000_01b3)
    })
}
impl Bindings {
    pub(super) fn heap_bytes(&self) -> usize {
        self.names.capacity() * size_of::<Name>()
            + self.buckets.capacity() * size_of::<u32>()
            + self.locals.capacity() * size_of::<u32>()
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
    pub(super) fn intern(&mut self, input: &[u8], start: usize, end: usize) {
        let bytes = &input[start..end];
        let hash = hash(bytes);
        if self.lookup(input, bytes, hash).is_some() {
            return;
        }
        if (self.names.len() + 1) * 2 >= self.buckets.len() {
            self.buckets = vec![0; (self.buckets.len() * 2).max(16)];
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
        self.buckets[slot] = self.names.len() as u32 + 2;
        self.names.push(Name {
            start,
            len: bytes.len(),
            hash,
        });
    }
    pub(super) fn name(&self, input: &[u8], bytes: &[u8]) -> u32 {
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
    pub(super) fn declare(&mut self, input: &[u8], bytes: &[u8]) {
        self.locals.push(self.name(input, bytes));
    }
    pub(super) fn hidden(&mut self, count: usize) {
        self.locals.resize(self.locals.len() + count, NONE);
    }
    pub(super) fn truncate(&mut self, count: u16) {
        self.locals
            .truncate(self.scopes.last().unwrap().base + usize::from(count));
    }
    pub(super) fn enter(&mut self, input: &[u8], line: u32, params: &[Box<[u8]>], vararg: bool) {
        self.scopes.push(Scope {
            base: self.locals.len(),
            head: NONE,
            count: 0,
            line,
        });
        for param in params {
            self.declare(input, param);
        }
        if vararg {
            self.locals.push(0);
        }
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
    pub(super) fn reference(&mut self, input: &[u8], bytes: &[u8]) -> Result<Reference, u32> {
        let name = self.name(input, bytes);
        let mut end = self.locals.len();
        let mut owner = None;
        for index in (0..self.scopes.len()).rev() {
            let start = self.scopes[index].base;
            let local = self.locals[start..end].iter().rposition(|&n| n == name);
            if index + 1 == self.scopes.len()
                && let Some(reg) = local
            {
                return Ok(Reference::Local(reg as u16));
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
                return Err(self.scopes[scope].line);
            }
            let capture = Capture {
                name,
                next: self.scopes[scope].head,
            };
            let index = if self.free == NONE {
                let index = self.captures.len() as u32;
                self.captures.push(capture);
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
