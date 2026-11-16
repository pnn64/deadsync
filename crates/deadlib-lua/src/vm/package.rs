//! Lua 5.1 module discovery, cached imports and module environments.
use super::*;
use crate::memory::fallible::{self, SliceExt, VecExt};
#[allow(unsafe_code)]
mod paths;
mod returns;

pub(super) const REQUIRE: u64 = 151;
pub(super) const MODULE: u64 = 152;
const SEEALL: u64 = 153;
const PRELOAD: u64 = 154;
const LUA: u64 = 155;
const C: u64 = 156;
const CROOT: u64 = 157;
const LOADLIB: u64 = 158;
const SENTINEL: Value = Value::LightUserdata(u64::MAX);

pub(super) fn contains(id: u64) -> bool {
    (REQUIRE..=LOADLIB).contains(&id)
}

fn runtime(message: &'static str) -> Error {
    Error::runtime_static(message)
}

fn replace(source: &[u8], needle: u8, value: &[u8]) -> Result<Vec<u8>, Error> {
    let mut out = fallible::with_capacity(source.len())?;
    for &byte in source {
        if byte == needle {
            out.try_extend_from_slice(value)?
        } else {
            out.try_push(byte)?
        }
    }
    Ok(out)
}

impl Vm {
    fn module_step(&mut self, mut frame: Frame) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        let base = frame.base;
        let scratch = base + frame.argc();
        if frame.pc == 0 {
            let name = self.package_name(frame)?;
            self.reserve_registers(scratch + 9)?;
            self.resize_registers(self.registers.len().max(scratch + 9));
            let arguments = frame.vararg_start..frame.vararg_start + frame.argc();
            self.registers.copy_within(arguments.clone(), base);
            self.relocate_native_error_functions(arguments.clone(), base);
            self.registers[arguments].fill(Value::Nil);
            self.registers[scratch + 8] = name;
            frame.vararg_start = base;
            frame.pc = 1;
            self.frames[owner] = frame;
            let key = self.try_intern(b"_LOADED")?;
            self.get_table(Value::Table(self.registry), key, scratch)?;
            return Ok(None);
        }
        match frame.pc {
            1 => {
                self.frames[owner].pc = 2;
                self.get_table(
                    self.registers[scratch],
                    self.registers[scratch + 8],
                    scratch + 1,
                )?;
            }
            2 => {
                if matches!(self.registers[scratch + 1], Value::Table(_)) {
                    self.frames[owner].pc = 6
                } else {
                    self.registers[scratch + 2] = Value::Table(self.globals);
                    self.registers[scratch + 4] = Value::Number(0.);
                    self.frames[owner].pc = 3
                }
            }
            3 => {
                let name = self.concat_bytes(self.registers[scratch + 8])?;
                let Value::Number(start) = self.registers[scratch + 4] else {
                    unreachable!("module field cursor")
                };
                let start = start as usize;
                let dot = name[start..]
                    .iter()
                    .position(|&b| b == b'.')
                    .map(|n| start + n);
                let end = dot.unwrap_or(name.len());
                let key = self.try_intern(&name[start..end])?;
                let target = self.registers[scratch + 2];
                let value = self.table(target)?.get(key);
                self.registers[scratch + 4] = Value::Number(dot.map_or(-1., |n| (n + 1) as f64));
                self.frames[owner].pc = 4;
                self.registers[scratch + 1] = match value {
                    Value::Table(_) => value,
                    Value::Nil => Value::Table(self.try_new_table()?),
                    _ => {
                        return Err(self.package_named_error(
                            b"name conflict for module '",
                            self.registers[scratch + 8],
                            b"'",
                        ));
                    }
                };
                if value == Value::Nil {
                    self.set_table(target, key, self.registers[scratch + 1])?
                }
            }
            4 => {
                if self.registers[scratch + 4] == Value::Number(-1.) {
                    self.frames[owner].pc = 5;
                    self.set_table(
                        self.registers[scratch],
                        self.registers[scratch + 8],
                        self.registers[scratch + 1],
                    )?;
                } else {
                    self.registers[scratch + 2] = self.registers[scratch + 1];
                    self.frames[owner].pc = 3
                }
            }
            5 => self.frames[owner].pc = 6,
            6 => {
                self.frames[owner].pc = 7;
                let key = self.try_intern(b"_NAME")?;
                self.get_table(self.registers[scratch + 1], key, scratch + 3)?;
            }
            7 => {
                let initialized = self.registers[scratch + 3] != Value::Nil;
                self.registers[scratch + 3] = Value::Nil;
                if initialized {
                    self.frames[owner].pc = 10
                } else {
                    self.frames[owner].pc = 8;
                    self.registers[scratch + 2] = self.registers[scratch + 1];
                    let key = self.try_intern(b"_M")?;
                    self.set_table(
                        self.registers[scratch + 1],
                        key,
                        self.registers[scratch + 1],
                    )?
                }
            }
            8 => {
                self.frames[owner].pc = 9;
                let key = self.try_intern(b"_NAME")?;
                self.registers[scratch + 2] = self.registers[scratch + 8];
                self.set_table(
                    self.registers[scratch + 1],
                    key,
                    self.registers[scratch + 8],
                )?;
            }
            9 => {
                let name = self.concat_bytes(self.registers[scratch + 8])?;
                let length = name.iter().rposition(|&b| b == b'.').map_or(0, |n| n + 1);
                let package = self.try_intern(&name[..length])?;
                let key = self.try_intern(b"_PACKAGE")?;
                self.registers[scratch + 2] = package;
                self.frames[owner].pc = 10;
                self.set_table(self.registers[scratch + 1], key, package)?;
            }
            10 => {
                let Some(debug::StackEntry::Frame(index)) = self.debug_stack(self.thread, 1) else {
                    return Err(runtime("'module' not called from a Lua function"));
                };
                let FrameKind::Lua(function) = self.frames[index].function else {
                    return Err(runtime("'module' not called from a Lua function"));
                };
                let Value::Table(module) = self.registers[scratch + 1] else {
                    unreachable!("module table")
                };
                self.set_closure_environment(Value::Function(function), module)?;
                self.registers[scratch + 2] = Value::Table(module);
                self.registers[scratch + 5] = Value::Number(1.);
                self.frames[owner].pc = 11;
            }
            11 => {
                let Value::Number(index) = self.registers[scratch + 5] else {
                    unreachable!("module option index")
                };
                if index as usize >= frame.argc() {
                    return self.return_frame(frame, base, 0);
                }
                self.registers[scratch + 6] = self.registers[base + index as usize];
                self.registers[scratch + 7] = self.registers[scratch + 2];
                self.registers[scratch + 5] = Value::Number(index + 1.);
                self.frames[owner].pc = 12;
                self.package_call(scratch + 6, 1, 0)?;
            }
            12 => self.frames[owner].pc = 11,
            _ => unreachable!("module phase"),
        }
        Ok(None)
    }
    fn require_step(&mut self, mut frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let owner = self.frames.len() - 1;
        // The visible C window lives at base. Captured C locals (the name and
        // loader index) stay beyond its largest temporary stack.
        let name_slot = base + 16;
        let index_slot = base + 17;
        match frame.pc {
            0 => {
                let name = self.package_name(frame)?;
                let original = match self.argument(frame.vararg_start - 1, frame.argc(), 0) {
                    value @ Value::String(_) => value,
                    _ => name,
                };
                self.registers[frame.vararg_start..frame.vararg_start + frame.argc()]
                    .fill(Value::Nil);
                self.reserve_registers(base + 18)?;
                self.resize_registers(self.registers.len().max(base + 18));
                self.registers[name_slot] = name;
                self.registers[base] = original;
                frame.vararg_start = base;
                frame.vararg_count = 1;
                frame.pc = 1;
                self.frames[owner] = frame;
                let key = self.try_intern(b"_LOADED")?;
                self.get_table(Value::Table(self.registry), key, base + 1)?;
            }
            1 => {
                frame.pc = 2;
                frame.vararg_count = 2;
                self.frames[owner] = frame;
                self.get_table(
                    self.registers[base + 1],
                    self.registers[name_slot],
                    base + 2,
                )?;
            }
            2 => {
                frame.vararg_count = 3;
                self.frames[owner] = frame;
                if self.registers[base + 2].is_truthy() {
                    if self.registers[base + 2] == SENTINEL {
                        return Err(self.package_named_error(
                            b"loop or previous error loading module '",
                            self.registers[name_slot],
                            b"'",
                        ));
                    }
                    return self.return_frame(frame, base + 2, 1);
                }
                self.frames[owner].pc = 3;
                self.package_field(REQUIRE, b"loaders", base + 3)?;
            }
            3 => {
                if !matches!(self.registers[base + 3], Value::Table(_)) {
                    return Err(runtime("'package.loaders' must be a table"));
                }
                self.registers[base + 4] = self.try_intern(b"")?;
                self.registers[index_slot] = Value::Number(1.);
                frame.pc = 4;
                frame.vararg_count = 5;
                self.frames[owner] = frame;
            }
            4 => {
                let loader = self
                    .table(self.registers[base + 3])?
                    .get(self.registers[index_slot]);
                if loader == Value::Nil {
                    let name = self.concat_bytes(self.registers[name_slot])?;
                    let errors = self.package_error_text(self.registers[base + 4])?;
                    let mut message = self.try_take_buffer(name.len() + errors.len() + 32)?;
                    message.try_extend_from_slice(b"module '")?;
                    message.try_extend_from_slice(&name)?;
                    message.try_extend_from_slice(b"' not found:")?;
                    message.try_extend_from_slice(io::c_bytes(&errors))?;
                    let value = self.try_finish_buffer(message)?;
                    let value = self.try_located_error(value, 1)?;
                    return Err(self.package_raised(value));
                }
                let Value::Number(index) = self.registers[index_slot] else {
                    unreachable!("loader index")
                };
                self.registers[index_slot] = Value::Number(index + 1.);
                self.registers[base + 5] = loader;
                self.registers[base + 6] = self.registers[name_slot];
                self.frames[owner].pc = 5;
                self.package_call(base + 5, 1, 1)?;
            }
            5 => {
                let value = self.registers[base + 5];
                if matches!(value, Value::Function(_) | Value::Native(_)) {
                    self.registers[base + 6] = SENTINEL;
                    frame.pc = 6;
                    frame.vararg_count = 7;
                    self.frames[owner] = frame;
                    self.set_table(
                        self.registers[base + 1],
                        self.registers[name_slot],
                        SENTINEL,
                    )?;
                } else {
                    self.frames[owner].pc = 4;
                    if matches!(value, Value::String(_) | Value::Number(_)) {
                        self.concatenate(self.registers[base + 4], value, base + 4)?;
                    }
                    self.registers[base + 5] = Value::Nil;
                    self.registers[base + 6] = Value::Nil;
                }
            }
            6 => {
                self.registers[base + 6] = self.registers[name_slot];
                frame.pc = 7;
                frame.vararg_count = 5;
                self.frames[owner] = frame;
                self.package_call(base + 5, 1, 1)?;
            }
            7 => {
                frame.vararg_count = 6;
                frame.pc = if self.registers[base + 5] == Value::Nil {
                    11
                } else {
                    8
                };
                self.frames[owner] = frame;
                if frame.pc == 8 {
                    self.set_table(
                        self.registers[base + 1],
                        self.registers[name_slot],
                        self.registers[base + 5],
                    )?;
                }
            }
            8 => {
                self.registers[base + 5] = Value::Nil;
                self.frames[owner].vararg_count = 5;
                self.frames[owner].pc = 11;
            }
            11 => {
                self.frames[owner].pc = 9;
                self.get_table(
                    self.registers[base + 1],
                    self.registers[name_slot],
                    base + frame.argc(),
                )?;
            }
            9 => {
                let result = base + frame.argc();
                frame.vararg_count += 1;
                self.frames[owner] = frame;
                if self.registers[result] != SENTINEL {
                    return self.return_frame(frame, result, 1);
                }
                self.registers[result + 1] = Value::Boolean(true);
                self.registers[result + 2] = Value::Boolean(true);
                frame.pc = 10;
                frame.vararg_count += 2;
                self.frames[owner] = frame;
                self.set_table(
                    self.registers[base + 1],
                    self.registers[name_slot],
                    Value::Boolean(true),
                )?;
            }
            10 => {
                self.registers[base + frame.argc() - 1] = Value::Nil;
                frame.vararg_count -= 1;
                self.frames[owner] = frame;
                return self.return_frame(frame, base + frame.argc() - 1, 1);
            }
            _ => unreachable!("require phase"),
        }
        Ok(None)
    }
    fn package_raised(&self, value: Value) -> Error {
        if matches!(value, Value::String(_)) {
            self.host_error(value)
        } else {
            Error::raised_static(value, "Lua error")
        }
    }
    fn package_error_text(&self, value: Value) -> Result<Vec<u8>, Error> {
        if matches!(value, Value::String(_) | Value::Number(_)) {
            self.concat_bytes(value)
        } else {
            b"(null)".try_to_vec().map_err(|_| self.memory_failure())
        }
    }
    fn preload_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let owner = self.frames.len() - 1;
        match frame.pc {
            0 => {
                self.registers[base] = self.package_name(frame)?;
                self.frames[owner].pc = 1;
                self.package_field(PRELOAD, b"preload", base + 1)?;
            }
            1 => {
                if !matches!(self.registers[base + 1], Value::Table(_)) {
                    return Err(runtime("'package.preload' must be a table"));
                }
                self.frames[owner].pc = 2;
                self.get_table(self.registers[base + 1], self.registers[base], base + 2)?;
            }
            2 => {
                self.registers[base + 3] = self.registers[base + 2];
                if self.registers[base + 2] == Value::Nil {
                    let name = self.concat_bytes(self.registers[base])?;
                    let mut bytes = self.try_take_buffer(name.len() + 40)?;
                    bytes.try_extend_from_slice(b"\n\tno field package.preload['")?;
                    bytes.try_extend_from_slice(&name)?;
                    bytes.try_extend_from_slice(b"']")?;
                    self.registers[base + 2] = self.try_finish_buffer(bytes)?;
                }
                return self.return_frame(frame, base + 2, 1);
            }
            _ => unreachable!("preload phase"),
        }
        Ok(None)
    }
    fn seeall_step(&mut self, mut frame: Frame) -> Result<Option<usize>, Error> {
        if frame.pc == 0 {
            let value = self.argument(frame.vararg_start - 1, frame.argc(), 0);
            let Value::Table(_) = value else {
                return Err(self.package_bad(frame, 0, b"table"));
            };
            let mt = if let Some(mt) = self.metatable(value) {
                mt
            } else {
                let mt = self.try_new_table()?;
                self.set_metatable(value, Some(mt))?;
                mt
            };
            let arguments = frame.vararg_start..frame.vararg_start + frame.argc();
            self.reserve_registers(frame.base + frame.argc() + 2)?;
            self.resize_registers(self.registers.len().max(frame.base + frame.argc() + 2));
            self.registers.copy_within(arguments.clone(), frame.base);
            self.relocate_native_error_functions(arguments.clone(), frame.base);
            self.registers[arguments].fill(Value::Nil);
            self.registers[frame.base + frame.argc()] = Value::Table(mt);
            self.registers[frame.base + frame.argc() + 1] = Value::Table(self.globals);
            frame.vararg_start = frame.base;
            frame.vararg_count += 2;
            frame.pc = 1;
            *self.frames.last_mut().expect("seeall frame") = frame;
            let key = self.try_intern(b"__index")?;
            self.set_table(Value::Table(mt), key, Value::Table(self.globals))?;
            return Ok(None);
        }
        self.registers[frame.base + frame.argc() - 1] = Value::Nil;
        frame.vararg_count -= 1;
        *self.frames.last_mut().expect("seeall frame") = frame;
        self.return_frame(frame, frame.base, 0)
    }
    fn searcher_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let owner = self.frames.len() - 1;
        match frame.pc {
            0 => {
                let name = self.package_name(frame)?;
                let bytes = self.concat_bytes(name)?;
                let search = &bytes;
                let replaced = replace(search, b'.', if cfg!(windows) { b"\\" } else { b"/" })?;
                self.reserve_registers(base + 11)?;
                self.resize_registers(self.registers.len().max(base + 11));
                self.registers[base] = name;
                self.registers[base + 1] = self.try_intern(&replaced)?;
                self.registers[base + 8] = self.registers[base + 1];
                self.frames[owner].pc = 1;
                self.package_field(LUA, b"path", base + 2)?;
            }
            1 => {
                if !matches!(
                    self.registers[base + 2],
                    Value::String(_) | Value::Number(_)
                ) {
                    return Err(runtime("'package.path' must be a string"));
                }
                let path = self.concat_bytes(self.registers[base + 2])?;
                let path = self.try_intern(io::c_bytes(&path))?;
                self.registers[base + 2] = path;
                self.registers[base + 9] = path;
                self.registers[base + 3] = self.try_intern(b"")?;
                self.registers[base + 4] = Value::Number(0.);
                self.frames[owner].pc = 2;
            }
            2 => {
                let Value::String(path_id) = self.registers[base + 9] else {
                    unreachable!("search path string")
                };
                let path = &self.strings.bytes[slot_index(path_id.0)];
                let Value::Number(cursor) = self.registers[base + 4] else {
                    unreachable!("search cursor")
                };
                let mut start = cursor as usize;
                while path.get(start) == Some(&b';') {
                    start += 1
                }
                if start == path.len() {
                    return self.return_frame(frame, base + 3, 1);
                }
                let end = path[start..]
                    .iter()
                    .position(|&b| b == b';')
                    .map_or(path.len(), |n| start + n);
                self.registers[base + 4] = Value::Number(end as f64);
                let Value::String(name_id) = self.registers[base + 8] else {
                    unreachable!("search name string")
                };
                let filename = replace(
                    &path[start..end],
                    b'?',
                    &self.strings.bytes[slot_index(name_id.0)],
                )?;
                if io::open_input(&filename, false)?.is_ok() {
                    self.registers[base + 5] = self.try_intern(&filename)?;
                    self.registers[base + 6] = Value::Native(CONT_FILE);
                    self.registers[base + 7] = self.registers[base + 5];
                    self.frames[owner].pc = 3;
                    self.package_call(base + 6, 1, 2)?;
                } else {
                    let mut bytes = self.concat_bytes(self.registers[base + 3])?;
                    bytes.try_extend_from_slice(b"\n\tno file '")?;
                    bytes.try_extend_from_slice(&filename)?;
                    bytes.try_push(b'\'')?;
                    self.registers[base + 3] = self.try_intern(&bytes)?;
                }
            }
            3 => {
                if self.registers[base + 6] == Value::Nil {
                    return self.package_load_error(frame, base + 5, self.registers[base + 7]);
                }
                return self.return_frame(frame, base + 6, 1);
            }
            _ => unreachable!("file search phase"),
        }
        Ok(None)
    }
    fn package_load_error(
        &mut self,
        frame: Frame,
        file: usize,
        error: Value,
    ) -> Result<Option<usize>, Error> {
        let name = self.concat_bytes(self.registers[frame.base])?;
        let filename = self.concat_bytes(self.registers[file])?;
        let text = self.package_error_text(error)?;
        let mut bytes = self.try_take_buffer(name.len() + filename.len() + text.len() + 50)?;
        bytes.try_extend_from_slice(b"error loading module '")?;
        bytes.try_extend_from_slice(&name)?;
        bytes.try_extend_from_slice(b"' from file '")?;
        bytes.try_extend_from_slice(&filename)?;
        bytes.try_extend_from_slice(b"':\n\t")?;
        bytes.try_extend_from_slice(io::c_bytes(&text))?;
        let value = self.try_finish_buffer(bytes)?;
        let value = self.try_located_error(value, 1)?;
        Err(self.package_raised(value))
    }
    pub(super) fn open_package(&mut self, loaded: TableId) -> Result<(), Error> {
        let library = self.try_new_table()?;
        let loaders = self.try_new_table()?;
        let preload = self.try_new_table()?;
        for (name, value) in [
            (b"loaded".as_slice(), Value::Table(loaded)),
            (b"preload", Value::Table(preload)),
            (b"loaders", Value::Table(loaders)),
            (b"seeall", Value::Native(SEEALL)),
            (b"loadlib", Value::Native(LOADLIB)),
        ] {
            let key = self.try_intern(name)?;
            self.raw_set(library, key, value)?;
        }
        for (i, id) in [PRELOAD, LUA, C, CROOT].into_iter().enumerate() {
            self.raw_set(loaders, Value::Number((i + 1) as f64), Value::Native(id))?;
            self.set_native_environment(id, library)?;
        }
        #[cfg(windows)]
        let (path, config) = (
            b".\\?.lua;!\\lua\\?.lua;!\\lua\\?\\init.lua;!\\?.lua;!\\?\\init.lua".as_slice(),
            b"\\\n;\n?\n!\n-".as_slice(),
        );
        #[cfg(not(windows))]
        let (path,config)=(b"./?.lua;/usr/local/share/lua/5.1/?.lua;/usr/local/share/lua/5.1/?/init.lua;/usr/local/lib/lua/5.1/?.lua;/usr/local/lib/lua/5.1/?/init.lua".as_slice(),b"/\n;\n?\n!\n-".as_slice());
        let bytes = paths::initial(c"LUA_PATH", path, self.tables.memory().clone())?;
        let key = self.try_intern(b"path")?;
        let value = self.try_intern(&bytes)?;
        self.raw_set(library, key, value)?;
        let key = self.try_intern(b"cpath")?;
        let value = self.try_intern(b"")?;
        self.raw_set(library, key, value)?;
        let key = self.try_intern(b"config")?;
        let value = self.try_intern(config)?;
        self.raw_set(library, key, value)?;
        for (name, id) in [(b"require".as_slice(), REQUIRE), (b"module", MODULE)] {
            self.set_global(name, Value::Native(id))?;
            self.set_native_environment(id, library)?;
        }
        let name = self.try_intern(b"package")?;
        self.raw_set(loaded, name, Value::Table(library))?;
        self.set_global(b"package", Value::Table(library))?;
        Ok(())
    }
    fn package_name(&mut self, frame: Frame) -> Result<Value, Error> {
        if !matches!(
            self.argument(frame.vararg_start - 1, frame.argc(), 0),
            Value::String(_) | Value::Number(_)
        ) {
            return Err(self.package_bad(frame, 0, b"string"));
        }
        let id = self.string_argument(frame.vararg_start - 1, frame.argc(), 0)?;
        self.registers[frame.vararg_start] = Value::String(id);
        let full = &self.strings.bytes[slot_index(id.0)];
        let bytes = io::c_bytes(full);
        if bytes.len() == full.len() {
            return Ok(Value::String(id));
        }
        let bytes = bytes.try_to_vec().map_err(|_| self.memory_failure())?;
        self.try_intern(&bytes)
    }
    fn package_named_error(&mut self, prefix: &[u8], name: Value, suffix: &[u8]) -> Error {
        let mut build = || -> Result<Value, Error> {
            let name = self.concat_bytes(name)?;
            let mut bytes = self.try_take_buffer(prefix.len() + name.len() + suffix.len())?;
            bytes.try_extend_from_slice(prefix)?;
            bytes.try_extend_from_slice(&name)?;
            bytes.try_extend_from_slice(suffix)?;
            let value = self.try_finish_buffer(bytes)?;
            self.try_located_error(value, 1)
        };
        match build() {
            Ok(value) => self.package_raised(value),
            Err(_) => self.memory_failure(),
        }
    }
    fn package_bad(&mut self, frame: Frame, index: usize, expected: &[u8]) -> Error {
        let actual = if index >= frame.argc() {
            "no value"
        } else {
            self.argument(frame.vararg_start - 1, frame.argc(), index)
                .type_name()
        };
        let mut build = || -> Result<Value, Error> {
            let mut detail = expected.try_to_vec()?;
            detail.try_extend_from_slice(b" expected, got ")?;
            detail.try_extend_from_slice(actual.as_bytes())?;
            let error = self.argument_error(index, &detail);
            if error.kind() == ErrorKind::Memory {
                return Err(error);
            }
            self.error_value(&error, 1)
        };
        match build() {
            Ok(value) => self.package_raised(value),
            Err(_) => self.memory_failure(),
        }
    }
    fn package_field(&mut self, id: u64, name: &[u8], result: usize) -> Result<(), Error> {
        let key = self.try_intern(name)?;
        self.get_table(Value::Table(self.builtin_environment(id)), key, result)
    }
    fn package_call(&mut self, call: usize, count: usize, wanted: u32) -> Result<(), Error> {
        if let Some(actual) = self.start_call(call, count, wanted, false)? {
            self.finish_call(call, actual, wanted)
        }
        Ok(())
    }
    pub(super) fn package_step(&mut self, id: u64, frame: Frame) -> Result<Option<usize>, Error> {
        match id {
            REQUIRE => self.require_step(frame),
            MODULE => self.module_step(frame),
            PRELOAD => self.preload_step(frame),
            LUA => self.searcher_step(frame),
            C | CROOT => {
                self.package_name(frame)?;
                self.registers[frame.base] =
                    self.try_intern(b"\n\tC modules are not supported by deadlib-lua")?;
                self.return_frame(frame, frame.base, 1)
            }
            SEEALL => self.seeall_step(frame),
            LOADLIB => {
                for index in 0..2 {
                    if !matches!(
                        self.argument(frame.vararg_start - 1, frame.argc(), index),
                        Value::String(_) | Value::Number(_)
                    ) {
                        return Err(self.package_bad(frame, index, b"string"));
                    }
                }
                self.registers[frame.base] = Value::Nil;
                self.registers[frame.base + 1] =
                    self.try_intern(b"C modules are not supported by deadlib-lua")?;
                self.registers[frame.base + 2] = self.try_intern(b"absent")?;
                self.return_frame(frame, frame.base, 3)
            }
            _ => unreachable!("package native ID"),
        }
    }
}
