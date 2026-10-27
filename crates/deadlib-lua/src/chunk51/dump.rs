//! Native Lua 5.1 serialization with an explicit prototype traversal stack.
use super::Wire;
use crate::{
    Error, ErrorKind,
    bytecode::{Constant, ProgramData, Prototype},
};

mod jumps;
mod lower;
use lower::Lowered;

fn fail() -> Error {
    Error::new(ErrorKind::Runtime, 0, "unable to dump given function")
}
struct Output {
    bytes: Vec<u8>,
    ends: Option<Vec<usize>>,
}
impl Output {
    fn end(&mut self) {
        if let Some(ends) = &mut self.ends {
            ends.push(self.bytes.len());
        }
    }
    fn block(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
        self.end();
    }
    fn integer(&mut self, n: usize) -> Result<(), Error> {
        let n = i32::try_from(n).map_err(|_| fail())?;
        self.block(&n.to_ne_bytes());
        Ok(())
    }
    fn string(&mut self, value: Option<&[u8]>) {
        self.block(&value.map_or(0, |s| s.len() + 1).to_ne_bytes());
        if let Some(value) = value {
            self.bytes.extend_from_slice(value);
            self.bytes.push(0);
            self.end();
        }
    }
}
enum Body<'a> {
    Native(&'a Wire),
    Source(Lowered),
}
impl Body<'_> {
    fn code(&self) -> &[u32] {
        match self {
            Self::Native(p) => &p.code,
            Self::Source(p) => &p.code,
        }
    }
    fn children(&self) -> &[usize] {
        match self {
            Self::Native(p) => &p.children,
            Self::Source(p) => &p.children,
        }
    }
    fn lines(&self) -> &[u32] {
        match self {
            Self::Native(p) => &p.lines,
            Self::Source(p) => &p.lines,
        }
    }
}
enum Task<'a> {
    Function(usize, Option<&'a [u8]>, usize),
    Debug(&'a Prototype, Body<'a>),
}

pub(crate) fn dump(data: &ProgramData, entry: usize) -> Result<Vec<u8>, Error> {
    Ok(serialize(data, entry, false)?.bytes)
}
pub(crate) fn dump_blocks(
    data: &ProgramData,
    entry: usize,
) -> Result<(Vec<u8>, Vec<usize>), Error> {
    let output = serialize(data, entry, true)?;
    Ok((output.bytes, output.ends.unwrap()))
}
fn serialize(data: &ProgramData, entry: usize, blocks: bool) -> Result<Output, Error> {
    let mut output = Output {
        bytes: Vec::with_capacity(256),
        ends: blocks.then(Vec::new),
    };
    output.block(&super::header());
    let mut tasks = vec![Task::Function(entry, None, 1)];
    while let Some(task) = tasks.pop() {
        match task {
            Task::Function(id, parent, depth) => {
                if depth > super::DEPTH_LIMIT {
                    return Err(fail());
                }
                let p = data.prototypes.get(id).ok_or_else(fail)?;
                let body = match p.debug.native.as_ref() {
                    Some(native) => Body::Native(&native.wire),
                    None => Body::Source(lower::lower(data, p)?),
                };
                let source = data.source(p);
                output.string(if parent == Some(source) {
                    None
                } else {
                    Some(source)
                });
                for line in p.debug.defined {
                    output.block(&line.to_ne_bytes());
                }
                let (stack, vararg, constants) = match &body {
                    Body::Native(wire) => (
                        p.debug.native.as_ref().unwrap().stack as u8,
                        wire.vararg,
                        wire.constants,
                    ),
                    Body::Source(lowered) => (
                        lowered.stack,
                        (u8::from(p.vararg) * 2) | (u8::from(p.compat_arg) * 5),
                        p.constants.len(),
                    ),
                };
                for flag in [p.upvalues.len() as u8, p.params as u8, vararg, stack] {
                    output.block(&[flag]);
                }
                output.integer(body.code().len())?;
                for word in body.code() {
                    output.bytes.extend_from_slice(&word.to_ne_bytes());
                }
                output.end();
                output.integer(constants)?;
                for k in &p.constants[..constants] {
                    match k {
                        Constant::Nil => output.block(&[0]),
                        Constant::Boolean(b) => {
                            output.block(&[1]);
                            output.block(&[u8::from(*b)]);
                        }
                        Constant::Number(n) => {
                            output.block(&[3]);
                            output.block(&n.to_ne_bytes());
                        }
                        Constant::String(s) => {
                            output.block(&[4]);
                            output.string(Some(s));
                        }
                    }
                }
                output.integer(body.children().len())?;
                let children = body.children().to_vec();
                tasks.push(Task::Debug(p, body));
                tasks.extend(
                    children
                        .into_iter()
                        .rev()
                        .map(|child| Task::Function(child, Some(source), depth + 1)),
                );
            }
            Task::Debug(p, body) => {
                output.integer(body.lines().len())?;
                for line in body.lines() {
                    output.bytes.extend_from_slice(&line.to_ne_bytes());
                }
                output.end();
                let remapped = match &body {
                    Body::Source(lowered) => lowered.locals.as_deref(),
                    _ => None,
                };
                let count = remapped.map_or(p.debug.locals.len(), <[_]>::len);
                output.integer(count)?;
                for index in 0..count {
                    let local =
                        &p.debug.locals[remapped.map_or(index, |ranges| ranges[index].index)];
                    output.string(Some(&local.name));
                    let bounds = match (&body, remapped) {
                        (Body::Native(wire), _) => wire.locals[index],
                        (Body::Source(_), Some(ranges)) => [ranges[index].start, ranges[index].end],
                        (Body::Source(lowered), None) => [
                            lowered.pcs[local.start as usize],
                            lowered.pcs[local.end as usize],
                        ],
                    };
                    for bound in bounds {
                        output.block(&bound.to_ne_bytes());
                    }
                }
                let count = match &body {
                    Body::Native(wire) => wire.names,
                    Body::Source(_) => p.debug.upvalues.len(),
                };
                output.integer(count)?;
                for name in &p.debug.upvalues[..count] {
                    output.string(Some(name));
                }
            }
        }
    }
    Ok(output)
}
