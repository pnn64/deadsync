// Frozen from 6a09e355d; visibility and separator-expression adapters only.
use mlua::{Function, Lua, MultiValue, Table, Value};
use std::path::{Component, Path, PathBuf};

use crate::runtime::note_song_lua_side_effect;

// Worker-owned, single-VM file output. Compilation simulates future commands:
// publishing their writes to disk would change the next compile's input. Keep
// them for this compile only; reads across handles observe closed output files.
// This is authored file state, not a gameplay cache. No live frame does I/O.
enum FileWrites {
    Small(Vec<(PathBuf, Vec<u8>)>),
    Large(rustc_hash::FxHashMap<PathBuf, Vec<u8>>),
}

impl Default for FileWrites {
    fn default() -> Self {
        Self::Small(Vec::new())
    }
}

impl FileWrites {
    fn get(&self, path: &Path) -> Option<&Vec<u8>> {
        match self {
            Self::Small(files) => files
                .iter()
                .find(|(name, _)| name == path)
                .map(|(_, bytes)| bytes),
            Self::Large(files) => files.get(path),
        }
    }

    fn insert(&mut self, path: &Path, bytes: Vec<u8>) {
        match self {
            Self::Small(files) => {
                if let Some((_, current)) = files.iter_mut().find(|(name, _)| name == path) {
                    *current = bytes;
                } else if files.len() < 4 {
                    files.push((path.to_path_buf(), bytes));
                } else {
                    // Move existing paths and buffers without copying them.
                    // Tiny stores retain the original allocation/lookup path.
                    let mut indexed = rustc_hash::FxHashMap::with_capacity_and_hasher(
                        files.len() + 1,
                        Default::default(),
                    );
                    indexed.extend(std::mem::take(files));
                    indexed.insert(path.to_path_buf(), bytes);
                    *self = Self::Large(indexed);
                }
            }
            Self::Large(files) => {
                if let Some(current) = files.get_mut(path) {
                    *current = bytes;
                } else {
                    files.insert(path.to_path_buf(), bytes);
                }
            }
        }
    }
}

pub(super) fn written_file(lua: &Lua, path: &Path) -> Option<Vec<u8>> {
    lua.app_data_ref::<FileWrites>()?.get(path).cloned()
}

#[derive(Default)]
struct RageFile {
    path: PathBuf,
    bytes: Vec<u8>,
    pos: usize,
    mode: i32,
    open: bool,
    eof: bool,
    destroyed: bool,
    error: String,
}

impl RageFile {
    fn check(&self, mode: i32) -> mlua::Result<()> {
        let reason = if self.destroyed {
            "has been destroyed"
        } else if !self.open {
            "is not open"
        } else if mode != 0 && self.mode & mode == 0 {
            if mode == 1 {
                "is not open for reading"
            } else {
                "is not open for writing"
            }
        } else {
            return Ok(());
        };
        Err(mlua::Error::RuntimeError(format!(
            "File '{}' {reason}.",
            self.path.display()
        )))
    }

    fn close(&mut self, lua: &Lua) {
        if self.open && self.mode & 2 != 0 {
            let mut writes = lua
                .app_data_mut::<FileWrites>()
                .expect("file store installed with RageFileUtil");
            writes.insert(&self.path, std::mem::take(&mut self.bytes));
        }
        self.open = false;
    }

    fn read(&mut self, count: i32) -> mlua::Result<&[u8]> {
        self.check(1)?;
        if count < -1 {
            return Err(mlua::Error::RuntimeError(
                "ReadBytes: invalid byte count".into(),
            ));
        }
        let start = self.pos.min(self.bytes.len());
        let available = self.bytes.len() - start;
        let wanted = if count == -1 {
            available
        } else {
            count as usize
        };
        let size = wanted.min(available);
        self.pos += size;
        // Exact-length reads don't discover EOF. Read(-1) keeps reading until
        // the native zero-byte read; ReadBytes(0) doesn't touch EOF at all.
        if count == -1 || wanted > available {
            self.eof = true;
        }
        Ok(c_string(&self.bytes[start..start + size]))
    }

    fn get_line(&mut self) -> mlua::Result<&[u8]> {
        self.check(1)?;
        let start = self.pos.min(self.bytes.len());
        let rest = &self.bytes[start..];
        if rest.is_empty() {
            self.eof = true;
            return Ok(&[]);
        }
        let size = memchr::memchr(b'\n', rest);
        let mut end = size.unwrap_or(rest.len());
        if size.is_some() && end > 0 && rest[end - 1] == b'\r' {
            end -= 1;
        }
        self.pos += size.map_or(rest.len(), |size| size + 1);
        // Even an unterminated final line delays EOF until the next call.
        Ok(c_string(&rest[..end]))
    }
}

fn c_string(bytes: &[u8]) -> &[u8] {
    &bytes[..memchr::memchr(0, bytes).unwrap_or(bytes.len())]
}

pub(super) fn file_path(song_dir: &Path, raw: &str) -> PathBuf {
    let raw = raw.replace('\\', "/");
    let raw = Path::new(&raw);
    let path = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        song_dir.join(raw)
    };
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            _ => result.push(part.as_os_str()),
        }
    }
    result
}

pub fn create_rage_file_util_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    if lua.app_data_ref::<FileWrites>().is_none() {
        lua.set_app_data(FileWrites::default());
    }
    let util = lua.create_table()?;
    let song_dir = song_dir.to_path_buf();
    util.set(
        "CreateRageFile",
        lua.create_function(move |lua, _args: MultiValue| create_rage_file_table(lua, &song_dir))?,
    )?;
    Ok(util)
}

fn create_rage_file_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    let file = lua.create_table()?;
    let state = lua.create_any_userdata(RageFile::default())?;
    let handle = state.clone();
    let song_dir = song_dir.to_path_buf();
    file.set(
        "Open",
        lua.create_function(move |lua, (_file, raw, mode): (Table, mlua::LuaString, i32)| {
            let mut state = handle.borrow_mut::<RageFile>()?;
            if state.destroyed {
                return state.check(0).map(|()| false);
            }
            note_song_lua_side_effect(lua)?;
            state.close(lua);
            state.path = file_path(
                &song_dir,
                &String::from_utf8_lossy(c_string(&raw.as_bytes())),
            );
            state.mode = mode;
            state.pos = 0;
            state.eof = false;
            state.bytes.clear();
            if mode & 3 == 3 {
                state.error = "Reading and writing are mutually exclusive".into();
                return Ok(false);
            }
            if mode & 3 == 0 {
                state.error = "Neither reading nor writing specified".into();
                return Ok(false);
            }
            if mode & 1 != 0 {
                match written_file(lua, &state.path)
                    .map(Ok)
                    .unwrap_or_else(|| std::fs::read(&state.path))
                {
                    Ok(bytes) => state.bytes = bytes,
                    Err(error) => {
                        state.error = match error.kind() {
                            std::io::ErrorKind::NotFound => "No such file or directory".into(),
                            std::io::ErrorKind::PermissionDenied => "Permission denied".into(),
                            _ => error.to_string(),
                        };
                        return Ok(false);
                    }
                }
            } else if state.path.is_dir() || state.path.ancestors().skip(1).any(Path::is_file) {
                state.error = "Not a writable file path".into();
                return Ok(false);
            }
            state.open = true;
            Ok(true)
        })?,
    )?;
    for name in ["Write", "PutLine"] {
        let handle = state.clone();
        file.set(
            name,
            lua.create_function(move |lua, (_file, text): (Table, mlua::LuaString)| {
                let mut state = handle.borrow_mut::<RageFile>()?;
                state.check(2)?;
                note_song_lua_side_effect(lua)?;
                let bytes = text.as_bytes();
                let bytes = c_string(&bytes);
                state.bytes.extend_from_slice(bytes);
                let count = if name == "PutLine" {
                    state.bytes.extend_from_slice(b"\r\n");
                    2
                } else {
                    bytes.len()
                };
                Ok(count)
            })?,
        )?;
    }
    for name in ["Read", "GetLine"] {
        let handle = state.clone();
        file.set(
            name,
            lua.create_function(move |lua, _file: Table| {
                let mut state = handle.borrow_mut::<RageFile>()?;
                let bytes = if name == "Read" {
                    state.read(-1)?
                } else {
                    state.get_line()?
                };
                lua.create_string(bytes)
            })?,
        )?;
    }
    let handle = state.clone();
    file.set(
        "ReadBytes",
        lua.create_function(move |lua, (_file, count): (Table, i32)| {
            let mut state = handle.borrow_mut::<RageFile>()?;
            lua.create_string(state.read(count)?)
        })?,
    )?;
    let handle = state.clone();
    file.set(
        "Seek",
        lua.create_function(move |_, (_file, pos): (Table, i32)| {
            let mut state = handle.borrow_mut::<RageFile>()?;
            state.check(1)?;
            if pos >= 0 && state.pos == pos as usize {
                return Ok(pos);
            }
            state.eof = false;
            if pos < 0 {
                return Ok(-1);
            }
            state.pos = pos as usize;
            Ok(pos)
        })?,
    )?;
    let handle = state.clone();
    file.set(
        "Tell",
        lua.create_function(move |_, _file: Table| {
            let state = handle.borrow::<RageFile>()?;
            state.check(1)?;
            Ok(state.pos)
        })?,
    )?;
    let handle = state.clone();
    file.set(
        "AtEOF",
        lua.create_function(move |_, _file: Table| {
            let state = handle.borrow::<RageFile>()?;
            state.check(1)?;
            Ok(state.eof)
        })?,
    )?;
    let handle = state.clone();
    file.set(
        "GetError",
        lua.create_function(move |_, _file: Table| {
            let state = handle.borrow::<RageFile>()?;
            if state.destroyed {
                state.check(0)?;
            }
            Ok(state.error.clone())
        })?,
    )?;
    let handle = state.clone();
    file.set(
        "Flush",
        lua.create_function(move |lua, file: Table| {
            let mut state = handle.borrow_mut::<RageFile>()?;
            if state.destroyed {
                state.check(0)?;
            }
            if !state.open {
                state.error = "Not open".into();
            }
            note_song_lua_side_effect(lua)?;
            Ok(file)
        })?,
    )?;
    for name in ["Close", "ClearError", "destroy"] {
        let handle = state.clone();
        file.set(
            name,
            lua.create_function(move |lua, args: MultiValue| {
                let Some(Value::Table(file)) = args.front() else {
                    return Err(mlua::Error::RuntimeError("RageFile self expected".into()));
                };
                let mut state = handle.borrow_mut::<RageFile>()?;
                if state.destroyed {
                    state.check(0)?;
                }
                note_song_lua_side_effect(lua)?;
                if name == "ClearError" {
                    state.error.clear();
                } else {
                    state.close(lua);
                }
                if name == "destroy" {
                    state.destroyed = true;
                }
                // Luna removes self and returns one value without pushing. Lua
                // 5.1 returns the calling function with no args, otherwise the
                // final argument. This is observable in the independent native VM.
                if args.len() > 1 {
                    Ok(args.back().cloned().expect("nonempty method arguments"))
                } else {
                    Ok(Value::Function(file.get::<Function>(name)?))
                }
            })?,
        )?;
    }
    Ok(file)
}


pub(super) fn file_path_text(raw: &str) -> String { raw.replace('\\', "/") }
