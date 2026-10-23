use super::{CompileError, Literal};

pub(super) struct Prototype {
    pub upvalues: usize,
    pub parameters: usize,
    pub registers: usize,
    pub code: Vec<u32>,
    pub constants: Vec<Literal>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    little: bool,
    size_t: usize,
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8], CompileError> {
        let end = self.at.checked_add(count).ok_or(CompileError::InvalidChunk)?;
        let bytes = self.bytes.get(self.at..end).ok_or(CompileError::InvalidChunk)?;
        self.at = end;
        Ok(bytes)
    }

    fn byte(&mut self) -> Result<u8, CompileError> {
        Ok(self.take(1)?[0])
    }

    fn uint(&mut self, width: usize) -> Result<u64, CompileError> {
        let little = self.little;
        let bytes = self.take(width)?;
        Ok(if little {
            bytes.iter().rev().fold(0, |n, b| (n << 8) | u64::from(*b))
        } else {
            bytes.iter().fold(0, |n, b| (n << 8) | u64::from(*b))
        })
    }

    fn count(&mut self) -> Result<usize, CompileError> {
        let n = self.uint(4)?;
        if n > i32::MAX as u64 || n as usize > self.bytes.len() - self.at {
            return Err(CompileError::InvalidChunk);
        }
        Ok(n as usize)
    }

    fn string(&mut self) -> Result<Option<Box<[u8]>>, CompileError> {
        let n = usize::try_from(self.uint(self.size_t)?).map_err(|_| CompileError::InvalidChunk)?;
        if n == 0 {
            return Ok(None);
        }
        let bytes = self.take(n)?;
        if bytes.last() != Some(&0) {
            return Err(CompileError::InvalidChunk);
        }
        Ok(Some(bytes[..n - 1].into()))
    }

    fn skip_string(&mut self) -> Result<(), CompileError> {
        let n = usize::try_from(self.uint(self.size_t)?).map_err(|_| CompileError::InvalidChunk)?;
        if n != 0 && self.take(n)?.last() != Some(&0) {
            return Err(CompileError::InvalidChunk);
        }
        Ok(())
    }
}

pub(super) fn decode(bytes: &[u8]) -> Result<Prototype, CompileError> {
    let mut r = Reader {
        bytes,
        at: 0,
        little: true,
        size_t: 8,
    };
    if r.take(6)? != b"\x1bLua\x51\0" {
        return Err(CompileError::InvalidChunk);
    }
    r.little = match r.byte()? {
        0 => false,
        1 => true,
        _ => return Err(CompileError::InvalidChunk),
    };
    if r.byte()? != 4 {
        return Err(CompileError::InvalidChunk);
    }
    r.size_t = match r.byte()? {
        4 => 4,
        8 => 8,
        _ => return Err(CompileError::InvalidChunk),
    };
    if r.take(3)? != [4, 8, 0] {
        return Err(CompileError::InvalidChunk);
    }
    r.skip_string()?;
    r.take(8)?; // Source line range.
    let upvalues = usize::from(r.byte()?);
    let parameters = usize::from(r.byte()?);
    if r.byte()? != 0 {
        return Err(CompileError::Unsupported {
            pc: 0,
            reason: "varargs",
        });
    }
    let registers = usize::from(r.byte()?);
    if !(1..=250).contains(&registers) || parameters > registers {
        return Err(CompileError::InvalidChunk);
    }
    let count = r.count()?;
    if count == 0 || count > 4096 {
        return Err(CompileError::Unsupported {
            pc: 0,
            reason: "instruction count",
        });
    }
    let mut code = Vec::with_capacity(count);
    for _ in 0..count {
        code.push(r.uint(4)? as u32);
    }
    let count = r.count()?;
    if count > 4096 {
        return Err(CompileError::Unsupported {
            pc: 0,
            reason: "constant count",
        });
    }
    let mut constants = Vec::with_capacity(count);
    for _ in 0..count {
        constants.push(match r.byte()? {
            0 => Literal::Nil,
            1 => Literal::Boolean(match r.byte()? {
                0 => false,
                1 => true,
                _ => return Err(CompileError::InvalidChunk),
            }),
            3 => Literal::Number(f64::from_bits(r.uint(8)?)),
            4 => Literal::Bytes(r.string()?.ok_or(CompileError::InvalidChunk)?),
            _ => return Err(CompileError::InvalidChunk),
        });
    }
    if r.count()? != 0 {
        return Err(CompileError::Unsupported {
            pc: 0,
            reason: "nested closures",
        });
    }
    let lines = r.count()?;
    r.take(lines.checked_mul(4).ok_or(CompileError::InvalidChunk)?)?;
    for _ in 0..r.count()? {
        r.skip_string()?;
        r.take(8)?;
    }
    for _ in 0..r.count()? {
        r.skip_string()?;
    }
    if r.at != bytes.len() {
        return Err(CompileError::InvalidChunk);
    }
    Ok(Prototype {
        upvalues,
        parameters,
        registers,
        code,
        constants,
    })
}
