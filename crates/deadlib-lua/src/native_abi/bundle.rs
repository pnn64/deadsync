//! Materialize the build's C ABI image without relying on the executable's path.
use super::*;
use std::io::Write;
use std::path::PathBuf;
pub(super) fn image() -> Result<Arc<Library>, Box<[u8]>> {
    static IMAGE: Mutex<Option<std::sync::Weak<Library>>> = Mutex::new(None);
    let mut cached = IMAGE.lock().unwrap();
    if let Some(image) = cached.as_ref().and_then(std::sync::Weak::upgrade) {
        return Ok(image);
    }
    let path = materialize().map_err(|e| {
        format!("cannot prepare Lua C ABI: {e}")
            .into_bytes()
            .into_boxed_slice()
    })?;
    let image = Arc::new(Library::shim(&path)?);
    if !matching_version(image.try_symbol(b"boundary_abi_version")?) {
        return Err(b"Lua C ABI image version differs".as_slice().into());
    }
    *cached = Some(Arc::downgrade(&image));
    Ok(image)
}
fn materialize() -> std::io::Result<PathBuf> {
    let bytes = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/",
        env!("DEADLIB_LUA_NATIVE_IMAGE")
    ));
    let root = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache")))
    }
    .unwrap_or_else(std::env::temp_dir);
    let folder = root
        .join("deadlib-lua/native")
        .join(env!("DEADLIB_LUA_NATIVE_IMAGE_ID"));
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(env!("DEADLIB_LUA_NATIVE_IMAGE"));
    if std::fs::read(&path).is_ok_and(|current| current == bytes) {
        return Ok(path);
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let temporary = folder.join(format!(
        "image-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut output = options.open(&temporary)?;
        output.write_all(bytes)?;
        output.flush()?;
        drop(output);
        if let Err(e) = std::fs::rename(&temporary, &path)
            && !std::fs::read(&path).is_ok_and(|current| current == bytes)
        {
            return Err(e);
        }
        Ok(path)
    })();
    let _ = std::fs::remove_file(temporary);
    result
}
