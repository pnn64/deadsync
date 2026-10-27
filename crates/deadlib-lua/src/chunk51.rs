//! Bounded, resumable decoding of native Lua 5.1 precompiled chunks.
//! Prototypes and continuations are flat; input prefixes are never replayed.
use crate::{Error, ErrorKind, bytecode::Constant};

const DEPTH_LIMIT: usize = 200;
pub(crate) const HEADER_SIZE: usize = 12;
pub(crate) fn header() -> [u8; HEADER_SIZE] {
    [
        27,
        b'L',
        b'u',
        b'a',
        0x51,
        0,
        u8::from(cfg!(target_endian = "little")),
        4,
        std::mem::size_of::<usize>() as u8,
        4,
        8,
        0,
    ]
}

#[derive(Debug, Default)]
struct Local {
    name: Option<Box<[u8]>>,
    start: u32,
    end: u32,
}
#[derive(Debug, Default)]
struct Prototype {
    source: Option<Box<[u8]>>,
    defined: [u32; 2],
    nups: u8,
    params: u8,
    vararg: u8,
    stack: u8,
    code: Vec<u32>,
    constants: Vec<Constant>,
    children: Vec<usize>,
    lines: Vec<u32>,
    locals: Vec<Local>,
    names: Vec<Option<Box<[u8]>>>,
}
#[derive(Debug, Default)]
struct Chunk {
    prototypes: Vec<Prototype>,
}

/// Native words/debug offsets retained for re-export of imported code.
#[derive(Clone, Debug)]
pub(crate) struct Wire {
    pub(crate) code: Box<[u32]>,
    pub(crate) lines: Box<[u32]>,
    pub(crate) locals: Box<[[u32; 2]]>,
    pub(crate) children: Box<[usize]>,
    pub(crate) constants: usize,
    pub(crate) names: usize,
    pub(crate) vararg: u8,
}
impl Wire {
    pub fn heap_bytes(&self) -> usize {
        std::mem::size_of_val(self.code.as_ref())
            + std::mem::size_of_val(self.lines.as_ref())
            + std::mem::size_of_val(self.locals.as_ref())
            + std::mem::size_of_val(self.children.as_ref())
    }
}

#[derive(Clone, Copy, Debug)]
enum StringTarget {
    Source(usize),
    Constant(usize),
    Local(usize),
    Upvalue(usize),
}
#[derive(Clone, Copy, Debug)]
enum Task {
    Header,
    Function(Option<usize>),
    Defined(usize, bool),
    Flags(usize),
    CodeCount(usize),
    Code(usize, usize),
    ConstantsCount(usize),
    Constants(usize, usize),
    Constant(usize, u8),
    ChildrenCount(usize),
    Children(usize, usize),
    LinesCount(usize),
    Lines(usize, usize),
    LocalsCount(usize),
    Locals(usize, usize),
    LocalBounds(usize),
    LocalBoundsEnd(usize),
    NamesCount(usize),
    Names(usize, usize),
    String(StringTarget, Option<usize>),
    Verify(usize),
}
#[derive(Debug)]
pub(crate) struct Decoder {
    chunk: Chunk,
    tasks: Vec<Task>,
    position: usize,
    depth: usize,
    base_depth: usize,
}
impl Decoder {
    pub(crate) fn new(base_depth: usize) -> Self {
        Self {
            chunk: Chunk::default(),
            tasks: vec![Task::Function(None), Task::Header],
            position: 0,
            depth: 0,
            base_depth,
        }
    }
    pub(crate) fn nesting(&self) -> usize {
        self.depth
    }
    pub(crate) fn heap_bytes(&self) -> usize {
        let mut bytes = self.tasks.capacity() * std::mem::size_of::<Task>()
            + self.chunk.prototypes.capacity() * std::mem::size_of::<Prototype>();
        for p in &self.chunk.prototypes {
            bytes += p.source.as_ref().map_or(0, |s| s.len())
                + p.code.capacity() * 4
                + p.constants.capacity() * std::mem::size_of::<Constant>()
                + p.children.capacity() * std::mem::size_of::<usize>()
                + p.lines.capacity() * 4
                + p.locals.capacity() * std::mem::size_of::<Local>()
                + p.names.capacity() * std::mem::size_of::<Option<Box<[u8]>>>();
            bytes += p
                .constants
                .iter()
                .map(|k| {
                    if let Constant::String(s) = k {
                        s.len()
                    } else {
                        0
                    }
                })
                .sum::<usize>();
            bytes += p
                .locals
                .iter()
                .map(|l| l.name.as_ref().map_or(0, |s| s.len()))
                .sum::<usize>();
            bytes += p.names.iter().flatten().map(|s| s.len()).sum::<usize>();
        }
        bytes
    }
    fn read<'a>(
        &mut self,
        input: &'a [u8],
        count: usize,
        closed: bool,
    ) -> Result<Option<&'a [u8]>, &'static str> {
        let Some(end) = self.position.checked_add(count) else {
            return Err("unexpected end");
        };
        match input.get(self.position..end) {
            Some(bytes) => {
                self.position = end;
                Ok(Some(bytes))
            }
            None if closed => Err("unexpected end"),
            None => Ok(None),
        }
    }
    /// `input` retains its prefix between calls; true means the root ended.
    /// The caller may leave trailing input unread, as Lua's binary loader does.
    pub(crate) fn resume(&mut self, input: &[u8], closed: bool) -> Result<bool, &'static str> {
        macro_rules! read {
            ($n:expr) => {
                match self.read(input, $n, closed)? {
                    Some(b) => b,
                    None => return Ok(false),
                }
            };
        }
        macro_rules! count {
            () => {{
                let n = i32::from_ne_bytes(read!(4).try_into().unwrap());
                if n < 0 {
                    return Err("bad integer");
                }
                n as usize
            }};
        }
        while let Some(task) = self.tasks.last().copied() {
            match task {
                Task::Header => {
                    if read!(HEADER_SIZE) != header() {
                        return Err("bad header");
                    }
                    self.tasks.pop();
                }
                Task::Function(parent) => {
                    if self.base_depth.saturating_add(self.depth) >= DEPTH_LIMIT {
                        return Err("code too deep");
                    }
                    self.depth += 1;
                    self.tasks.pop();
                    let id = self.chunk.prototypes.len();
                    self.chunk.prototypes.push(Prototype::default());
                    if let Some(parent) = parent {
                        self.chunk.prototypes[parent].children.push(id);
                    }
                    self.tasks.extend([
                        Task::Verify(id),
                        Task::NamesCount(id),
                        Task::LocalsCount(id),
                        Task::LinesCount(id),
                        Task::ChildrenCount(id),
                        Task::ConstantsCount(id),
                        Task::CodeCount(id),
                        Task::Flags(id),
                        Task::Defined(id, true),
                        Task::Defined(id, false),
                        Task::String(StringTarget::Source(id), None),
                    ]);
                }
                Task::Defined(id, last) => {
                    let line = count!() as u32;
                    self.chunk.prototypes[id].defined[usize::from(last)] = line;
                    self.tasks.pop();
                }
                Task::Flags(id) => {
                    let bytes = read!(4);
                    let p = &mut self.chunk.prototypes[id];
                    p.nups = bytes[0];
                    p.params = bytes[1];
                    p.vararg = bytes[2];
                    p.stack = bytes[3];
                    self.tasks.pop();
                }
                Task::CodeCount(id)
                | Task::ConstantsCount(id)
                | Task::ChildrenCount(id)
                | Task::LinesCount(id)
                | Task::LocalsCount(id)
                | Task::NamesCount(id) => {
                    let n = count!();
                    *self.tasks.last_mut().unwrap() = match task {
                        Task::CodeCount(_) => Task::Code(id, n),
                        Task::ConstantsCount(_) => Task::Constants(id, n),
                        Task::ChildrenCount(_) => Task::Children(id, n),
                        Task::LinesCount(_) => Task::Lines(id, n),
                        Task::LocalsCount(_) => Task::Locals(id, n),
                        _ => Task::Names(id, n),
                    };
                }
                Task::Code(_, 0)
                | Task::Constants(_, 0)
                | Task::Children(_, 0)
                | Task::Lines(_, 0)
                | Task::Locals(_, 0)
                | Task::Names(_, 0) => {
                    self.tasks.pop();
                }
                Task::Code(id, n) => {
                    let word = u32::from_ne_bytes(read!(4).try_into().unwrap());
                    self.chunk.prototypes[id].code.push(word);
                    *self.tasks.last_mut().unwrap() = Task::Code(id, n - 1);
                }
                Task::Constants(id, n) => {
                    let kind = read!(1)[0];
                    *self.tasks.last_mut().unwrap() = Task::Constants(id, n - 1);
                    match kind {
                        0 => self.chunk.prototypes[id].constants.push(Constant::Nil),
                        1 | 3 => self.tasks.push(Task::Constant(id, kind)),
                        4 => self
                            .tasks
                            .push(Task::String(StringTarget::Constant(id), None)),
                        _ => return Err("bad constant"),
                    }
                }
                Task::Constant(id, kind) => {
                    let value = if kind == 1 {
                        Constant::Boolean(read!(1)[0] != 0)
                    } else {
                        Constant::Number(f64::from_ne_bytes(read!(8).try_into().unwrap()))
                    };
                    self.chunk.prototypes[id].constants.push(value);
                    self.tasks.pop();
                }
                Task::Children(id, n) => {
                    *self.tasks.last_mut().unwrap() = Task::Children(id, n - 1);
                    self.tasks.push(Task::Function(Some(id)));
                }
                Task::Lines(id, n) => {
                    let line = u32::from_ne_bytes(read!(4).try_into().unwrap());
                    self.chunk.prototypes[id].lines.push(line);
                    *self.tasks.last_mut().unwrap() = Task::Lines(id, n - 1);
                }
                Task::Locals(id, n) => {
                    self.chunk.prototypes[id].locals.push(Local::default());
                    *self.tasks.last_mut().unwrap() = Task::Locals(id, n - 1);
                    self.tasks.extend([
                        Task::LocalBounds(id),
                        Task::String(StringTarget::Local(id), None),
                    ]);
                }
                Task::LocalBounds(id) => {
                    // Keep each integer separate so a negative first bound is
                    // rejected without requesting bytes for the second one.
                    let start = count!() as u32;
                    self.chunk.prototypes[id].locals.last_mut().unwrap().start = start;
                    *self.tasks.last_mut().unwrap() = Task::LocalBoundsEnd(id);
                }
                Task::LocalBoundsEnd(id) => {
                    let end = count!() as u32;
                    self.chunk.prototypes[id].locals.last_mut().unwrap().end = end;
                    self.tasks.pop();
                }
                Task::Names(id, n) => {
                    *self.tasks.last_mut().unwrap() = Task::Names(id, n - 1);
                    self.tasks
                        .push(Task::String(StringTarget::Upvalue(id), None));
                }
                Task::String(target, None) => {
                    let size = usize::from_ne_bytes(
                        read!(std::mem::size_of::<usize>()).try_into().unwrap(),
                    );
                    *self.tasks.last_mut().unwrap() = Task::String(target, Some(size));
                }
                Task::String(target, Some(size)) => {
                    let value = if size == 0 {
                        None
                    } else {
                        Some(read!(size)[..size - 1].into())
                    };
                    match target {
                        StringTarget::Source(id) => self.chunk.prototypes[id].source = value,
                        StringTarget::Constant(id) => self.chunk.prototypes[id]
                            .constants
                            .push(Constant::String(value.ok_or("bad constant")?)),
                        StringTarget::Local(id) => {
                            self.chunk.prototypes[id].locals.last_mut().unwrap().name = value
                        }
                        StringTarget::Upvalue(id) => self.chunk.prototypes[id].names.push(value),
                    }
                    self.tasks.pop();
                }
                Task::Verify(id) => {
                    verify(&self.chunk, id)?;
                    self.tasks.pop();
                    self.depth -= 1;
                }
            }
        }
        Ok(true)
    }
}

pub(crate) fn diagnostic(name: &[u8], why: &str) -> Error {
    let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
    let name = if matches!(name.first(), Some(b'@' | b'=')) {
        &name[1..]
    } else if name.first() == Some(&27) {
        b"binary string"
    } else {
        name
    };
    let mut message = name.to_vec();
    message.extend_from_slice(b": ");
    message.extend_from_slice(why.as_bytes());
    message.extend_from_slice(b" in precompiled chunk");
    Error::from_bytes(ErrorKind::Syntax, 0, message)
}
impl Decoder {
    pub fn program(&mut self, name: &[u8]) -> Result<crate::Program, Error> {
        let chunk = std::mem::take(&mut self.chunk);
        translate::program(chunk).map_err(|why| diagnostic(name, why))
    }
}
pub(crate) fn compile(source: &[u8], name: &[u8], depth: usize) -> Result<crate::Program, Error> {
    let mut decoder = Decoder::new(depth);
    decoder
        .resume(source, true)
        .map_err(|why| diagnostic(name, why))?;
    decoder.program(name)
}

mod dump;
mod translate;
pub(crate) use dump::dump;
pub(crate) use dump::dump_blocks;
mod verify;
use verify::verify;

#[cfg(test)]
mod tests;
