//! Existing actor media-path lookup, independent of either Lua runtime.
use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

type PrefixCache = Mutex<HashMap<(String, String), Option<PathBuf>>>;
static PREFIXES: OnceLock<PrefixCache> = OnceLock::new();

pub(super) fn text(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    let bytes = bytes(call, value)?;
    String::from_utf8(bytes).map_err(|error| call.error(error.to_string().as_bytes()))
}

pub(super) fn field(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &[u8],
) -> Result<Option<String>, Error> {
    let key = call.try_intern(key)?;
    let value = call.table_get(Value::Table(actor), key)?;
    if value == Value::Nil {
        Ok(None)
    } else {
        text(call, value).map(Some)
    }
}

pub(super) fn resolve(
    call: &mut HostCall<'_>,
    actor: TableId,
    raw: &str,
) -> Result<Option<PathBuf>, Error> {
    let path = Path::new(raw.trim());
    if deadlib_assets::generated_texture_shared_key(raw.trim()).is_some()
        || (path.is_absolute() && path.is_file())
    {
        return Ok(Some(path.to_owned()));
    }
    if let Some(dir) =
        field(call, actor, b"__songlua_script_dir")?.filter(|dir| !dir.trim().is_empty())
    {
        let candidate = Path::new(&dir).join(raw);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
        return Ok(prefix(Path::new(&dir), path));
    }
    Ok(None)
}

fn prefix(base: &Path, path: &Path) -> Option<PathBuf> {
    if path.extension().is_some() {
        return None;
    }
    let prefix = path.file_name()?.to_str()?.trim();
    if prefix.is_empty() {
        return None;
    }
    let dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(|| base.to_owned(), |parent| base.join(parent));
    if !dir.is_dir() {
        return None;
    }
    let key = (
        dir.to_string_lossy().to_ascii_lowercase(),
        prefix.to_ascii_lowercase(),
    );
    let cache = PREFIXES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned()
    {
        return cached;
    }
    let found = fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name();
            let name = file.to_str()?;
            let path = name
                .get(..prefix.len())
                .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
                .then(|| entry.path())?;
            (path.is_file() && crate::is_song_lua_media_path(&path)).then_some(path)
        })
        .min_by(|left, right| {
            let left = left
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let right = right
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            left.bytes()
                .map(|b| b.to_ascii_lowercase())
                .cmp(right.bytes().map(|b| b.to_ascii_lowercase()))
        });
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, found.clone());
    found
}

// Whether a texture path exists. A path found once is taken to stay for the
// lifetime of this runtime: live visuals resolve the same textures on every
// frame, which would otherwise query the file system per sprite per frame. A
// missing path is queried again, so a file written during play is still found.
fn present(call: &mut HostCall<'_>, path: &Path) -> bool {
    if call.store_ref().present_paths.contains(path) {
        return true;
    }
    let found = path.exists();
    if found {
        call.store().present_paths.insert(path.to_owned());
    }
    found
}

pub(super) fn texture_path(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Option<PathBuf>, Error> {
    let key = call.try_intern(b"__songlua_default_texture")?;
    if !matches!(
        call.raw_table_get(actor, key)?,
        Value::Nil | Value::Boolean(false)
    ) {
        return Ok(None);
    }
    let Some(texture) = field(call, actor, b"Texture")? else {
        return Ok(None);
    };
    let texture = texture.trim();
    if texture.is_empty() {
        return Ok(None);
    }
    let raw = Path::new(texture);
    if deadlib_assets::generated_texture_shared_key(texture).is_some()
        || (raw.is_absolute() && present(call, raw))
    {
        return Ok(Some(raw.to_owned()));
    }
    if raw.is_absolute()
        && let Some(path) = prefix(Path::new(""), raw)
    {
        return Ok(Some(path));
    }
    for key in [b"__songlua_script_dir".as_slice(), b"__songlua_song_dir"] {
        if let Some(dir) = field(call, actor, key)? {
            let candidate = Path::new(&dir).join(texture);
            if present(call, &candidate) {
                return Ok(Some(candidate));
            }
            if let Some(path) = prefix(Path::new(&dir), raw) {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}
