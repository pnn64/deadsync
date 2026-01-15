use crate::core::gfx::{Backend, SamplerDesc, SamplerFilter, SamplerWrap, Texture as GfxTexture};
use crate::game::profile;
use crate::ui::font::{self, Font, FontLoadData};
use image::RgbaImage;
use log::{info, warn};
use std::{
    collections::HashMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

// --- Texture Metadata ---

#[derive(Clone, Copy, Debug)]
pub struct TexMeta {
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug, Default)]
pub struct TextureHints {
    pub raw: String,
    pub mipmaps: Option<bool>,
    pub grayscale: bool,
    pub alphamap: bool,
    pub doubleres: bool,
    pub stretch: bool,
    pub dither: bool,
    pub color_depth: Option<u32>,
    pub sampler_filter: Option<SamplerFilter>,
    pub sampler_wrap: Option<SamplerWrap>,
}

impl TextureHints {
    #[inline(always)]
    pub fn is_default(&self) -> bool {
        self.raw.is_empty() || self.raw.eq_ignore_ascii_case("default")
    }
}

pub fn parse_texture_hints(raw: &str) -> TextureHints {
    let mut hints = TextureHints::default();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return hints;
    }
    hints.raw = trimmed.to_string();
    if trimmed.eq_ignore_ascii_case("default") {
        return hints;
    }

    let s = trimmed.to_ascii_lowercase();
    if s.contains("32bpp") {
        hints.color_depth = Some(32);
    } else if s.contains("16bpp") {
        hints.color_depth = Some(16);
    }
    if s.contains("dither") {
        hints.dither = true;
    }
    if s.contains("stretch") {
        hints.stretch = true;
    }
    if s.contains("mipmaps") {
        hints.mipmaps = Some(true);
    }
    if s.contains("nomipmaps") {
        hints.mipmaps = Some(false);
    }
    if s.contains("grayscale") {
        hints.grayscale = true;
    }
    if s.contains("alphamap") {
        hints.alphamap = true;
    }
    if s.contains("doubleres") {
        hints.doubleres = true;
    }
    if s.contains("nearest") || s.contains("point") {
        hints.sampler_filter = Some(SamplerFilter::Nearest);
    }
    if s.contains("linear") {
        hints.sampler_filter = Some(SamplerFilter::Linear);
    }
    if s.contains("wrap") || s.contains("repeat") {
        hints.sampler_wrap = Some(SamplerWrap::Repeat);
    }
    if s.contains("clamp") {
        hints.sampler_wrap = Some(SamplerWrap::Clamp);
    }

    hints
}

impl TextureHints {
    #[inline(always)]
    pub fn sampler_desc(&self) -> SamplerDesc {
        SamplerDesc {
            filter: self.sampler_filter.unwrap_or(SamplerFilter::Linear),
            wrap: self.sampler_wrap.unwrap_or(SamplerWrap::Clamp),
            mipmaps: self.mipmaps.unwrap_or(false),
        }
    }
}

static TEX_META: once_cell::sync::Lazy<RwLock<HashMap<String, TexMeta>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(HashMap::new()));

pub fn register_texture_dims(key: &str, w: u32, h: u32) {
    let mut m = TEX_META.write().unwrap();
    m.insert(key.to_string(), TexMeta { w, h });
}

pub fn texture_dims(key: &str) -> Option<TexMeta> {
    TEX_META.read().unwrap().get(key).copied()
}

pub fn canonical_texture_key<P: AsRef<Path>>(p: P) -> String {
    let p = p.as_ref();
    let rel = p.strip_prefix(Path::new("assets")).unwrap_or(p);
    rel.to_string_lossy().replace('\\', "/")
}

#[inline(always)]
fn append_noteskins_pngs(list: &mut Vec<(String, String)>, folder: &str) {
    let dir = Path::new("assets").join(folder);
    if let Ok(entries) = fs::read_dir(dir) {
        let prefix = format!("{}/", folder);
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
                && let Ok(name) = entry.file_name().into_string()
            {
                let mut key = String::with_capacity(prefix.len() + name.len());
                key.push_str(&prefix);
                key.push_str(&name);
                list.push((key.clone(), key));
            }
        }
    }
}

pub fn parse_sprite_sheet_dims(filename: &str) -> (u32, u32) {
    #[inline(always)]
    fn parse_ascii_digits(bytes: &[u8]) -> Option<u32> {
        if bytes.is_empty() {
            return None;
        }
        let mut value: u32 = 0;
        for &b in bytes {
            if !b.is_ascii_digit() {
                return None;
            }
            let digit = (b - b'0') as u32;
            value = value.checked_mul(10)?.checked_add(digit)?;
        }
        Some(value)
    }

    #[inline(always)]
    fn is_res_tag(bytes: &[u8], idx: usize) -> bool {
        idx + 4 <= bytes.len()
            && bytes[idx] == b'('
            && bytes[idx + 1].eq_ignore_ascii_case(&b'r')
            && bytes[idx + 2].eq_ignore_ascii_case(&b'e')
            && bytes[idx + 3].eq_ignore_ascii_case(&b's')
    }

    #[inline(always)]
    fn skip_parenthetical(bytes: &[u8], start: usize) -> usize {
        let mut depth = 0usize;
        let mut idx = start;
        while idx < bytes.len() {
            match bytes[idx] {
                b'(' => depth += 1,
                b')' => {
                    if depth == 0 {
                        return idx + 1;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return idx + 1;
                    }
                }
                _ => {}
            }
            idx += 1;
        }
        bytes.len()
    }

    let bytes = filename.as_bytes();
    let mut dims: Option<(u32, u32)> = None;
    let mut i = 0usize;

    while i < bytes.len() {
        if is_res_tag(bytes, i) {
            i = skip_parenthetical(bytes, i);
            continue;
        }

        let b = bytes[i];
        if (b == b'x' || b == b'X') && i > 0 && bytes[i - 1].is_ascii_digit() {
            let mut left = i;
            while left > 0 && bytes[left - 1].is_ascii_digit() {
                left -= 1;
            }

            let mut right = i + 1;
            while right < bytes.len() && bytes[right].is_ascii_digit() {
                right += 1;
            }

            if left < i
                && i + 1 < right
                && let (Some(w), Some(h)) = (
                    parse_ascii_digits(&bytes[left..i]),
                    parse_ascii_digits(&bytes[i + 1..right]),
                )
                && w > 0
                && h > 0
            {
                dims = Some((w, h));
            }

            i = right;
            continue;
        }

        i += 1;
    }

    dims.unwrap_or((1, 1))
}

#[inline(always)]
fn apply_texture_hints(image: &mut RgbaImage, hints: &TextureHints) {
    if !(hints.grayscale || hints.alphamap) {
        return;
    }

    for pixel in image.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let lum = ((r as u16 * 30 + g as u16 * 59 + b as u16 * 11) / 100) as u8;
        if hints.alphamap {
            pixel.0 = [255, 255, 255, lum];
        } else {
            pixel.0 = [lum, lum, lum, a];
        }
    }
}

// --- Asset Manager ---

pub struct AssetManager {
    pub textures: HashMap<String, GfxTexture>,
    fonts: HashMap<&'static str, Font>,
    current_dynamic_banner: Option<(String, PathBuf)>,
    current_density_graph: Option<(String, String)>,
    current_dynamic_background: Option<(String, PathBuf)>,
    current_profile_avatar: Option<(String, PathBuf)>,
}

impl AssetManager {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            fonts: HashMap::new(),
            current_dynamic_banner: None,
            current_density_graph: None,
            current_dynamic_background: None,
            current_profile_avatar: None,
        }
    }

    // --- Font Management ---

    pub fn register_font(&mut self, name: &'static str, font: Font) {
        self.fonts.insert(name, font);
    }

    pub fn fonts(&self) -> &HashMap<&'static str, Font> {
        &self.fonts
    }

    pub fn with_fonts<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HashMap<&'static str, Font>) -> R,
    {
        f(&self.fonts)
    }

    pub fn with_font<F, R>(&self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Font) -> R,
    {
        self.fonts.get(name).map(f)
    }

    // --- Loading Logic ---

    pub fn load_initial_assets(&mut self, backend: &mut Backend) -> Result<(), Box<dyn Error>> {
        self.load_initial_textures(backend)?;
        self.load_initial_fonts(backend)?;
        Ok(())
    }

    fn load_initial_textures(&mut self, backend: &mut Backend) -> Result<(), Box<dyn Error>> {
        info!("Loading initial textures...");

        #[inline(always)]
        fn fallback_rgba() -> RgbaImage {
            let data: [u8; 16] = [
                255, 0, 255, 255, 128, 128, 128, 255, 128, 128, 128, 255, 255, 0, 255, 255,
            ];
            RgbaImage::from_raw(2, 2, data.to_vec()).expect("fallback image")
        }

        // Load __white texture
        let white_img = RgbaImage::from_raw(1, 1, vec![255, 255, 255, 255]).unwrap();
        let white_tex = backend.create_texture(&white_img, SamplerDesc::default())?;
        self.textures.insert("__white".to_string(), white_tex);
        register_texture_dims("__white", 1, 1);
        info!("Loaded built-in texture: __white");

        let mut textures_to_load: Vec<(String, String)> = vec![
            ("logo.png".to_string(), "logo.png".to_string()),
            ("init_arrow.png".to_string(), "init_arrow.png".to_string()),
            ("dance.png".to_string(), "dance.png".to_string()),
            // Test Input pad assets (Simply Love-style)
            (
                "test_input/dance.png".to_string(),
                "test_input/dance.png".to_string(),
            ),
            (
                "test_input/buttons.png".to_string(),
                "test_input/buttons.png".to_string(),
            ),
            (
                "test_input/highlight.png".to_string(),
                "test_input/highlight.png".to_string(),
            ),
            (
                "test_input/highlightgreen.png".to_string(),
                "test_input/highlightgreen.png".to_string(),
            ),
            (
                "test_input/highlightred.png".to_string(),
                "test_input/highlightred.png".to_string(),
            ),
            (
                "test_input/highlightarrow.png".to_string(),
                "test_input/highlightarrow.png".to_string(),
            ),
            ("meter_arrow.png".to_string(), "meter_arrow.png".to_string()),
            (
                "rounded-square.png".to_string(),
                "rounded-square.png".to_string(),
            ),
            ("circle.png".to_string(), "circle.png".to_string()),
            ("swoosh.png".to_string(), "swoosh.png".to_string()),
            ("heart.png".to_string(), "heart.png".to_string()),
            (
                "combo_explosion.png".to_string(),
                "combo_explosion.png".to_string(),
            ),
            (
                "combo_100milestone_splode.png".to_string(),
                "combo_100milestone_splode.png".to_string(),
            ),
            (
                "combo_100milestone_minisplode.png".to_string(),
                "combo_100milestone_minisplode.png".to_string(),
            ),
            (
                "combo_1000milestone_swoosh.png".to_string(),
                "combo_1000milestone_swoosh.png".to_string(),
            ),
            (
                "hit_mine_explosion.png".to_string(),
                "hit_mine_explosion.png".to_string(),
            ),
            (
                "titlemenu_flycenter.png".to_string(),
                "titlemenu_flycenter.png".to_string(),
            ),
            (
                "titlemenu_flytop.png".to_string(),
                "titlemenu_flytop.png".to_string(),
            ),
            (
                "titlemenu_flybottom.png".to_string(),
                "titlemenu_flybottom.png".to_string(),
            ),
            (
                "banner1.png".to_string(),
                "_fallback/banner1.png".to_string(),
            ),
            (
                "banner2.png".to_string(),
                "_fallback/banner2.png".to_string(),
            ),
            (
                "banner3.png".to_string(),
                "_fallback/banner3.png".to_string(),
            ),
            (
                "banner4.png".to_string(),
                "_fallback/banner4.png".to_string(),
            ),
            (
                "banner5.png".to_string(),
                "_fallback/banner5.png".to_string(),
            ),
            (
                "banner6.png".to_string(),
                "_fallback/banner6.png".to_string(),
            ),
            (
                "banner7.png".to_string(),
                "_fallback/banner7.png".to_string(),
            ),
            (
                "banner8.png".to_string(),
                "_fallback/banner8.png".to_string(),
            ),
            (
                "banner9.png".to_string(),
                "_fallback/banner9.png".to_string(),
            ),
            (
                "banner10.png".to_string(),
                "_fallback/banner10.png".to_string(),
            ),
            (
                "banner11.png".to_string(),
                "_fallback/banner11.png".to_string(),
            ),
            (
                "banner12.png".to_string(),
                "_fallback/banner12.png".to_string(),
            ),
            (
                "noteskins/bar/tex notes.png".to_string(),
                "noteskins/bar/tex notes.png".to_string(),
            ),
            (
                "noteskins/bar/tex receptors.png".to_string(),
                "noteskins/bar/tex receptors.png".to_string(),
            ),
            (
                "noteskins/bar/tex glow.png".to_string(),
                "noteskins/bar/tex glow.png".to_string(),
            ),
            (
                "judgements/Love 2x7 (doubleres).png".to_string(),
                "judgements/Love 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Love Chroma 2x7 (doubleres).png".to_string(),
                "judgements/Love Chroma 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Rainbowmatic 2x7 (doubleres).png".to_string(),
                "judgements/Rainbowmatic 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/GrooveNights 2x7 (doubleres).png".to_string(),
                "judgements/GrooveNights 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Emoticon 2x7 (doubleres).png".to_string(),
                "judgements/Emoticon 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Censored 1x7 (doubleres).png".to_string(),
                "judgements/Censored 1x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Chromatic 2x7 (doubleres).png".to_string(),
                "judgements/Chromatic 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/ITG2 2x7 (doubleres).png".to_string(),
                "judgements/ITG2 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Bebas 2x7 (doubleres).png".to_string(),
                "judgements/Bebas 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Code 2x7 (doubleres).png".to_string(),
                "judgements/Code 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Comic Sans 2x7 (doubleres).png".to_string(),
                "judgements/Comic Sans 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Focus 2x7 (doubleres).png".to_string(),
                "judgements/Focus 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Grammar 2x7 (doubleres).png".to_string(),
                "judgements/Grammar 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Miso 2x7 (doubleres).png".to_string(),
                "judgements/Miso 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Papyrus 2x7 (doubleres).png".to_string(),
                "judgements/Papyrus 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Roboto 2x7 (doubleres).png".to_string(),
                "judgements/Roboto 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Shift 2x7 (doubleres).png".to_string(),
                "judgements/Shift 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Tactics 2x7 (doubleres).png".to_string(),
                "judgements/Tactics 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Wendy 2x7 (doubleres).png".to_string(),
                "judgements/Wendy 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Wendy Chroma 2x7 (doubleres).png".to_string(),
                "judgements/Wendy Chroma 2x7 (doubleres).png".to_string(),
            ),
            (
                "hold_judgements/Love 1x2 (doubleres).png".to_string(),
                "hold_judgements/Love 1x2 (doubleres).png".to_string(),
            ),
            (
                "hold_judgements/mute 1x2 (doubleres).png".to_string(),
                "hold_judgements/mute 1x2 (doubleres).png".to_string(),
            ),
            (
                "hold_judgements/ITG2 1x2 (doubleres).png".to_string(),
                "hold_judgements/ITG2 1x2 (doubleres).png".to_string(),
            ),
            (
                "grades/grades 1x19.png".to_string(),
                "grades/grades 1x19.png".to_string(),
            ),
        ];

        append_noteskins_pngs(&mut textures_to_load, "noteskins/cel");
        append_noteskins_pngs(&mut textures_to_load, "noteskins/metal");
        append_noteskins_pngs(&mut textures_to_load, "noteskins/enchantment-v2");
        append_noteskins_pngs(&mut textures_to_load, "noteskins/devcel-2024-v3");

        let mut handles = Vec::with_capacity(textures_to_load.len());
        for (key, relative_path) in textures_to_load {
            handles.push(std::thread::spawn(move || {
                let path = if relative_path.starts_with("noteskins/") {
                    Path::new("assets").join(&relative_path)
                } else {
                    Path::new("assets/graphics").join(&relative_path)
                };
                match image::open(&path) {
                    Ok(img) => Ok::<(String, RgbaImage), (String, String)>((key, img.to_rgba8())),
                    Err(e) => Err((key, e.to_string())),
                }
            }));
        }

        let fallback_image = Arc::new(fallback_rgba());
        for h in handles {
            match h.join().expect("texture decode thread panicked") {
                Ok((key, rgba)) => {
                    let texture = backend.create_texture(&rgba, SamplerDesc::default())?;
                    register_texture_dims(&key, rgba.width(), rgba.height());
                    info!("Loaded texture: {}", key);
                    self.textures.insert(key, texture);
                }
                Err((key, msg)) => {
                    warn!(
                        "Failed to load texture for key '{}': {}. Using fallback.",
                        key, msg
                    );
                    let texture =
                        backend.create_texture(&fallback_image, SamplerDesc::default())?;
                    register_texture_dims(&key, fallback_image.width(), fallback_image.height());
                    self.textures.insert(key, texture);
                }
            }
        }

        let profile = profile::get();
        self.set_profile_avatar(backend, profile.avatar_path);

        Ok(())
    }

    fn load_initial_fonts(&mut self, backend: &mut Backend) -> Result<(), Box<dyn Error>> {
        for &name in &[
            "wendy",
            "miso",
            "cjk",
            "emoji",
            "game",
            "wendy_monospace_numbers",
            "wendy_screenevaluation",
            "wendy_combo",
            "combo_arial_rounded",
            "combo_asap",
            "combo_bebas_neue",
            "combo_source_code",
            "combo_work",
            "combo_wendy_cursed",
            "wendy_white",
        ] {
            let ini_path_str = match name {
                "wendy" => "assets/fonts/wendy/_wendy small.ini",
                "miso" => "assets/fonts/miso/_miso light.ini",
                "cjk" => "assets/fonts/cjk/_jfonts 16px.ini",
                "emoji" => "assets/fonts/emoji/_emoji 16px.ini",
                "game" => "assets/fonts/game/_game chars 16px.ini",
                "wendy_monospace_numbers" => "assets/fonts/wendy/_wendy monospace numbers.ini",
                "wendy_screenevaluation" => "assets/fonts/wendy/_ScreenEvaluation numbers.ini",
                "wendy_combo" => "assets/fonts/_combo/wendy/Wendy.ini",
                "combo_arial_rounded" => "assets/fonts/_combo/Arial Rounded/Arial Rounded.ini",
                "combo_asap" => "assets/fonts/_combo/Asap/Asap.ini",
                "combo_bebas_neue" => "assets/fonts/_combo/Bebas Neue/Bebas Neue.ini",
                "combo_source_code" => "assets/fonts/_combo/Source Code/Source Code.ini",
                "combo_work" => "assets/fonts/_combo/Work/Work.ini",
                "combo_wendy_cursed" => "assets/fonts/_combo/Wendy (Cursed)/Wendy (Cursed).ini",
                "wendy_white" => "assets/fonts/wendy/_wendy white.ini",
                _ => return Err(format!("Unknown font name: {}", name).into()),
            };

            let FontLoadData {
                mut font,
                required_textures,
            } = font::parse(ini_path_str)?;

            if name == "miso" {
                font.fallback_font_name = Some("cjk");
                info!("Font 'miso' configured to use 'cjk' as fallback.");
            }

            if name == "cjk" {
                font.fallback_font_name = Some("emoji");
                info!("Font 'cjk' configured to use 'emoji' as fallback.");
            }

            for tex_path in &required_textures {
                let key = canonical_texture_key(tex_path);
                if !self.textures.contains_key(&key) {
                    let hints = font
                        .texture_hints_map
                        .get(&key)
                        .map(|s| parse_texture_hints(s))
                        .unwrap_or_default();
                    let mut image_data = image::open(tex_path)?.to_rgba8();
                    if !hints.is_default() {
                        apply_texture_hints(&mut image_data, &hints);
                    }
                    let texture = backend.create_texture(&image_data, hints.sampler_desc())?;
                    register_texture_dims(&key, image_data.width(), image_data.height());
                    self.textures.insert(key.clone(), texture);
                    info!("Loaded font texture: {}", key);
                }
            }
            self.register_font(name, font);
            info!("Loaded font '{}' from '{}'", name, ini_path_str);
        }
        Ok(())
    }

    // --- Dynamic Asset Management ---

    pub fn destroy_dynamic_assets(&mut self, backend: &mut Backend) {
        if self.current_dynamic_banner.is_some()
            || self.current_density_graph.is_some()
            || self.current_dynamic_background.is_some()
        {
            backend.wait_for_idle(); // Wait for GPU to finish using old textures
            if let Some((key, _)) = self.current_dynamic_banner.take() {
                self.textures.remove(&key);
            }
            if let Some((key, _)) = self.current_density_graph.take() {
                self.textures.remove(&key);
            }
            if let Some((key, _)) = self.current_dynamic_background.take() {
                self.textures.remove(&key);
            }
        }
        self.destroy_current_profile_avatar(backend);
    }

    pub fn set_dynamic_banner(
        &mut self,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) -> String {
        if let Some(path) = path_opt {
            if self
                .current_dynamic_banner
                .as_ref()
                .is_some_and(|(_, p)| p == &path)
            {
                return self.current_dynamic_banner.as_ref().unwrap().0.clone();
            }

            self.destroy_current_dynamic_banner(backend);

            match image::open(&path) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    match backend.create_texture(&rgba, SamplerDesc::default()) {
                        Ok(texture) => {
                            let key = path.to_string_lossy().into_owned();
                            self.textures.insert(key.clone(), texture);
                            register_texture_dims(&key, rgba.width(), rgba.height());
                            self.current_dynamic_banner = Some((key.clone(), path));
                            key
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create GPU texture for {:?}: {}. Using fallback.",
                                path, e
                            );
                            "banner1.png".to_string()
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to open banner image {:?}: {}. Using fallback.",
                        path, e
                    );
                    "banner1.png".to_string()
                }
            }
        } else {
            self.destroy_current_dynamic_banner(backend);
            "banner1.png".to_string()
        }
    }

    pub fn set_density_graph(
        &mut self,
        backend: &mut Backend,
        data: Option<(String, rssp::graph::GraphImageData)>,
    ) -> String {
        const FALLBACK_KEY: &str = "__white";

        if let Some((key, graph_data)) = data {
            if self
                .current_density_graph
                .as_ref()
                .is_some_and(|(_, cache_key)| cache_key == &key)
            {
                return self.current_density_graph.as_ref().unwrap().0.clone();
            }

            self.destroy_current_density_graph(backend);

            let rgba_image =
                match RgbaImage::from_raw(graph_data.width, graph_data.height, graph_data.data) {
                    Some(img) => img,
                    None => {
                        warn!(
                            "Failed to create RgbaImage from raw graph data for key '{}'.",
                            key
                        );
                        return FALLBACK_KEY.to_string();
                    }
                };

            match backend.create_texture(&rgba_image, SamplerDesc::default()) {
                Ok(texture) => {
                    self.textures.insert(key.clone(), texture);
                    register_texture_dims(&key, rgba_image.width(), rgba_image.height());
                    self.current_density_graph = Some((key.clone(), key.clone()));
                    key
                }
                Err(e) => {
                    warn!(
                        "Failed to create GPU texture for density graph ('{}'): {}.",
                        key, e
                    );
                    FALLBACK_KEY.to_string()
                }
            }
        } else {
            self.destroy_current_density_graph(backend);
            FALLBACK_KEY.to_string()
        }
    }

    pub fn set_dynamic_background(
        &mut self,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) -> String {
        const FALLBACK_KEY: &str = "__white";

        if let Some(path) = path_opt {
            if self
                .current_dynamic_background
                .as_ref()
                .is_some_and(|(_, p)| p == &path)
            {
                return self.current_dynamic_background.as_ref().unwrap().0.clone();
            }

            self.destroy_current_dynamic_background(backend);

            match image::open(&path) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    match backend.create_texture(&rgba, SamplerDesc::default()) {
                        Ok(texture) => {
                            let key = path.to_string_lossy().into_owned();
                            self.textures.insert(key.clone(), texture);
                            register_texture_dims(&key, rgba.width(), rgba.height());
                            self.current_dynamic_background = Some((key.clone(), path));
                            key
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create GPU texture for background {:?}: {}. Using fallback.",
                                path, e
                            );
                            FALLBACK_KEY.to_string()
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to open background image {:?}: {}. Using fallback.",
                        path, e
                    );
                    FALLBACK_KEY.to_string()
                }
            }
        } else {
            self.destroy_current_dynamic_background(backend);
            FALLBACK_KEY.to_string()
        }
    }

    pub fn set_profile_avatar(&mut self, backend: &mut Backend, path_opt: Option<PathBuf>) {
        if let Some(path) = path_opt {
            if self
                .current_profile_avatar
                .as_ref()
                .is_some_and(|(_, p)| p == &path)
            {
                if let Some((key, _)) = &self.current_profile_avatar {
                    profile::set_avatar_texture_key(Some(key.clone()));
                }
                return;
            }

            self.destroy_current_profile_avatar(backend);

            match image::open(&path) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    match backend.create_texture(&rgba, SamplerDesc::default()) {
                        Ok(texture) => {
                            let key = path.to_string_lossy().into_owned();
                            self.textures.insert(key.clone(), texture);
                            register_texture_dims(&key, rgba.width(), rgba.height());
                            self.current_profile_avatar = Some((key.clone(), path));
                            profile::set_avatar_texture_key(Some(key));
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create GPU texture for profile avatar {:?}: {}.",
                                path, e
                            );
                            profile::set_avatar_texture_key(None);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to open profile avatar image {:?}: {}.", path, e);
                    profile::set_avatar_texture_key(None);
                }
            }
        } else {
            self.destroy_current_profile_avatar(backend);
        }
    }

    fn destroy_current_dynamic_banner(&mut self, backend: &mut Backend) {
        if let Some((key, _)) = self.current_dynamic_banner.take() {
            backend.wait_for_idle();
            self.textures.remove(&key);
        }
    }

    fn destroy_current_density_graph(&mut self, backend: &mut Backend) {
        if let Some((key, _)) = self.current_density_graph.take() {
            backend.wait_for_idle();
            self.textures.remove(&key);
        }
    }

    fn destroy_current_dynamic_background(&mut self, backend: &mut Backend) {
        if let Some((key, _)) = self.current_dynamic_background.take() {
            backend.wait_for_idle();
            self.textures.remove(&key);
        }
    }

    fn destroy_current_profile_avatar(&mut self, backend: &mut Backend) {
        if let Some((key, _)) = self.current_profile_avatar.take() {
            backend.wait_for_idle();
            self.textures.remove(&key);
        }
        profile::set_avatar_texture_key(None);
    }
}
