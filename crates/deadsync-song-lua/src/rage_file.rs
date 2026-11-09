use std::path::{Component, Path, PathBuf};

// Worker-owned, single-VM file output. Compilation simulates future commands:
// publishing their writes to disk would change the next compile's input. Keep
// them for this compile only; reads across handles observe closed output files.
// This is authored file state, not a gameplay cache. No live frame does I/O.
pub(crate) enum FileWrites {
    Small(Vec<(PathBuf, Vec<u8>)>),
    Large(rustc_hash::FxHashMap<PathBuf, Vec<u8>>),
}

impl Default for FileWrites {
    fn default() -> Self {
        Self::Small(Vec::new())
    }
}

impl FileWrites {
    pub(crate) fn get(&self, path: &Path) -> Option<&Vec<u8>> {
        match self {
            Self::Small(files) => files
                .iter()
                .find(|(name, _)| name == path)
                .map(|(_, bytes)| bytes),
            Self::Large(files) => files.get(path),
        }
    }

    pub(crate) fn insert(&mut self, path: &Path, bytes: Vec<u8>) {
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

pub(crate) fn c_string(bytes: &[u8]) -> &[u8] {
    &bytes[..memchr::memchr(0, bytes).unwrap_or(bytes.len())]
}

fn file_path_text(raw: &str) -> std::borrow::Cow<'_, str> {
    if raw.as_bytes().contains(&b'\\') {
        std::borrow::Cow::Owned(raw.replace('\\', "/"))
    } else {
        std::borrow::Cow::Borrowed(raw)
    }
}

pub(crate) fn set_file_path(result: &mut PathBuf, song_dir: &Path, raw: &str) {
    let raw = file_path_text(raw);
    let raw = Path::new(raw.as_ref());
    let prefixed = matches!(raw.components().next(), Some(Component::Prefix(_)));
    let verbatim_base = matches!(song_dir.components().next(), Some(Component::Prefix(prefix)) if prefix.kind().is_verbatim());
    result.clear();
    if !raw.has_root() && !prefixed && !verbatim_base {
        result.reserve(song_dir.as_os_str().len() + raw.as_os_str().len() + 1);
        for part in song_dir.components().chain(raw.components()) {
            match part {
                Component::CurDir => {}
                Component::ParentDir => {
                    result.pop();
                }
                _ => result.push(part.as_os_str()),
            }
        }
    } else {
        // Let Path::join retain platform-specific rooted, drive-relative and
        // verbatim-prefix rules before applying the existing lexical collapse.
        let path = if raw.is_absolute() {
            std::borrow::Cow::Borrowed(raw)
        } else {
            std::borrow::Cow::Owned(song_dir.join(raw))
        };
        result.reserve(path.as_os_str().len());
        for part in path.components() {
            match part {
                Component::CurDir => {}
                Component::ParentDir => {
                    result.pop();
                }
                _ => result.push(part.as_os_str()),
            }
        }
    }
}
