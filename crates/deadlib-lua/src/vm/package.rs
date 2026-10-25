//! Lua 5.1 module discovery, cached imports and module environments.
use super::*;

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

fn runtime(message: impl Into<Box<str>>) -> Error {
    Error::new(ErrorKind::Runtime, 0, message)
}

fn replace(source: &[u8], needle: u8, value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(source.len());
    for &byte in source {
        if byte == needle {
            out.extend_from_slice(value)
        } else {
            out.push(byte)
        }
    }
    out
}

fn initial_path(variable: &str, default: &[u8]) -> Vec<u8> {
    let source = std::env::var_os(variable).map(|s| s.to_string_lossy().as_bytes().to_vec());
    let path = if let Some(source) = source {
        let mut out = Vec::new();
        let mut at = 0;
        while at < source.len() {
            if source[at..].starts_with(b";;") {
                out.extend_from_slice(b";\x01;");
                at += 2
            } else {
                out.push(source[at]);
                at += 1
            }
        }
        replace(&out, 1, default)
    } else {
        default.to_vec()
    };
    #[cfg(windows)]
    {
        let executable = std::env::current_exe().expect("current executable path");
        let parent = executable
            .parent()
            .expect("executable directory")
            .to_string_lossy();
        replace(&path, b'!', parent.as_bytes())
    }
    #[cfg(not(windows))]
    {
        path
    }
}

impl Vm {
    fn module_step(&mut self, mut frame: Frame) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        let base = frame.base;
        let scratch = base + frame.argc();
        if frame.pc == 0 {
            let name = self.package_name(frame)?;
            self.resize_registers(self.registers.len().max(scratch + 9));
            self.registers
                .copy_within(frame.vararg_start..frame.vararg_start + frame.argc(), base);
            self.registers[scratch + 8] = name;
            self.registers[base] = self.argument(frame.vararg_start - 1, frame.argc(), 0);
            frame.vararg_start = base;
            frame.pc = 1;
            self.frames[owner] = frame;
            let key = self.intern(b"_LOADED");
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
                let key = self.intern(&name[start..end]);
                let target = self.registers[scratch + 2];
                let value = self.table(target)?.get(key);
                self.registers[scratch + 4] = Value::Number(dot.map_or(-1., |n| (n + 1) as f64));
                self.frames[owner].pc = 4;
                self.registers[scratch + 1] = match value {
                    Value::Table(_) => value,
                    Value::Nil => Value::Table(self.new_table()),
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
                let key = self.intern(b"_NAME");
                self.get_table(self.registers[scratch + 1], key, scratch + 3)?;
            }
            7 => {
                if self.registers[scratch + 3] != Value::Nil {
                    self.frames[owner].pc = 10
                } else {
                    self.frames[owner].pc = 8;
                    let key = self.intern(b"_M");
                    self.set_table(
                        self.registers[scratch + 1],
                        key,
                        self.registers[scratch + 1],
                    )?
                }
            }
            8 => {
                self.frames[owner].pc = 9;
                let key = self.intern(b"_NAME");
                self.set_table(
                    self.registers[scratch + 1],
                    key,
                    self.registers[scratch + 8],
                )?;
            }
            9 => {
                let name = self.concat_bytes(self.registers[scratch + 8])?;
                let length = name.iter().rposition(|&b| b == b'.').map_or(0, |n| n + 1);
                let package = self.intern(&name[..length]);
                let key = self.intern(b"_PACKAGE");
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
                self.functions[slot_index(function.0)].env = module;
                self.registers[scratch + 2] = Value::Table(module);
                self.registers[scratch + 5] = Value::Number(1.);
                self.frames[owner].pc = 11;
            }
            11 => {
                let Value::Number(index) = self.registers[scratch + 5] else {
                    unreachable!("module option index")
                };
                if index as usize >= frame.argc() {
                    return Ok(self.return_frame(frame, base, 0));
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
    fn require_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let owner = self.frames.len() - 1;
        match frame.pc {
            0 => {
                let name = self.package_name(frame)?;
                let original = match self.argument(frame.vararg_start - 1, frame.argc(), 0) {
                    value @ Value::String(_) => value,
                    _ => name,
                };
                self.registers[frame.vararg_start..frame.vararg_start + frame.argc()]
                    .fill(Value::Nil);
                self.frames[owner].vararg_count = 0;
                self.resize_registers(self.registers.len().max(base + 9));
                self.registers[base + 8] = name;
                self.registers[base] = original;
                self.frames[owner].pc = 1;
                let key = self.intern(b"_LOADED");
                self.get_table(Value::Table(self.registry), key, base + 1)?;
            }
            1 => {
                self.frames[owner].pc = 2;
                self.get_table(self.registers[base + 1], self.registers[base + 8], base + 2)?;
            }
            2 => {
                if self.registers[base + 2].is_truthy() {
                    if self.registers[base + 2] == SENTINEL {
                        return Err(self.package_named_error(
                            b"loop or previous error loading module '",
                            self.registers[base + 8],
                            b"'",
                        ));
                    }
                    return Ok(self.return_frame(frame, base + 2, 1));
                }
                self.frames[owner].pc = 3;
                self.package_field(REQUIRE, b"loaders", base + 3)?;
            }
            3 => {
                if !matches!(self.registers[base + 3], Value::Table(_)) {
                    return Err(runtime("'package.loaders' must be a table"));
                }
                self.registers[base + 4] = self.intern(b"");
                self.registers[base + 5] = Value::Number(1.);
                self.frames[owner].pc = 4;
            }
            4 => {
                let loader = self
                    .table(self.registers[base + 3])?
                    .get(self.registers[base + 5]);
                if loader == Value::Nil {
                    let name = self.concat_bytes(self.registers[base + 8])?;
                    let errors = self
                        .concat_bytes(self.registers[base + 4])
                        .unwrap_or_else(|_| b"(null)".to_vec());
                    let mut message = self.take_buffer(name.len() + errors.len() + 32);
                    message.extend_from_slice(b"module '");
                    message.extend_from_slice(&name);
                    message.extend_from_slice(b"' not found:");
                    message.extend_from_slice(io::c_bytes(&errors));
                    let value = self.finish_buffer(message);
                    let value = self.located_error(value, 1);
                    return Err(self.package_raised(value));
                }
                let Value::Number(index) = self.registers[base + 5] else {
                    unreachable!("loader index")
                };
                self.registers[base + 5] = Value::Number(index + 1.);
                self.registers[base + 6] = loader;
                self.registers[base + 7] = self.registers[base + 8];
                self.frames[owner].pc = 5;
                self.package_call(base + 6, 1, 1)?;
            }
            5 => {
                let value = self.registers[base + 6];
                if matches!(value, Value::Function(_) | Value::Native(_)) {
                    self.frames[owner].pc = 6;
                    self.set_table(self.registers[base + 1], self.registers[base + 8], SENTINEL)?;
                } else {
                    self.frames[owner].pc = 4;
                    if matches!(value, Value::String(_) | Value::Number(_)) {
                        self.concatenate(self.registers[base + 4], value, base + 4)?;
                    }
                }
            }
            6 => {
                self.registers[base + 7] = self.registers[base + 8];
                self.frames[owner].pc = 7;
                self.package_call(base + 6, 1, 1)?;
            }
            7 => {
                self.frames[owner].pc = 8;
                if self.registers[base + 6] != Value::Nil {
                    self.set_table(
                        self.registers[base + 1],
                        self.registers[base + 8],
                        self.registers[base + 6],
                    )?;
                }
            }
            8 => {
                self.frames[owner].pc = 9;
                self.get_table(self.registers[base + 1], self.registers[base + 8], base + 2)?;
            }
            9 => {
                if self.registers[base + 2] != SENTINEL {
                    return Ok(self.return_frame(frame, base + 2, 1));
                }
                self.registers[base + 2] = Value::Boolean(true);
                self.frames[owner].pc = 10;
                self.set_table(
                    self.registers[base + 1],
                    self.registers[base + 8],
                    Value::Boolean(true),
                )?;
            }
            10 => return Ok(self.return_frame(frame, base + 2, 1)),
            _ => unreachable!("require phase"),
        }
        Ok(None)
    }
    fn package_raised(&self, value: Value) -> Error {
        let message = if let Value::String(id) = value {
            String::from_utf8_lossy(&self.strings.bytes[slot_index(id.0)]).into_owned()
        } else {
            "Lua error".into()
        };
        Error::raised(value, message)
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
                if self.registers[base + 2] == Value::Nil {
                    let name = self.concat_bytes(self.registers[base])?;
                    let mut bytes = self.take_buffer(name.len() + 40);
                    bytes.extend_from_slice(b"\n\tno field package.preload['");
                    bytes.extend_from_slice(&name);
                    bytes.extend_from_slice(b"']");
                    self.registers[base + 2] = self.finish_buffer(bytes);
                }
                return Ok(self.return_frame(frame, base + 2, 1));
            }
            _ => unreachable!("preload phase"),
        }
        Ok(None)
    }
    fn seeall_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        if frame.pc == 0 {
            let value = self.argument(frame.vararg_start - 1, frame.argc(), 0);
            let Value::Table(_) = value else {
                return Err(self.package_bad(frame, 0, b"table"));
            };
            let mt = if let Some(mt) = self.metatable(value) {
                mt
            } else {
                let mt = self.new_table();
                self.set_metatable(value, Some(mt));
                mt
            };
            self.frames.last_mut().expect("seeall frame").pc = 1;
            let key = self.intern(b"__index");
            self.set_table(Value::Table(mt), key, Value::Table(self.globals))?;
            return Ok(None);
        }
        Ok(self.return_frame(frame, frame.base, 0))
    }
    fn searcher_step(&mut self, id: u64, frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let owner = self.frames.len() - 1;
        match frame.pc {
            0 => {
                let name = self.package_name(frame)?;
                let bytes = self.concat_bytes(name)?;
                let search = if id == CROOT {
                    let Some(dot) = bytes.iter().position(|&b| b == b'.') else {
                        return Ok(self.return_frame(frame, base, 0));
                    };
                    &bytes[..dot]
                } else {
                    &bytes
                };
                let replaced = replace(search, b'.', if cfg!(windows) { b"\\" } else { b"/" });
                self.resize_registers(self.registers.len().max(base + 8));
                self.registers[base] = name;
                self.registers[base + 1] = self.intern(&replaced);
                self.frames[owner].pc = 1;
                self.package_field(id, if id == LUA { b"path" } else { b"cpath" }, base + 2)?;
            }
            1 => {
                if !matches!(
                    self.registers[base + 2],
                    Value::String(_) | Value::Number(_)
                ) {
                    return Err(runtime(if id == LUA {
                        "'package.path' must be a string"
                    } else {
                        "'package.cpath' must be a string"
                    }));
                }
                let path = self.concat_bytes(self.registers[base + 2])?;
                let path = self.intern(io::c_bytes(&path));
                self.registers[base + 2] = path;
                self.registers[base + 3] = self.intern(b"");
                self.registers[base + 4] = Value::Number(0.);
                self.frames[owner].pc = 2;
            }
            2 => {
                let Value::String(path_id) = self.registers[base + 2] else {
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
                    return Ok(self.return_frame(frame, base + 3, 1));
                }
                let end = path[start..]
                    .iter()
                    .position(|&b| b == b';')
                    .map_or(path.len(), |n| start + n);
                self.registers[base + 4] = Value::Number(end as f64);
                let Value::String(name_id) = self.registers[base + 1] else {
                    unreachable!("search name string")
                };
                let filename = replace(
                    &path[start..end],
                    b'?',
                    &self.strings.bytes[slot_index(name_id.0)],
                );
                if io::open(&filename, b"r").is_ok() {
                    self.registers[base + 5] = self.intern(&filename);
                    if id != LUA {
                        let error =
                            self.intern(b"dynamic libraries are not enabled in this Rust engine");
                        return self.package_load_error(frame, base + 5, error);
                    }
                    self.registers[base + 6] = Value::Native(CONT_FILE);
                    self.registers[base + 7] = self.registers[base + 5];
                    self.frames[owner].pc = 3;
                    self.package_call(base + 6, 1, 2)?;
                } else {
                    let mut bytes = self.concat_bytes(self.registers[base + 3])?;
                    bytes.extend_from_slice(b"\n\tno file '");
                    bytes.extend_from_slice(&filename);
                    bytes.push(b'\'');
                    self.registers[base + 3] = self.intern(&bytes);
                }
            }
            3 => {
                if self.registers[base + 6] == Value::Nil {
                    return self.package_load_error(frame, base + 5, self.registers[base + 7]);
                }
                return Ok(self.return_frame(frame, base + 6, 1));
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
        let text = self
            .concat_bytes(error)
            .unwrap_or_else(|_| b"(null)".to_vec());
        let mut bytes = self.take_buffer(name.len() + filename.len() + text.len() + 50);
        bytes.extend_from_slice(b"error loading module '");
        bytes.extend_from_slice(&name);
        bytes.extend_from_slice(b"' from file '");
        bytes.extend_from_slice(&filename);
        bytes.extend_from_slice(b"':\n\t");
        bytes.extend_from_slice(io::c_bytes(&text));
        let value = self.finish_buffer(bytes);
        let value = self.located_error(value, 1);
        Err(self.package_raised(value))
    }
    pub(super) fn open_package(&mut self, loaded: TableId) {
        let library = self.new_table();
        let loaders = self.new_table();
        let preload = self.new_table();
        for (name, value) in [
            (b"loaded".as_slice(), Value::Table(loaded)),
            (b"preload", Value::Table(preload)),
            (b"loaders", Value::Table(loaders)),
            (b"seeall", Value::Native(SEEALL)),
            (b"loadlib", Value::Native(LOADLIB)),
        ] {
            let key = self.intern(name);
            self.raw_set(library, key, value).unwrap();
        }
        for (i, id) in [PRELOAD, LUA, C, CROOT].into_iter().enumerate() {
            self.raw_set(loaders, Value::Number((i + 1) as f64), Value::Native(id))
                .unwrap();
            self.set_native_environment(id, library);
        }
        #[cfg(windows)]
        let (path, cpath, config) = (
            b".\\?.lua;!\\lua\\?.lua;!\\lua\\?\\init.lua;!\\?.lua;!\\?\\init.lua".as_slice(),
            b".\\?.dll;!\\?.dll;!\\loadall.dll".as_slice(),
            b"\\\n;\n?\n!\n-".as_slice(),
        );
        #[cfg(not(windows))]
        let (path,cpath,config)=(b"./?.lua;/usr/local/share/lua/5.1/?.lua;/usr/local/share/lua/5.1/?/init.lua;/usr/local/lib/lua/5.1/?.lua;/usr/local/lib/lua/5.1/?/init.lua".as_slice(),b"./?.so;/usr/local/lib/lua/5.1/?.so;/usr/local/lib/lua/5.1/loadall.so".as_slice(),b"/\n;\n?\n!\n-".as_slice());
        for (field, bytes) in [
            (b"path".as_slice(), initial_path("LUA_PATH", path)),
            (b"cpath", initial_path("LUA_CPATH", cpath)),
            (b"config", config.to_vec()),
        ] {
            let key = self.intern(field);
            let value = self.intern(&bytes);
            self.raw_set(library, key, value).unwrap();
        }
        for (name, id) in [(b"require".as_slice(), REQUIRE), (b"module", MODULE)] {
            self.set_global(name, Value::Native(id)).unwrap();
            self.set_native_environment(id, library);
        }
        let name = self.intern(b"package");
        self.raw_set(loaded, name, Value::Table(library)).unwrap();
        self.set_global(b"package", Value::Table(library)).unwrap();
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
        let bytes = bytes.to_vec();
        Ok(self.intern(&bytes))
    }
    fn package_named_error(&mut self, prefix: &[u8], name: Value, suffix: &[u8]) -> Error {
        let name = self.concat_bytes(name).expect("validated package name");
        let mut bytes = self.take_buffer(prefix.len() + name.len() + suffix.len());
        bytes.extend_from_slice(prefix);
        bytes.extend_from_slice(&name);
        bytes.extend_from_slice(suffix);
        let value = self.finish_buffer(bytes);
        let value = self.located_error(value, 1);
        self.package_raised(value)
    }
    fn package_bad(&mut self, frame: Frame, index: usize, expected: &[u8]) -> Error {
        let (name, kind) = self
            .debug_name(self.thread, self.frames.len() - 1)
            .unwrap_or_else(|| (b"?".to_vec().into_boxed_slice(), b""));
        let actual = if index >= frame.argc() {
            "no value"
        } else {
            self.argument(frame.vararg_start - 1, frame.argc(), index)
                .type_name()
        };
        let mut bytes = self.take_buffer(96);
        if kind == b"method" && index == 0 {
            bytes.extend_from_slice(b"calling '");
            bytes.extend_from_slice(&name);
            bytes.extend_from_slice(b"' on bad self (");
        } else {
            bytes.extend_from_slice(
                format!(
                    "bad argument #{} to '",
                    index + usize::from(kind != b"method")
                )
                .as_bytes(),
            );
            bytes.extend_from_slice(&name);
            bytes.extend_from_slice(b"' (");
        }
        bytes.extend_from_slice(expected);
        bytes.extend_from_slice(b" expected, got ");
        bytes.extend_from_slice(actual.as_bytes());
        bytes.push(b')');
        let value = self.finish_buffer(bytes);
        let value = self.located_error(value, 1);
        self.package_raised(value)
    }
    fn package_field(&mut self, id: u64, name: &[u8], result: usize) -> Result<(), Error> {
        let key = self.intern(name);
        self.get_table(Value::Table(self.native_environment(id)), key, result)
    }
    fn package_call(&mut self, call: usize, count: usize, wanted: u32) -> Result<(), Error> {
        if self.native_depth(self.frames.len()) >= 199 {
            return Err(runtime("C stack overflow"));
        }
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
            LUA | C | CROOT => self.searcher_step(id, frame),
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
                    self.intern(b"dynamic libraries are not enabled in this Rust engine");
                self.registers[frame.base + 2] = self.intern(b"absent");
                Ok(self.return_frame(frame, frame.base, 3))
            }
            _ => unreachable!("package native ID"),
        }
    }
}
