//! Persistent shared-bytecode caches; cache failure never changes compilation errors.
use crate::{Error, Program};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

mod codec;
static TEMP_ID: AtomicU64 = AtomicU64::new(1);

/// What happened to a persistent compiled-program entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheStatus {
    /// The source, name, build and bytecode checks passed.
    Hit,
    /// Compilation succeeded and a replacement entry was saved.
    Stored,
    /// Compilation succeeded without saving an entry (I/O, build or size limit).
    Bypassed,
}

/// A compiled program and its cache outcome.
#[derive(Clone, Debug)]
pub struct CachedProgram {
    /// Immutable code with the same host lifetime rules as [crate::compile].
    pub program: Program,
    /// Whether this call loaded, saved or bypassed the cache.
    pub status: CacheStatus,
}

/// A disk cache for Lua 5.1 programs, independent of any VM.
///
/// Entries contain shared instructions, constants, collector/debug metadata and
/// retained native-chunk words. They do not contain VM objects or environments.
/// Source bytes and chunk names are compared in full, so filename collisions
/// cause misses. Entries are bound to the format, target and Cargo build inputs.
///
/// Use a directory trusted to store executable code: integrity checks detect
/// damaged files; they do not authenticate their author. Missing, stale, invalid,
/// oversized or unreadable entries cause normal compilation. Failed writes do
/// not turn valid Lua into a compilation error. Concurrent writers use separate
/// temporary files and rename completed entries; a failed replacement preserves
/// the existing file.
///
/// Default limits are 64 MiB per file and 256 MiB of decoded object/payload
/// storage, excluding allocator metadata. Compilation itself uses its normal
/// limits. Direct rustc builds without the Cargo build identity bypass caching.
///
/// ~~~
/// use deadlib_lua::{CacheStatus, ProgramCache, Value, Vm};
/// # let directory = std::env::temp_dir().join(format!("deadlib-lua-doc-{}", std::process::id()));
/// let cache = ProgramCache::new(&directory);
/// let cached = cache.compile_named(b"return 6 * 7", b"=example")?;
/// let mut vm = Vm::new();
/// assert_eq!(vm.execute(&cached.program)?, &[Value::Number(42.0)]);
/// let again = cache.compile_named(b"return 6 * 7", b"=example")?;
/// assert_eq!(again.status, CacheStatus::Hit);
/// # for entry in std::fs::read_dir(&directory).unwrap() {
/// #     std::fs::remove_file(entry.unwrap().path()).unwrap();
/// # }
/// # std::fs::remove_dir(&directory).unwrap();
/// # Ok::<(), deadlib_lua::Error>(())
/// ~~~
#[derive(Clone, Debug)]
pub struct ProgramCache {
    directory: PathBuf,
    file_limit: usize,
    decoded_limit: usize,
}

impl ProgramCache {
    /// Select a cache directory. It is created only when saving a program.
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            file_limit: 64 * 1024 * 1024,
            decoded_limit: 256 * 1024 * 1024,
        }
    }

    /// Override the maximum file bytes and decoded object/payload bytes.
    ///
    /// Exceeding a limit bypasses caching; it does not reject otherwise valid Lua.
    pub fn with_limits(mut self, file_bytes: usize, decoded_bytes: usize) -> Self {
        self.file_limit = file_bytes;
        self.decoded_limit = decoded_bytes;
        self
    }

    /// Compile with the same default chunk name as [crate::compile].
    ///
    /// # Errors
    /// Returns the normal source/native-chunk compilation error.
    pub fn compile(&self, source: &[u8]) -> Result<CachedProgram, Error> {
        self.compile_named(source, b"=(deadlib-lua)")
    }

    /// Load matching verified code, or compile and attempt to save a replacement.
    ///
    /// # Errors
    /// Returns the same compilation errors as [crate::compile_named] on a miss.
    pub fn compile_named(&self, source: &[u8], name: &[u8]) -> Result<CachedProgram, Error> {
        let Some(build) = option_env!("DEADLIB_LUA_CACHE_BUILD") else {
            return crate::compile_named(source, name).map(|program| CachedProgram {
                program,
                status: CacheStatus::Bypassed,
            });
        };
        let revision = crate::locale::revision();
        let context = lexer_context();
        let path = self.path(source, name);
        if let Some(bytes) = self.read(&path)
            && let Some(program) =
                codec::decode(&bytes, source, name, build, &context, self.decoded_limit)
            && revision == crate::locale::revision()
        {
            return Ok(CachedProgram {
                program,
                status: CacheStatus::Hit,
            });
        }
        let program = crate::compile_named(source, name)?;
        let status = if revision == crate::locale::revision()
            && let Some(bytes) = codec::encode(
                &program,
                source,
                name,
                build,
                &context,
                self.file_limit,
                self.decoded_limit,
            )
            && self.save(&path, &bytes)
        {
            CacheStatus::Stored
        } else {
            CacheStatus::Bypassed
        };
        Ok(CachedProgram { program, status })
    }

    fn path(&self, source: &[u8], name: &[u8]) -> PathBuf {
        // Only an index. Full source/name comparisons decide whether code matches.
        let mut hash = 0xcbf29ce484222325u64;
        for bytes in [source, name] {
            for word in bytes.as_chunks::<8>().0 {
                hash = (hash ^ u64::from_le_bytes(*word)).wrapping_mul(0x9e3779b185ebca87);
            }
            for &byte in bytes.as_chunks::<8>().1 {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
            hash ^= (bytes.len() as u64).rotate_left(17);
        }
        self.directory.join(format!("{hash:016x}.dlc"))
    }

    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        let mut file = File::open(path).ok()?;
        let len = usize::try_from(file.metadata().ok()?.len()).ok()?;
        if len > self.file_limit {
            return None;
        }
        let mut bytes = vec![0; len];
        file.read_exact(&mut bytes).ok()?;
        let mut extra = [0];
        (file.read(&mut extra).ok()? == 0).then_some(bytes)
    }

    fn save(&self, path: &Path, bytes: &[u8]) -> bool {
        if fs::create_dir_all(&self.directory).is_err() {
            return false;
        }
        for _ in 0..8 {
            let temporary = self.directory.join(format!(
                ".{}.{}.{}.tmp",
                path.file_stem().unwrap().to_string_lossy(),
                std::process::id(),
                TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            let mut file = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
            {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return false,
            };
            let written = file.write_all(bytes).is_ok();
            drop(file);
            let saved = written && fs::rename(&temporary, path).is_ok();
            if !saved {
                let _ = fs::remove_file(&temporary);
            }
            return saved;
        }
        false
    }
}

fn lexer_context() -> [u8; 128] {
    // ASCII classes are fixed. The parser also observes the locale's high bytes.
    std::array::from_fn(|i| {
        let byte = i as u8 + 128;
        u8::from(crate::locale::is_alpha(byte))
            | (u8::from(crate::locale::is_digit(byte)) << 1)
            | (u8::from(crate::locale::is_space(byte)) << 2)
    })
}

#[cfg(test)]
mod tests;
