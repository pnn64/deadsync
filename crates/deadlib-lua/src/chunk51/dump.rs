//! Native Lua 5.1 serialization with an explicit prototype traversal stack.
use super::Wire;
use crate::{
    Error, ErrorKind,
    bytecode::{Constant, ProgramData, Prototype},
};

mod lower;
use lower::Lowered;

fn fail() -> Error {
    Error::new(ErrorKind::Runtime, 0, "unable to dump given function")
}
fn integer(bytes: &mut Vec<u8>, n: usize) -> Result<(), Error> {
    let n = i32::try_from(n).map_err(|_| fail())?;
    bytes.extend_from_slice(&n.to_ne_bytes());
    Ok(())
}
fn string(bytes: &mut Vec<u8>, value: Option<&[u8]>) {
    bytes.extend_from_slice(&value.map_or(0, |s| s.len() + 1).to_ne_bytes());
    if let Some(value) = value {
        bytes.extend_from_slice(value);
        bytes.push(0);
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
    let mut bytes = Vec::with_capacity(256);
    bytes.extend_from_slice(&super::header());
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
                string(
                    &mut bytes,
                    if parent == Some(source) {
                        None
                    } else {
                        Some(source)
                    },
                );
                for line in p.debug.defined {
                    bytes.extend_from_slice(&line.to_ne_bytes());
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
                bytes.extend_from_slice(&[p.upvalues.len() as u8, p.params as u8, vararg, stack]);
                integer(&mut bytes, body.code().len())?;
                for word in body.code() {
                    bytes.extend_from_slice(&word.to_ne_bytes());
                }
                integer(&mut bytes, constants)?;
                for k in &p.constants[..constants] {
                    match k {
                        Constant::Nil => bytes.push(0),
                        Constant::Boolean(b) => bytes.extend_from_slice(&[1, u8::from(*b)]),
                        Constant::Number(n) => {
                            bytes.push(3);
                            bytes.extend_from_slice(&n.to_ne_bytes());
                        }
                        Constant::String(s) => {
                            bytes.push(4);
                            string(&mut bytes, Some(s));
                        }
                    }
                }
                integer(&mut bytes, body.children().len())?;
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
                integer(&mut bytes, body.lines().len())?;
                for line in body.lines() {
                    bytes.extend_from_slice(&line.to_ne_bytes());
                }
                integer(&mut bytes, p.debug.locals.len())?;
                for (index, local) in p.debug.locals.iter().enumerate() {
                    string(&mut bytes, Some(&local.name));
                    let bounds = match &body {
                        Body::Native(wire) => wire.locals[index],
                        Body::Source(lowered) => [
                            lowered.pcs[local.start as usize],
                            lowered.pcs[local.end as usize],
                        ],
                    };
                    for bound in bounds {
                        bytes.extend_from_slice(&bound.to_ne_bytes());
                    }
                }
                let count = match &body {
                    Body::Native(wire) => wire.names,
                    Body::Source(_) => p.debug.upvalues.len(),
                };
                integer(&mut bytes, count)?;
                for name in &p.debug.upvalues[..count] {
                    string(&mut bytes, Some(name));
                }
            }
        }
    }
    Ok(bytes)
}
