//! Calls stay in the same C runtime that exported the module's Lua API.
use super::*;
use crate::vm::io::native::{File, FileBackend};

type Operation = unsafe extern "C" fn(c_int, *mut c_void, *mut BoundaryValue) -> c_int;
#[derive(Clone, Debug)]
pub(super) struct Backend {
    operation: Operation,
    _library: Arc<Library>,
}
impl Backend {
    pub(super) fn new(library: Arc<Library>) -> Result<Self, Box<[u8]>> {
        // SAFETY: this private export belongs to the checked protocol image.
        let operation = unsafe {
            std::mem::transmute::<usize, Operation>(library.try_symbol(b"boundary_stdio")?)
        };
        Ok(Self {
            operation,
            _library: library,
        })
    }
}
impl FileBackend for Backend {
    fn identity(&self) -> usize {
        self._library.0
    }
    fn attach(&self, pointer: usize) -> Box<dyn File> {
        Box::new(Handle {
            backend: self.clone(),
            pointer,
            owned: false,
            pipe: false,
        })
    }
    fn open(&self, path: Option<&[u8]>, mode: &[u8], pipe: bool) -> Result<Box<dyn File>, i32> {
        let string = |bytes: &[u8]| {
            CString::new(bytes.split(|&b| b == 0).next().unwrap()).expect("C prefix")
        };
        let path = path.map(string);
        let mode = string(mode);
        let mut value = BoundaryValue {
            bytes: path.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
            userdata: mode.as_ptr() as *mut c_void,
            integer: c_int::from(pipe),
            ..Default::default()
        };
        // SAFETY: these C strings remain live through this synchronous factory.
        let code = unsafe { (self.operation)(13, std::ptr::null_mut(), &mut value) };
        if code != 0 {
            return Err(if code == -1 { 0 } else { code });
        }
        Ok(Box::new(Handle {
            backend: self.clone(),
            pointer: value.userdata as usize,
            owned: true,
            pipe,
        }))
    }
}
#[derive(Debug)]
struct Handle {
    backend: Backend,
    pointer: usize,
    owned: bool,
    pipe: bool,
}
impl Handle {
    fn call(&self, op: c_int, value: &mut BoundaryValue) -> Result<(), i32> {
        // SAFETY: file userdata supplies a live FILE pointer under the Lua 5.1
        // C API contract. Borrowed buffers outlive this synchronous primitive.
        let code = unsafe { (self.backend.operation)(op, self.pointer as *mut c_void, value) };
        if code == 0 {
            Ok(())
        } else {
            Err(if code == -1 { 0 } else { code })
        }
    }
}
impl File for Handle {
    fn pointer(&self) -> usize {
        self.pointer
    }
    fn publish(&mut self) {
        self.owned = false;
    }
    fn get(&mut self, peek: bool) -> Result<Option<u8>, i32> {
        let mut value = BoundaryValue::default();
        self.call(c_int::from(peek), &mut value)?;
        Ok((value.integer >= 0).then_some(value.integer as u8))
    }
    fn read(&mut self, bytes: &mut [u8]) -> Result<usize, i32> {
        let mut value = BoundaryValue {
            bytes: bytes.as_mut_ptr().cast(),
            length: bytes.len(),
            ..Default::default()
        };
        self.call(2, &mut value)?;
        Ok(value.length)
    }
    fn number(&mut self) -> Result<Option<f64>, i32> {
        let mut value = BoundaryValue::default();
        self.call(10, &mut value)?;
        Ok((value.integer != 0).then_some(value.number))
    }
    fn line(&mut self, bytes: &mut Vec<u8>) -> Result<bool, i32> {
        let mut buffer = [0; if cfg!(windows) { 512 } else { 8192 }];
        loop {
            let mut value = BoundaryValue {
                bytes: buffer.as_mut_ptr().cast(),
                length: buffer.len(),
                ..Default::default()
            };
            self.call(12, &mut value)?;
            bytes.extend_from_slice(&buffer[..value.length]);
            if value.integer & 1 == 0 || value.integer & 2 != 0 {
                if value.wide_integer != 0 {
                    return Err(if value.wide_integer == -1 {
                        0
                    } else {
                        value.wide_integer as i32
                    });
                }
                return Ok(value.integer & 2 != 0 || !bytes.is_empty());
            }
        }
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), i32> {
        self.call(
            3,
            &mut BoundaryValue {
                bytes: bytes.as_ptr().cast(),
                length: bytes.len(),
                ..Default::default()
            },
        )
    }
    fn flush(&mut self) -> Result<(), i32> {
        self.call(4, &mut BoundaryValue::default())
    }
    fn seek(&mut self, mode: usize, offset: i64) -> Result<i64, i32> {
        let mut value = BoundaryValue {
            integer: mode as c_int,
            wide_integer: offset as isize,
            ..Default::default()
        };
        self.call(5, &mut value)?;
        Ok(value.wide_integer as i64)
    }
    fn buffering(&mut self, mode: u8, size: usize) -> Result<(), i32> {
        self.call(
            6,
            &mut BoundaryValue {
                integer: c_int::from(mode),
                length: size,
                ..Default::default()
            },
        )
    }
    fn clear_error(&mut self) {
        let _ = self.call(9, &mut BoundaryValue::default());
    }
    fn close(&mut self, pipe: bool) -> Result<(), i32> {
        let result = self.call(if pipe { 8 } else { 7 }, &mut BoundaryValue::default());
        self.owned = false;
        result
    }
    fn display(&self) -> Vec<u8> {
        let mut bytes = vec![0; 64];
        let mut value = BoundaryValue {
            bytes: bytes.as_mut_ptr().cast(),
            length: bytes.len(),
            ..Default::default()
        };
        self.call(11, &mut value).expect("file pointer formatting");
        bytes.truncate(value.length);
        bytes
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        if self.owned {
            let _ = self.close(self.pipe);
        }
    }
}
