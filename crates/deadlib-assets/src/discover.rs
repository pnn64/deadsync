use crate::{
    ASSET_TEXTURE_CONTEXT, GraphicTextureDiscovery, strip_sprite_hints,
    texture_filename_has_multiframe_hint,
};
use deadlib_present::actors::{ActorResourceArena, SpriteSource, TextureKeyHandle};
use deadlib_present::texture as present_texture;
use deadlib_render::INVALID_TEXTURE_HANDLE;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

pub const NONE_TEXTURE_CHOICE_KEY: &str = "None";
pub const INITIAL_GRAPHIC_TEXTURES: [GraphicTextureDiscovery; 3] = [
    GraphicTextureDiscovery {
        folder: "judgements",
        love_first: true,
        require_multiframe_hint: true,
    },
    GraphicTextureDiscovery {
        folder: "hold_judgements",
        love_first: false,
        require_multiframe_hint: true,
    },
    GraphicTextureDiscovery {
        folder: "held_miss",
        love_first: false,
        require_multiframe_hint: false,
    },
];

pub struct GraphicTextureChoiceCache {
    judgment: OnceLock<Vec<TextureChoice>>,
    hold_judgment: OnceLock<Vec<TextureChoice>>,
    held_miss: OnceLock<Vec<TextureChoice>>,
}

impl GraphicTextureChoiceCache {
    pub const fn new() -> Self {
        Self {
            judgment: OnceLock::new(),
            hold_judgment: OnceLock::new(),
            held_miss: OnceLock::new(),
        }
    }

    pub fn judgment_texture_choices(
        &self,
        graphic_roots: impl Fn(&str) -> Vec<PathBuf>,
    ) -> &[TextureChoice] {
        self.judgment
            .get_or_init(|| {
                texture_choices_from_folder(INITIAL_GRAPHIC_TEXTURES[0], true, graphic_roots)
            })
            .as_slice()
    }

    pub fn hold_judgment_texture_choices(
        &self,
        graphic_roots: impl Fn(&str) -> Vec<PathBuf>,
    ) -> &[TextureChoice] {
        self.hold_judgment
            .get_or_init(|| {
                texture_choices_from_folder(INITIAL_GRAPHIC_TEXTURES[1], true, graphic_roots)
            })
            .as_slice()
    }

    pub fn held_miss_texture_choices(
        &self,
        graphic_roots: impl Fn(&str) -> Vec<PathBuf>,
    ) -> &[TextureChoice] {
        self.held_miss
            .get_or_init(|| {
                texture_choices_from_folder(INITIAL_GRAPHIC_TEXTURES[2], true, graphic_roots)
            })
            .as_slice()
    }
}

impl Default for GraphicTextureChoiceCache {
    fn default() -> Self {
        Self::new()
    }
}

fn texture_choices_from_folder(
    spec: GraphicTextureDiscovery,
    include_none: bool,
    graphic_roots: impl Fn(&str) -> Vec<PathBuf>,
) -> Vec<TextureChoice> {
    let discovered = discover_graphic_textures_in_roots(
        spec.folder,
        graphic_roots(spec.folder),
        spec.love_first,
        spec.require_multiframe_hint,
    );
    texture_choices_from_discovered(discovered, include_none)
}

#[derive(Clone, Debug)]
pub struct DiscoveredTexture {
    pub key: String,
    pub label: String,
    pub source_path: String,
}

pub struct TextureChoice {
    pub key: Arc<str>,
    pub label: String,
    cached_handle: AtomicU64,
    cached_generation: AtomicU64,
    cached_actor_texture: AtomicU64,
}

impl TextureChoice {
    pub fn new(key: String, label: String) -> Self {
        Self {
            key: Arc::from(key),
            label,
            cached_handle: AtomicU64::new(INVALID_TEXTURE_HANDLE),
            cached_generation: AtomicU64::new(u64::MAX),
            cached_actor_texture: AtomicU64::new(0),
        }
    }

    #[inline(always)]
    pub fn texture_key_handle(&self) -> TextureKeyHandle {
        present_texture::cached_texture_key_handle(
            &self.key,
            &self.cached_handle,
            &self.cached_generation,
            &ASSET_TEXTURE_CONTEXT,
        )
    }

    #[inline(always)]
    pub fn actor_texture_source(&self, arena: &ActorResourceArena) -> SpriteSource {
        let generation = crate::texture_registry_generation();
        let mut handle = self.cached_handle.load(Ordering::Relaxed);
        if handle == INVALID_TEXTURE_HANDLE
            || self.cached_generation.load(Ordering::Relaxed) != generation
        {
            handle = crate::texture_handle(self.key.as_ref());
            self.cached_handle.store(handle, Ordering::Relaxed);
            self.cached_generation.store(generation, Ordering::Relaxed);
        }
        arena.texture_source(&self.key, handle, generation, &self.cached_actor_texture)
    }
}

impl Clone for TextureChoice {
    fn clone(&self) -> Self {
        Self {
            key: Arc::clone(&self.key),
            label: self.label.clone(),
            cached_handle: AtomicU64::new(self.cached_handle.load(Ordering::Relaxed)),
            cached_generation: AtomicU64::new(self.cached_generation.load(Ordering::Relaxed)),
            cached_actor_texture: AtomicU64::new(0),
        }
    }
}

impl core::fmt::Debug for TextureChoice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextureChoice")
            .field("key", &self.key)
            .field("label", &self.label)
            .finish()
    }
}

impl PartialEq for TextureChoice {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.label == other.label
    }
}

impl Eq for TextureChoice {}

impl TextureChoiceLike for TextureChoice {
    fn key(&self) -> &str {
        self.key.as_ref()
    }
}

pub trait TextureChoiceLike {
    fn key(&self) -> &str;
}

fn absolute_or_self(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

pub fn graphic_texture_roots(
    folder: &str,
    portable: bool,
    data_dir: &Path,
    exe_dir: &Path,
) -> Vec<PathBuf> {
    let mut roots = Vec::with_capacity(3);
    if !portable {
        let data_root = data_dir.join("assets").join("graphics").join(folder);
        if data_root.is_dir() {
            roots.push(data_root);
        }
    }

    let cwd_root = Path::new("assets").join("graphics").join(folder);
    if cwd_root.is_dir() {
        let cwd_root = absolute_or_self(&cwd_root);
        if !roots.iter().any(|root| root == &cwd_root) {
            roots.push(cwd_root);
        }
    }

    let exe_root = exe_dir.join("assets").join("graphics").join(folder);
    if exe_root.is_dir() && !roots.iter().any(|root| root == &exe_root) {
        roots.push(exe_root);
    }
    roots
}

fn is_png_file(filename: &str) -> bool {
    Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
}

pub fn discover_graphic_textures_in_roots(
    folder: &str,
    roots: impl IntoIterator<Item = PathBuf>,
    love_first: bool,
    require_multiframe_hint: bool,
) -> Vec<DiscoveredTexture> {
    let mut discovered = Vec::new();
    let mut seen_keys = HashSet::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if require_multiframe_hint && !texture_filename_has_multiframe_hint(file_name) {
                continue;
            }
            if !require_multiframe_hint && !is_png_file(file_name) {
                continue;
            }
            let key = format!("{folder}/{file_name}");
            if !seen_keys.insert(key.to_ascii_lowercase()) {
                continue;
            }
            let label = strip_sprite_hints(file_name);
            if label.eq_ignore_ascii_case(NONE_TEXTURE_CHOICE_KEY) {
                continue;
            }
            discovered.push(DiscoveredTexture {
                key,
                label,
                source_path: absolute_or_self(&path).to_string_lossy().replace('\\', "/"),
            });
        }
    }
    discovered.sort_by(|a, b| {
        let a_love = love_first && a.label.eq_ignore_ascii_case("Love");
        let b_love = love_first && b.label.eq_ignore_ascii_case("Love");
        match (a_love, b_love) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .label
                .to_ascii_lowercase()
                .cmp(&b.label.to_ascii_lowercase()),
        }
    });
    discovered
}

pub fn texture_choices_from_discovered(
    discovered: impl IntoIterator<Item = DiscoveredTexture>,
    include_none: bool,
) -> Vec<TextureChoice> {
    let mut choices: Vec<TextureChoice> = discovered
        .into_iter()
        .map(|texture| TextureChoice::new(texture.key, texture.label))
        .collect();
    if include_none {
        choices.push(TextureChoice::new(
            NONE_TEXTURE_CHOICE_KEY.to_string(),
            NONE_TEXTURE_CHOICE_KEY.to_string(),
        ));
    }
    choices
}

pub fn canonical_texture_key_with_asset_roots(
    path: &Path,
    asset_roots: impl IntoIterator<Item = PathBuf>,
) -> String {
    for root in asset_roots {
        if let Ok(rel) = path.strip_prefix(root) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    let rel = path.strip_prefix(Path::new("assets")).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

pub fn initial_texture_source_path(
    relative_path: &str,
    resolve_asset_path: impl FnOnce(&str) -> PathBuf,
) -> PathBuf {
    let rel = Path::new(relative_path);
    let path = if rel.is_absolute() {
        rel.to_path_buf()
    } else if relative_path.starts_with("noteskins/") {
        Path::new("assets").join(relative_path)
    } else {
        Path::new("assets/graphics").join(relative_path)
    };
    resolve_asset_path(&path.to_string_lossy())
}

pub fn texture_key_source_path(
    raw: &str,
    key: &str,
    resolve_asset_path: impl Fn(&str) -> PathBuf,
) -> PathBuf {
    if let Some(path) = crate::direct_texture_key_path(raw, key) {
        return path;
    }
    let asset_path = resolve_asset_path(&format!("assets/{key}"));
    if asset_path.is_file() {
        asset_path
    } else {
        resolve_asset_path(&format!("assets/graphics/{key}"))
    }
}

pub fn noteskin_png_texture_entries(
    roots: &[PathBuf],
    folder: &str,
    canonical_key: impl Fn(&Path) -> String,
) -> Vec<(String, String)> {
    let mut list = Vec::new();
    let mut seen_keys = HashSet::new();
    for root in roots {
        let base = root.parent().expect("noteskin root has parent");
        let mut dirs = vec![base.join(folder)];
        while let Some(dir) = dirs.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
                {
                    continue;
                }
                let key = canonical_key(&path);
                if key.starts_with("noteskins/") && seen_keys.insert(key.clone()) {
                    let file_path = path.to_string_lossy().replace('\\', "/");
                    list.push((key, file_path));
                }
            }
        }
    }
    list
}

pub fn resolve_texture_choice_key<'a, T: TextureChoiceLike>(
    requested: Option<&str>,
    choices: &'a [T],
) -> Option<&'a str> {
    resolve_texture_choice_entry(requested, choices).map(TextureChoiceLike::key)
}

pub fn resolve_texture_choice_entry<'a, T: TextureChoiceLike>(
    requested: Option<&str>,
    choices: &'a [T],
) -> Option<&'a T> {
    // When the caller explicitly opts out of a texture (e.g. user selected "None"),
    // honor that and render nothing. Only fall back to the first available choice
    // when a texture was requested but could not be located in the discovered set
    // (e.g. the user-customized file was removed).
    let key = requested?;
    choices
        .iter()
        .find(|choice| choice.key().eq_ignore_ascii_case(key))
        .or_else(|| {
            choices
                .iter()
                .find(|choice| !choice.key().eq_ignore_ascii_case(NONE_TEXTURE_CHOICE_KEY))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct Choice(&'static str);

    impl TextureChoiceLike for Choice {
        fn key(&self) -> &str {
            self.0
        }
    }

    #[test]
    fn resolves_requested_texture_choice_case_insensitively() {
        let choices = [Choice("Love"), Choice("Metal")];

        assert_eq!(
            resolve_texture_choice_entry(Some("metal"), &choices),
            Some(&choices[1])
        );
    }

    #[test]
    fn falls_back_to_first_non_none_texture_choice() {
        let choices = [Choice(NONE_TEXTURE_CHOICE_KEY), Choice("Love")];

        assert_eq!(
            resolve_texture_choice_key(Some("missing"), &choices),
            Some("Love")
        );
    }

    #[test]
    fn explicit_none_request_keeps_none_choice() {
        let choices = [Choice(NONE_TEXTURE_CHOICE_KEY), Choice("Love")];

        assert_eq!(
            resolve_texture_choice_key(Some(NONE_TEXTURE_CHOICE_KEY), &choices),
            Some(NONE_TEXTURE_CHOICE_KEY)
        );
    }

    #[test]
    fn missing_request_resolves_to_no_choice() {
        let choices = [Choice("Love")];

        assert_eq!(resolve_texture_choice_key(None, &choices), None);
    }

    #[test]
    fn texture_choices_from_discovered_appends_none_choice() {
        let choices = texture_choices_from_discovered(
            [DiscoveredTexture {
                key: "judgements/Love 2x6.png".to_string(),
                label: "Love".to_string(),
                source_path: "assets/graphics/judgements/Love 2x6.png".to_string(),
            }],
            true,
        );

        assert_eq!(
            choices,
            [
                TextureChoice::new("judgements/Love 2x6.png".to_string(), "Love".to_string()),
                TextureChoice::new(
                    NONE_TEXTURE_CHOICE_KEY.to_string(),
                    NONE_TEXTURE_CHOICE_KEY.to_string()
                ),
            ]
        );
    }

    #[test]
    fn texture_choice_exposes_key_for_resolution() {
        let choice = TextureChoice::new("key.png".to_string(), "Key".to_string());

        assert_eq!(choice.key(), "key.png");
    }

    #[test]
    fn texture_choice_actor_source_uses_arena_ownership() {
        let choice = TextureChoice::new("key.png".to_string(), "Key".to_string());
        let arena = ActorResourceArena::new(1);

        let first = choice.actor_texture_source(&arena);
        let second = choice.actor_texture_source(&arena);

        assert!(matches!(first, SpriteSource::ArenaTextureHandle { .. }));
        assert!(matches!(second, SpriteSource::ArenaTextureHandle { .. }));
        assert_eq!(Arc::strong_count(&choice.key), 2);
        assert_eq!(arena.stats().texture_misses, 1);
        assert_eq!(arena.stats().texture_hits, 1);
    }

    #[test]
    fn graphic_texture_choice_cache_discovers_judgments() {
        let root = std::env::temp_dir().join(format!(
            "deadsync-graphic-choice-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Metal 2x7.png"), [0u8]).unwrap();
        std::fs::write(root.join("Love 2x7.png"), [0u8]).unwrap();

        let cache = GraphicTextureChoiceCache::new();
        let choices = cache.judgment_texture_choices(|folder| {
            if folder == "judgements" {
                vec![root.clone()]
            } else {
                Vec::new()
            }
        });

        assert_eq!(choices[0].key(), "judgements/Love 2x7.png");
        assert_eq!(choices[1].key(), "judgements/Metal 2x7.png");
        assert_eq!(choices[2].key(), NONE_TEXTURE_CHOICE_KEY);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn initial_texture_source_path_maps_relative_asset_roots() {
        let resolved =
            initial_texture_source_path("judgements/Love 2x6.png", |path| PathBuf::from(path));
        assert_eq!(
            resolved,
            PathBuf::from("assets/graphics/judgements/Love 2x6.png")
        );

        let resolved =
            initial_texture_source_path("noteskins/dance/foo.png", |path| PathBuf::from(path));
        assert_eq!(resolved, PathBuf::from("assets/noteskins/dance/foo.png"));
    }

    #[test]
    fn initial_texture_source_path_keeps_absolute_paths() {
        let path = if cfg!(windows) {
            PathBuf::from("C:/tmp/texture.png")
        } else {
            PathBuf::from("/tmp/texture.png")
        };
        let resolved =
            initial_texture_source_path(&path.to_string_lossy(), |path| PathBuf::from(path));
        assert_eq!(resolved, path);
    }

    #[test]
    fn texture_key_source_path_prefers_assets_root_when_present() {
        let dir =
            std::env::temp_dir().join(format!("deadsync-texture-source-{}", std::process::id()));
        let asset_path = dir.join("assets").join("foo.png");
        std::fs::create_dir_all(asset_path.parent().unwrap()).expect("create fixture dir");
        std::fs::write(&asset_path, [0u8]).expect("write fixture");

        let resolved = texture_key_source_path("foo.png", "foo.png", |path| dir.join(path));

        assert_eq!(resolved, asset_path);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn texture_key_source_path_falls_back_to_graphics_root() {
        let resolved = texture_key_source_path("foo.png", "foo.png", |path| PathBuf::from(path));

        assert_eq!(resolved, PathBuf::from("assets/graphics/foo.png"));
    }
}
