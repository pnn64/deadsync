//! Temporary directories owned by current filesystem regression tests.
pub struct Directory(pub std::path::PathBuf);
impl Directory {
    pub fn new(label: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().canonicalize().unwrap();
        let path = base.join(format!(
            "deadsync-resource-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert_eq!(path.parent(), Some(base.as_path()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
