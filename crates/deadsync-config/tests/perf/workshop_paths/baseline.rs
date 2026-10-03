// Frozen from 8e6b38040 (0.5.1706).
use super::*;

impl AppDirs {
    pub(super) fn original_workshop_dir(&self, cwd: Option<&Path>) -> PathBuf {
        let roots = cwd
            .into_iter()
            .flat_map(|cwd| [cwd.to_path_buf(), cwd.join("deadsync")])
            .chain(std::iter::once(self.exe_dir.clone()));
        roots
            .map(|root| root.join("assets/noteskins"))
            .find(|root| root.is_dir())
            .unwrap_or_else(|| self.exe_dir.join("assets/noteskins"))
            .join("hurg")
    }

    pub(super) fn original_asset_paths(&self, cwd: Option<&Path>) -> AssetPaths {
        // The install destination wins over older copies (including cargo's
        // copied assets), so a successful download activates that exact pack.
        let mut pack_root = self.original_workshop_dir(cwd);
        pack_root.pop();
        let mut noteskin_pack_roots = vec![pack_root];
        for root in self.media_roots("assets/noteskins", cwd) {
            if !noteskin_pack_roots.contains(&root) {
                noteskin_pack_roots.push(root);
            }
        }
        let mut search_roots = Vec::with_capacity(4);
        let mut graphic_roots = Vec::with_capacity(3);
        if !self.portable {
            search_roots.push(self.data_dir.clone());
            graphic_roots.push(self.data_dir.join("assets/graphics"));
        }
        if let Some(cwd) = cwd {
            search_roots.extend([cwd.to_path_buf(), cwd.join("deadsync")]);
            graphic_roots.push(cwd.join("assets/graphics"));
        }
        search_roots.push(self.exe_dir.clone());
        graphic_roots.push(self.exe_dir.join("assets/graphics"));
        AssetPaths {
            search_roots,
            graphic_roots,
            texture_roots: [self.data_dir.join("assets"), self.exe_dir.join("assets")],
            noteskin_roots: self.noteskin_roots(),
            noteskin_pack_roots,
            noteskin_cache: self.noteskin_cache_dir(),
            banner_cache: self.banner_cache_dir(),
            cdtitle_cache: self.cdtitle_cache_dir(),
        }
    }
}
