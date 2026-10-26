//! Little-endian, bounded serialization of the shared compiler representation.
use crate::{Program, bytecode::*, chunk51::Wire};
use std::{mem::size_of, sync::Arc};

// Bump for a wire-layout change. Cargo's build identity also invalidates entries
// after compiler/runtime changes without depending on the package version.
const HEADER: &[u8] = b"DeadLuaCache\0\x01\x51";
const OPS: [Op; 40] = [
    Op::Load,
    Op::Move,
    Op::Bool,
    Op::GetGlobal,
    Op::SetGlobal,
    Op::NewTable,
    Op::GetTable,
    Op::SetTable,
    Op::Add,
    Op::Sub,
    Op::Mul,
    Op::Div,
    Op::Mod,
    Op::Pow,
    Op::Concat,
    Op::Eq,
    Op::Ne,
    Op::Lt,
    Op::Le,
    Op::Gt,
    Op::Ge,
    Op::Neg,
    Op::Not,
    Op::Len,
    Op::Jump,
    Op::JumpFalse,
    Op::JumpTrue,
    Op::ForPrep,
    Op::ForLoop,
    Op::Call,
    Op::Return,
    Op::Closure,
    Op::GetUpvalue,
    Op::SetUpvalue,
    Op::Close,
    Op::Vararg,
    Op::SetList,
    Op::TailCall,
    Op::CallIter,
    Op::ConcatRange,
];

const fn crc_table() -> [[u32; 256]; 8] {
    let mut table = [[0; 256]; 8];
    let mut i = 0;
    while i < 256 {
        let mut value = i as u32;
        let mut bit = 0;
        while bit < 8 {
            value = (value >> 1) ^ (0x82f63b78u32.wrapping_mul(value & 1));
            bit += 1;
        }
        table[0][i] = value;
        i += 1;
    }
    let mut slice = 1;
    while slice < 8 {
        let mut i = 0;
        while i < 256 {
            let previous = table[slice - 1][i];
            table[slice][i] = (previous >> 8) ^ table[0][previous as u8 as usize];
            i += 1;
        }
        slice += 1;
    }
    table
}
static CRC: [[u32; 256]; 8] = crc_table();
pub(super) fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    let (words, tail) = bytes.as_chunks::<8>();
    for word in words {
        // Eight byte contributions have independent lookups. Only the next
        // complete block depends on the previous CRC; loads are safe/unaligned.
        let low = crc ^ u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
        crc = CRC[7][low as u8 as usize]
            ^ CRC[6][(low >> 8) as u8 as usize]
            ^ CRC[5][(low >> 16) as u8 as usize]
            ^ CRC[4][(low >> 24) as usize]
            ^ CRC[3][word[4] as usize]
            ^ CRC[2][word[5] as usize]
            ^ CRC[1][word[6] as usize]
            ^ CRC[0][word[7] as usize];
    }
    for &byte in tail {
        crc = (crc >> 8) ^ CRC[0][((crc as u8) ^ byte) as usize];
    }
    !crc
}

struct Writer {
    bytes: Vec<u8>,
    limit: usize,
}
impl Writer {
    fn put(&mut self, bytes: &[u8]) -> Option<()> {
        let len = self.bytes.len().checked_add(bytes.len())?;
        if len > self.limit {
            return None;
        }
        if len > self.bytes.capacity() {
            let capacity = self
                .bytes
                .capacity()
                .saturating_mul(2)
                .max(len)
                .min(self.limit);
            self.bytes.reserve_exact(capacity - self.bytes.len());
        }
        self.bytes.extend_from_slice(bytes);
        Some(())
    }
    fn u8(&mut self, n: u8) -> Option<()> {
        self.put(&[n])
    }
    fn u16(&mut self, n: u16) -> Option<()> {
        self.put(&n.to_le_bytes())
    }
    fn u32(&mut self, n: u32) -> Option<()> {
        self.put(&n.to_le_bytes())
    }
    fn u64(&mut self, n: u64) -> Option<()> {
        self.put(&n.to_le_bytes())
    }
    fn count(&mut self, n: usize) -> Option<()> {
        self.u32(n.try_into().ok()?)
    }
    fn bytes(&mut self, bytes: &[u8]) -> Option<()> {
        self.count(bytes.len())?;
        self.put(bytes)
    }
    fn list<T>(
        &mut self,
        values: &[T],
        mut write: impl FnMut(&mut Self, &T) -> Option<()>,
    ) -> Option<()> {
        self.count(values.len())?;
        for value in values {
            write(self, value)?;
        }
        Some(())
    }
    fn words(&mut self, values: &[u32]) -> Option<()> {
        self.list(values, |w, &n| w.u32(n))
    }
    fn upvalues(&mut self, values: &[Upvalue]) -> Option<()> {
        self.list(values, |w, value| {
            let (tag, n) = match value {
                Upvalue::Local(n) => (0, u16::from(*n)),
                Upvalue::Outer(n) => (1, *n),
            };
            w.u8(tag)?;
            w.u16(n)
        })
    }
    fn prototype(&mut self, p: &Prototype) -> Option<()> {
        self.u16(p.registers)?;
        self.u16(p.params)?;
        self.u8(u8::from(p.vararg))?;
        self.u8(u8::from(p.compat_arg))?;
        self.upvalues(&p.upvalues)?;
        self.list(&p.code, |w, i| {
            if OPS.get(i.op as usize) != Some(&i.op) {
                return None;
            }
            w.u8(i.op as u8)?;
            w.u8(i.a)?;
            w.u16(i.b)?;
            w.u32(i.c)
        })?;
        self.words(&p.lines)?;
        self.words(&p.gc_live)?;
        self.list(&p.gc_maps, |w, mask| {
            for &word in mask {
                w.u64(word)?;
            }
            Some(())
        })?;
        self.list(&p.constants, |w, k| match k {
            Constant::Nil => w.u8(0),
            Constant::Boolean(value) => {
                w.u8(1)?;
                w.u8(u8::from(*value))
            }
            Constant::Number(value) => {
                w.u8(2)?;
                w.u64(value.to_bits())
            }
            Constant::String(value) => {
                w.u8(3)?;
                w.bytes(value)
            }
        })?;
        self.list(&p.bindings, |w, binding| {
            w.u32(binding.prototype)?;
            w.upvalues(&binding.upvalues)
        })?;
        let d = &p.debug;
        for n in d.defined {
            self.u32(n)?;
        }
        self.list(&d.upvalues, |w, name| w.bytes(name))?;
        self.list(&d.locals, |w, local| {
            w.bytes(&local.name)?;
            w.u32(local.start)?;
            w.u32(local.end)?;
            w.u16(local.register)
        })?;
        self.list(&d.calls, |w, call| {
            w.u32(call.pc)?;
            w.bytes(&call.name)?;
            w.u8(match call.kind {
                NameKind::Unknown => 0,
                NameKind::Local => 1,
                NameKind::Global => 2,
                NameKind::Upvalue => 3,
                NameKind::Field => 4,
                NameKind::Method => 5,
            })?;
            w.u32(call.temporaries.start)?;
            w.u32(call.temporaries.end)
        })?;
        self.words(&d.active_lines)?;
        self.words(&d.hook_lines)?;
        self.bytes(&d.temporaries)?;
        self.u8(u8::from(d.native.is_some()))?;
        if let Some(n) = &d.native {
            self.u32(n.source)?;
            self.u16(n.stack)?;
            self.u8(u8::from(n.has_lines))?;
            self.words(&n.wire.code)?;
            self.words(&n.wire.lines)?;
            self.list(&n.wire.locals, |w, bounds| {
                w.u32(bounds[0])?;
                w.u32(bounds[1])
            })?;
            self.list(&n.wire.children, |w, &id| w.count(id))?;
            self.count(n.wire.constants)?;
            self.count(n.wire.names)?;
            self.u8(n.wire.vararg)?;
        }
        Some(())
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    remaining: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.position.checked_add(n)?;
        let bytes = self.bytes.get(self.position..end)?;
        self.position = end;
        Some(bytes)
    }
    fn charge(&mut self, bytes: usize) -> Option<()> {
        self.remaining = self.remaining.checked_sub(bytes)?;
        Some(())
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn boolean(&mut self) -> Option<bool> {
        match self.u8()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
    fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }
    fn boxed(&mut self) -> Option<Box<[u8]>> {
        let bytes = self.bytes()?;
        self.charge(bytes.len())?;
        Some(bytes.into())
    }
    fn list<T>(
        &mut self,
        minimum: usize,
        mut read: impl FnMut(&mut Self) -> Option<T>,
    ) -> Option<Box<[T]>> {
        let n = self.u32()? as usize;
        if n > (self.bytes.len() - self.position) / minimum {
            return None;
        }
        self.charge(n.checked_mul(size_of::<T>())?)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(read(self)?);
        }
        Some(out.into_boxed_slice())
    }
    fn words(&mut self) -> Option<Box<[u32]>> {
        self.list(4, Self::u32)
    }
    fn upvalues(&mut self) -> Option<Box<[Upvalue]>> {
        self.list(3, |r| {
            let tag = r.u8()?;
            let n = r.u16()?;
            match tag {
                0 => Some(Upvalue::Local(n.try_into().ok()?)),
                1 => Some(Upvalue::Outer(n)),
                _ => None,
            }
        })
    }
    fn prototype(&mut self) -> Option<Prototype> {
        let registers = self.u16()?;
        let params = self.u16()?;
        let vararg = self.boolean()?;
        let compat_arg = self.boolean()?;
        let upvalues = self.upvalues()?;
        let code = self.list(8, |r| {
            Some(Instruction {
                op: *OPS.get(r.u8()? as usize)?,
                a: r.u8()?,
                b: r.u16()?,
                c: r.u32()?,
            })
        })?;
        let lines = self.words()?;
        let gc_live = self.words()?;
        let gc_maps = self.list(32, |r| Some([r.u64()?, r.u64()?, r.u64()?, r.u64()?]))?;
        let constants = self.list(1, |r| match r.u8()? {
            0 => Some(Constant::Nil),
            1 => Some(Constant::Boolean(r.boolean()?)),
            2 => Some(Constant::Number(f64::from_bits(r.u64()?))),
            3 => Some(Constant::String(r.boxed()?)),
            _ => None,
        })?;
        let bindings = self.list(8, |r| {
            Some(ClosureBinding {
                prototype: r.u32()?,
                upvalues: r.upvalues()?,
            })
        })?;
        let defined = [self.u32()?, self.u32()?];
        let names = self.list(4, Self::boxed)?;
        let locals = self.list(14, |r| {
            Some(DebugLocal {
                name: r.boxed()?,
                start: r.u32()?,
                end: r.u32()?,
                register: r.u16()?,
            })
        })?;
        let calls = self.list(17, |r| {
            let pc = r.u32()?;
            let name = r.boxed()?;
            let kind = match r.u8()? {
                0 => NameKind::Unknown,
                1 => NameKind::Local,
                2 => NameKind::Global,
                3 => NameKind::Upvalue,
                4 => NameKind::Field,
                5 => NameKind::Method,
                _ => return None,
            };
            Some(DebugCall {
                pc,
                name,
                kind,
                temporaries: r.u32()?..r.u32()?,
            })
        })?;
        let active_lines = self.words()?;
        let hook_lines = self.words()?;
        let temporaries = self.boxed()?;
        let native = if self.boolean()? {
            self.charge(size_of::<NativeDebug>())?;
            Some(Box::new(NativeDebug {
                source: self.u32()?,
                stack: self.u16()?,
                has_lines: self.boolean()?,
                wire: Wire {
                    code: self.words()?,
                    lines: self.words()?,
                    locals: self.list(8, |r| Some([r.u32()?, r.u32()?]))?,
                    children: self.list(4, |r| Some(r.u32()? as usize))?,
                    constants: self.u32()? as usize,
                    names: self.u32()? as usize,
                    vararg: self.u8()?,
                },
            }))
        } else {
            None
        };
        Some(Prototype {
            registers,
            params,
            vararg,
            compat_arg,
            upvalues,
            code,
            lines,
            gc_live,
            gc_maps,
            constants,
            bindings,
            debug: DebugInfo {
                native,
                defined,
                upvalues: names,
                locals,
                calls,
                active_lines,
                hook_lines,
                temporaries,
            },
        })
    }
}

fn decoded_bytes(data: &ProgramData) -> usize {
    let mut bytes = size_of::<ProgramData>()
        + 4 * size_of::<usize>()
        + data.name.len()
        + size_of_val(data.sources.as_ref())
        + data.sources.iter().map(|s| s.len()).sum::<usize>()
        + size_of_val(data.prototypes.as_ref());
    for p in &data.prototypes {
        bytes += size_of_val(p.code.as_ref())
            + size_of_val(p.lines.as_ref())
            + size_of_val(p.gc_live.as_ref())
            + size_of_val(p.gc_maps.as_ref())
            + size_of_val(p.upvalues.as_ref())
            + size_of_val(p.constants.as_ref())
            + size_of_val(p.bindings.as_ref())
            + p.debug.heap_bytes()
            + p.bindings
                .iter()
                .map(|b| size_of_val(b.upvalues.as_ref()))
                .sum::<usize>()
            + p.constants
                .iter()
                .map(|k| {
                    if let Constant::String(s) = k {
                        s.len()
                    } else {
                        0
                    }
                })
                .sum::<usize>();
    }
    if data.prototypes.iter().any(|p| p.debug.native.is_some()) {
        bytes += data.prototypes.len();
    }
    bytes
}

pub(super) fn encode(
    program: &Program,
    source: &[u8],
    name: &[u8],
    build: &str,
    context: &[u8; 128],
    limit: usize,
    decoded_limit: usize,
) -> Option<Vec<u8>> {
    if decoded_bytes(&program.data) > decoded_limit {
        return None;
    }
    let mut w = Writer {
        bytes: Vec::with_capacity(limit.min(4096)),
        limit,
    };
    w.put(HEADER)?;
    w.bytes(std::env::consts::OS.as_bytes())?;
    w.bytes(std::env::consts::ARCH.as_bytes())?;
    w.u8(size_of::<usize>() as u8)?;
    w.u8(u8::from(cfg!(target_endian = "little")))?;
    w.bytes(build.as_bytes())?;
    w.put(context)?;
    w.bytes(source)?;
    w.bytes(name)?;
    w.u32(program.data.entry)?;
    w.bytes(&program.data.name)?;
    w.list(&program.data.sources, |w, s| w.bytes(s))?;
    w.list(&program.data.prototypes, Writer::prototype)?;
    w.u32(checksum(&w.bytes))?;
    Some(w.bytes)
}

pub(super) fn decode(
    bytes: &[u8],
    source: &[u8],
    name: &[u8],
    build: &str,
    context: &[u8; 128],
    limit: usize,
) -> Option<Program> {
    let end = bytes.len().checked_sub(4)?;
    if checksum(&bytes[..end]) != u32::from_le_bytes(bytes[end..].try_into().ok()?) {
        return None;
    }
    let mut r = Reader {
        bytes: &bytes[..end],
        position: 0,
        remaining: limit,
    };
    if r.take(HEADER.len())? != HEADER
        || r.bytes()? != std::env::consts::OS.as_bytes()
        || r.bytes()? != std::env::consts::ARCH.as_bytes()
        || r.u8()? as usize != size_of::<usize>()
        || r.boolean()? != cfg!(target_endian = "little")
        || r.bytes()? != build.as_bytes()
        || r.take(context.len())? != context
        || r.bytes()? != source
        || r.bytes()? != name
    {
        return None;
    }
    r.charge(size_of::<ProgramData>() + 4 * size_of::<usize>())?;
    let entry = r.u32()?;
    let program_name = r.boxed()?;
    let sources = r.list(4, Reader::boxed)?;
    let prototypes = r.list(64, Reader::prototype)?;
    if r.position != end {
        return None;
    }
    // Native re-export walks a tree. Validate retained wire ranges and edges in
    // addition to the shared-instruction verifier before exposing the program.
    if prototypes.iter().any(|p| p.debug.native.is_some()) {
        if entry != 0 {
            return None;
        }
        r.charge(prototypes.len())?;
        let mut seen = vec![false; prototypes.len()];
        for (index, p) in prototypes.iter().enumerate() {
            let n = p.debug.native.as_ref()?;
            let w = &n.wire;
            if w.constants > p.constants.len()
                || w.names > p.debug.upvalues.len()
                || w.locals.len() != p.debug.locals.len()
                || w.vararg & !7 != 0
                || (w.vararg & 2 != 0) != p.vararg
                || (w.vararg & 4 != 0) != p.compat_arg
                || n.has_lines == w.lines.is_empty()
                || (!w.lines.is_empty() && w.lines.len() != w.code.len())
                || w.locals
                    .iter()
                    .any(|b| b[0] > b[1] || b[1] as usize > w.code.len())
            {
                return None;
            }
            for &child in &w.children {
                if child <= index || *seen.get(child)? {
                    return None;
                }
                seen[child] = true;
            }
        }
        if seen.first() != Some(&false) || seen[1..].contains(&false) {
            return None;
        }
    }
    let program = Program {
        lifetime: Arc::new(()),
        data: Arc::new(ProgramData {
            prototypes,
            entry,
            name: program_name,
            sources,
        }),
    };
    program.verify().ok()?;
    Some(program)
}
