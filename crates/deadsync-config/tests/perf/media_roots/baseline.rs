// Frozen from 68e7d6aae (0.5.1704).
use super::*;

impl AppDirs {
    pub(super) fn original_media_roots(&self, dirname: &str, cwd: Option<&Path>) -> Vec<PathBuf> {
        let mut roots = Vec::with_capacity(4);
        let candidates = [
            Some(self.data_dir.join(dirname)),
            Some(self.exe_dir.join(dirname)),
            cwd.map(|path| path.join(dirname)),
            cwd.map(|path| path.join("deadsync").join(dirname)),
        ];
        for root in candidates.into_iter().flatten() {
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
        roots
    }
}
