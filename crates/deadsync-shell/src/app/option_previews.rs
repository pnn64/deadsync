//! Bounded, session-owned Player Options preview preparation.
//!
//! At most two CPU jobs/results are live at a time. Only the application thread
//! uploads/retires textures, at most one per frame. Runtimes and native textures
//! survive screen exits; only unneeded entries are evicted while browsing options.
//! Existing session textures are borrowed and never counted as eviction candidates.

use deadlib_assets::AssetManager;
use deadlib_render::Backend;
use deadlib_render_core::SamplerDesc;
use deadsync_assets::noteskin::{self, Noteskin, Style};
use deadsync_noteskin::pack::InstalledPack;
use deadsync_theme_simply_love::screens::player_options::{
    self, NoteskinPreviewPriority, NoteskinPreviewRequest, State,
};
use image::RgbaImage;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, mpsc};

const MAX_RUNTIMES: usize = 32;
const MAX_JOBS: usize = 2;
// Conservative native-pixel accounting includes room for mipmaps. Driver
// allocation overhead and textures already owned by the game are additional.
const TEXTURE_BUDGET: usize = 256 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;

struct Runtime {
    skin: Arc<Noteskin>,
    used: u64,
    loaded_parts: u16,
    parts: u16,
    textures: Vec<(Arc<str>, bool)>,
    components: Vec<(u16, Vec<Arc<str>>)>,
}

impl Runtime {
    fn set_parts(&mut self, parts: u16) {
        let parts = parts & self.loaded_parts;
        if self.parts == parts {
            return;
        }
        self.parts = parts;
        self.textures = player_options::noteskin_preview_textures(&self.skin, parts);
        self.components.clear();
        for part in 0..11 {
            let bit = 1 << part;
            if parts & bit != 0 {
                self.components.push((
                    bit,
                    player_options::noteskin_preview_textures(&self.skin, bit)
                        .into_iter()
                        .map(|(key, _)| key)
                        .collect(),
                ));
            }
        }
    }

    fn ready_parts(&self, assets: &AssetManager) -> u16 {
        self.components
            .iter()
            .filter(|(_, textures)| {
                textures
                    .iter()
                    .all(|key| assets.has_uploaded_texture_key(key))
            })
            .fold(0, |parts, (bit, _)| parts | bit)
    }
}

struct Resident {
    bytes: usize,
    used: u64,
}

#[derive(Clone, PartialEq, Eq)]
enum Work {
    Skin(Arc<str>, usize, u16),
    Texture(Arc<str>, bool),
}

enum Ready {
    Skin(Arc<Noteskin>),
    Texture(Arc<RgbaImage>, SamplerDesc),
}

struct Pending {
    work: Work,
    generation: u64,
    result: mpsc::Receiver<Result<Ready, String>>,
}

#[derive(Default)]
pub(super) struct Service {
    runtimes: HashMap<Arc<str>, Runtime>,
    resident: HashMap<Arc<str>, Resident>,
    resident_bytes: usize,
    pending: Vec<Pending>,
    requests: Vec<NoteskinPreviewRequest>,
    last_requests: Vec<NoteskinPreviewRequest>,
    wanted_textures: HashSet<Arc<str>>,
    visible_textures: HashSet<Arc<str>>,
    failed_skins: HashSet<Arc<str>>,
    failed_textures: HashSet<Arc<str>>,
    deferred_textures: HashSet<Arc<str>>,
    catalog: Option<Arc<[InstalledPack]>>,
    cols: usize,
    tick: u64,
    generation: u64,
}

impl Service {
    /// Drain an obsolete result without uploading or retaining decoded pixels
    /// while another screen is active. Never wait for a running decoder.
    pub(super) fn idle(&mut self) {
        self.pending
            .retain(|pending| matches!(pending.result.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }

    pub(super) fn update(
        &mut self,
        state: &mut State,
        assets: &mut AssetManager,
        backend: &mut Backend,
    ) {
        let cols = player_options::take_noteskin_preview_requests(state, &mut self.requests);
        self.update_cache(cols, assets, backend);
        player_options::retain_noteskin_previews(state, &self.requests);
        for request in &self.requests {
            if request.priority == NoteskinPreviewPriority::Nearby {
                continue;
            }
            let ready = self.runtimes.get(&request.name);
            let parts = ready.map_or(0, |runtime| runtime.ready_parts(assets));
            player_options::set_noteskin_preview(
                state,
                &request.name,
                parts,
                ready.map(|runtime| runtime.skin.clone()),
            );
        }
    }

    fn update_cache(&mut self, cols: usize, assets: &mut AssetManager, backend: &mut Backend) {
        let catalog = noteskin::pack_catalog();
        let changed = self.cols != cols
            || self
                .catalog
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, &catalog));
        if changed {
            // Keep the old worker's slot until completion, then discard its
            // result, so catalog/style changes cannot multiply decode workers.
            self.generation = self.generation.wrapping_add(1);
            self.runtimes.clear();
            self.failed_skins.clear();
            self.failed_textures.clear();
            self.cols = cols;
            self.catalog = Some(catalog);
            for key in self.resident.keys() {
                if let Some((_, texture)) = assets.remove_texture(key) {
                    backend.retire_texture(texture);
                }
            }
            self.resident.clear();
            self.resident_bytes = 0;
        }
        self.tick = self.tick.wrapping_add(1);
        // A renderer rebuild may have replaced the asset store between visits.
        self.resident.retain(|key, resident| {
            if assets.has_uploaded_texture_key(key) {
                true
            } else {
                self.resident_bytes -= resident.bytes;
                false
            }
        });
        let demand_changed = self.last_requests != self.requests;
        if demand_changed {
            self.last_requests.clone_from(&self.requests);
        }
        // Budget deferrals can be retried once the visible working set changes.
        self.wanted_textures.clear();
        self.visible_textures.clear();
        for request in &self.requests {
            if let Some(runtime) = self.runtimes.get_mut(&request.name) {
                runtime.used = self.tick;
                runtime.set_parts(request.parts);
                self.wanted_textures
                    .extend(runtime.textures.iter().map(|(key, _)| key.clone()));
                if request.priority != NoteskinPreviewPriority::Nearby {
                    self.visible_textures
                        .extend(runtime.textures.iter().map(|(key, _)| key.clone()));
                }
            }
        }
        for key in &self.wanted_textures {
            if let Some(resident) = self.resident.get_mut(key) {
                resident.used = self.tick;
            }
        }
        if demand_changed {
            self.deferred_textures.clear();
            self.failed_skins
                .retain(|name| self.requests.iter().any(|request| request.name == *name));
            self.failed_textures
                .retain(|key| self.wanted_textures.contains(key));
        }
        self.poll(assets, backend);
        // Cache state may have changed after receiving a runtime or uploading a
        // texture. Publish only complete components; no render-thread miss loads.
        while self.pending.len() < MAX_JOBS {
            let Some(work) = self.next_work(assets) else {
                break;
            };
            self.start(work);
        }
    }

    fn next_work(&mut self, assets: &AssetManager) -> Option<Work> {
        for request in &self.requests {
            let loaded = self
                .runtimes
                .get(&request.name)
                .map_or(0, |runtime| runtime.loaded_parts);
            if request.parts & !loaded != 0
                && !self.failed_skins.contains(&request.name)
                // Variants may share a compiler-cache file. Runtime preparation
                // stays serial while overlapping it with native image decoding.
                && !self.pending.iter().any(|pending| matches!(pending.work, Work::Skin(..)))
            {
                return Some(Work::Skin(
                    request.name.clone(),
                    self.cols,
                    request.parts | loaded,
                ));
            }
            if let Some(runtime) = self.runtimes.get_mut(&request.name) {
                runtime.set_parts(request.parts);
                for (key, model) in &runtime.textures {
                    if !assets.has_uploaded_texture_key(key)
                        && !self.failed_textures.contains(key)
                        && !self.deferred_textures.contains(key)
                        && !self.pending.iter().any(|pending| matches!(&pending.work, Work::Texture(pending_key, _) if pending_key == key))
                    {
                        return Some(Work::Texture(key.clone(), *model));
                    }
                }
            }
        }
        None
    }

    fn start(&mut self, work: Work) {
        let (tx, rx) = mpsc::sync_channel(1);
        let task = work.clone();
        match std::thread::Builder::new()
            .name("option-preview".into())
            .spawn(move || {
                let started = std::time::Instant::now();
                let kind = if matches!(task, Work::Skin(..)) {
                    "runtime"
                } else {
                    "texture"
                };
                let result = match task {
                    Work::Skin(name, cols, parts) => noteskin::load_itg_preview(
                        &Style {
                            num_cols: cols,
                            num_players: 1,
                        },
                        &name,
                        native_parts(parts),
                    )
                    .map(Ready::Skin),
                    Work::Texture(key, model) => decode(&key, model),
                };
                log::debug!(
                    "Options preview {kind} prepared in {:.1}ms",
                    started.elapsed().as_secs_f64() * 1000.0
                );
                #[cfg(test)]
                if std::env::var_os("DEADSYNC_PREVIEW_TIMINGS").is_some() {
                    eprintln!(
                        "Preview {kind}: {:.1}ms",
                        started.elapsed().as_secs_f64() * 1000.0
                    );
                }
                let _ = tx.send(result);
            }) {
            Ok(_) => self.pending.push(Pending {
                work,
                generation: self.generation,
                result: rx,
            }),
            Err(error) => self.fail(&work, &error.to_string()),
        }
    }

    fn poll(&mut self, assets: &mut AssetManager, backend: &mut Backend) {
        // Poll every slot so a slow background choice cannot hold up a ready
        // focused choice. Consume at most one result/upload per frame.
        let Some((index, result)) = self
            .pending
            .iter()
            .enumerate()
            .find_map(|(index, pending)| match pending.result.try_recv() {
                Ok(result) => Some((index, result)),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some((index, Err("preview worker disconnected".into())))
                }
            })
        else {
            return;
        };
        let pending = self.pending.remove(index);
        if pending.generation != self.generation {
            return;
        }
        match (&pending.work, result) {
            (Work::Skin(name, cols, parts), Ok(Ready::Skin(skin)))
                if *cols == self.cols && !name.is_empty() =>
            {
                let Some(requested_parts) = self
                    .requests
                    .iter()
                    .find(|request| request.name == *name)
                    .map(|request| request.parts)
                else {
                    return;
                };
                if self.runtimes.len() >= MAX_RUNTIMES && !self.runtimes.contains_key(name) {
                    let oldest = self
                        .runtimes
                        .iter()
                        .filter(|(name, _)| {
                            !self.requests.iter().any(|request| request.name == **name)
                        })
                        .min_by_key(|(_, runtime)| runtime.used)
                        .map(|(name, _)| name.clone());
                    if let Some(oldest) = oldest {
                        self.runtimes.remove(&oldest);
                    } else {
                        self.fail(&pending.work, "visible runtime budget exceeded");
                        return;
                    }
                }
                let mut runtime = Runtime {
                    skin,
                    used: self.tick,
                    loaded_parts: *parts,
                    parts: 0,
                    textures: Vec::new(),
                    components: Vec::new(),
                };
                // Publish readiness immediately, even if another focused job
                // fills the last worker slot before next_work visits this name.
                runtime.set_parts(requested_parts);
                self.runtimes.insert(name.clone(), runtime);
            }
            (Work::Texture(key, _), Ok(Ready::Texture(image, sampler))) => {
                // Navigation may have made the decode obsolete while it ran.
                if !self.wanted_textures.contains(key) || assets.has_uploaded_texture_key(key) {
                    return;
                }
                let bytes = texture_bytes(&image, sampler);
                if !self.make_room(bytes, self.visible_textures.contains(key), assets, backend) {
                    self.deferred_textures.insert(key.clone());
                    log::debug!("Deferring native preview '{key}': visible texture budget is full");
                    return;
                }
                match assets.update_texture_for_key_with_sampler(backend, key, &image, sampler) {
                    Ok(()) => {
                        self.resident.insert(
                            key.clone(),
                            Resident {
                                bytes,
                                used: self.tick,
                            },
                        );
                        self.resident_bytes += bytes;
                        log::debug!(
                            "Options preview cache: {} runtimes, {} textures, {:.1} MiB budgeted",
                            self.runtimes.len(),
                            self.resident.len(),
                            self.resident_bytes as f64 / 1048576.0
                        );
                    }
                    Err(error) => self.fail(&pending.work, &error.to_string()),
                }
            }
            (_, Err(error)) => self.fail(&pending.work, &error),
            _ => {} // Result from a previous catalog/style generation.
        }
    }

    fn make_room(
        &mut self,
        bytes: usize,
        visible: bool,
        assets: &mut AssetManager,
        backend: &mut Backend,
    ) -> bool {
        while self.resident_bytes.saturating_add(bytes) > TEXTURE_BUDGET {
            let oldest = eviction_candidate(
                &self.resident,
                if visible {
                    &self.visible_textures
                } else {
                    &self.wanted_textures
                },
            );
            let Some(oldest) = oldest else { return false };
            let old = self
                .resident
                .remove(&oldest)
                .expect("eviction candidate exists");
            self.resident_bytes -= old.bytes;
            if let Some((_, texture)) = assets.remove_texture(&oldest) {
                backend.retire_texture(texture);
            }
        }
        true
    }

    fn fail(&mut self, work: &Work, error: &str) {
        let key = match work {
            Work::Skin(name, ..) => {
                self.failed_skins.insert(name.clone());
                name
            }
            Work::Texture(key, _) => {
                self.failed_textures.insert(key.clone());
                key
            }
        };
        log::warn!("Cannot prepare options preview '{key}': {error}");
    }
}

fn native_parts(bits: u16) -> deadsync_noteskin::runtime::SkinParts {
    use deadsync_noteskin::runtime::{SkinPart, SkinParts};
    [
        SkinPart::Arrows,
        SkinPart::Receptors,
        SkinPart::HoldActive,
        SkinPart::HoldInactive,
        SkinPart::RollActive,
        SkinPart::RollInactive,
        SkinPart::TapExplosions,
        SkinPart::HoldExplosions,
        SkinPart::Mines,
        SkinPart::Mines,
        SkinPart::Lifts,
    ]
    .into_iter()
    .enumerate()
    .filter(|(index, _)| bits & (1 << index) != 0)
    .fold(SkinParts::default(), |parts, (_, part)| parts.with(part))
}

fn eviction_candidate(
    resident: &HashMap<Arc<str>, Resident>,
    wanted: &HashSet<Arc<str>>,
) -> Option<Arc<str>> {
    resident
        .iter()
        .filter(|(key, _)| !wanted.contains(*key))
        .min_by_key(|(_, resident)| resident.used)
        .map(|(key, _)| key.clone())
}

fn texture_bytes(image: &RgbaImage, sampler: SamplerDesc) -> usize {
    image
        .as_raw()
        .len()
        .saturating_mul(if sampler.mipmaps { 2 } else { 1 })
}

fn decode(key: &str, model: bool) -> Result<Ready, String> {
    if let Some(generated) = deadlib_assets::generated_texture(key) {
        if generated.image.as_raw().len() as u64 > MAX_IMAGE_BYTES {
            return Err("native preview image exceeds 64 MiB".into());
        }
        return Ok(Ready::Texture(generated.image, generated.sampler));
    }
    let job = deadsync_assets::textures::texture_decode_job(key, model);
    let reader = image::ImageReader::open(&job.path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|e| e.to_string())?;
    let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
    if u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(4)
        > MAX_IMAGE_BYTES
    {
        return Err("native preview image exceeds 64 MiB".into());
    }
    // Use precisely the gameplay decoder (format fallback, texture hints, and
    // hidden-alpha treatment). Only scheduling and residency differ here.
    let image =
        deadlib_assets::decode_texture_image(&job.path, &job.hints).map_err(|e| e.to_string())?;
    if image.as_raw().len() as u64 > MAX_IMAGE_BYTES {
        return Err("native preview image exceeds 64 MiB".into());
    }
    Ok(Ready::Texture(Arc::new(image), job.sampler))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eviction_preserves_visible_sources_and_chooses_oldest_hidden_source() {
        let resident = HashMap::from([
            (Arc::from("visible"), Resident { bytes: 10, used: 0 }),
            (Arc::from("recent"), Resident { bytes: 20, used: 5 }),
            (Arc::from("old"), Resident { bytes: 30, used: 1 }),
        ]);
        let wanted = HashSet::from([Arc::from("visible")]);
        assert_eq!(
            eviction_candidate(&resident, &wanted).as_deref(),
            Some("old")
        );
        let wanted = resident.keys().cloned().collect();
        assert!(eviction_candidate(&resident, &wanted).is_none());
    }

    #[test]
    fn idle_discards_completed_pixels_and_never_waits_for_a_worker() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut service = Service::default();
        service.pending.push(Pending {
            work: Work::Texture(Arc::from("unused"), false),
            generation: 0,
            result: rx,
        });
        service.idle();
        assert!(!service.pending.is_empty());
        let image = Arc::new(RgbaImage::new(8, 8));
        tx.send(Ok(Ready::Texture(image.clone(), SamplerDesc::default())))
            .unwrap();
        service.idle();
        assert!(service.pending.is_empty());
        assert_eq!(
            Arc::strong_count(&image),
            1,
            "decoded source is released after leaving options"
        );
    }

    #[cfg(all(
        target_os = "windows",
        not(target_pointer_width = "32"),
        not(target_vendor = "win7")
    ))]
    #[test]
    #[allow(deprecated)]
    #[ignore = "requires DEADSYNC_WORKSHOP_FIXTURE; exercises native uploads with a hidden software window"]
    fn workshop_preview_cache_benchmark() {
        use std::time::{Duration, Instant};
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let root = std::path::PathBuf::from(
            std::env::var_os("DEADSYNC_WORKSHOP_FIXTURE").expect("fixture path"),
        )
        .canonicalize()
        .unwrap();
        let exe = root.parent().unwrap().parent().unwrap().parent().unwrap();
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/options-preview-fixture");
        let dirs = deadsync_config::dirs::AppDirs {
            cache_dir: data.join("cache"),
            data_dir: data,
            exe_dir: exe.to_owned(),
            portable: false,
        };
        deadsync_assets::init_paths(dirs.asset_paths(None)).unwrap();
        let pack = noteskin::pack_catalog()
            .iter()
            .find(|pack| pack.root == root)
            .cloned()
            .expect("installed Workshop catalog");
        let event_loop = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        let window = event_loop
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
            )
            .unwrap();
        let gpu = std::env::var_os("DEADSYNC_PREVIEW_WGPU").is_some();
        let mut backend = deadlib_render::create_backend(
            if gpu {
                deadlib_render_core::BackendType::VulkanWgpu
            } else {
                deadlib_render_core::BackendType::Software
            },
            Arc::new(window),
            deadlib_render_core::ProjectionMatrix::IDENTITY,
            false,
            deadlib_render_core::PresentModePolicy::Immediate,
            false,
            false,
        )
        .unwrap();
        let mut assets = AssetManager::new();
        let mut service = Service::default();
        let frame = deadlib_render_core::RenderFrame {
            clear_color: [0.0; 4],
            render_targets: vec![],
            cameras: vec![],
            sprite_instances: vec![],
            mesh_vertices: vec![],
            tmesh_instances: vec![],
            tmesh_geometries: vec![],
            ops: vec![],
        };
        let choices: Vec<_> = pack
            .manifest
            .skins
            .iter()
            .flat_map(|skin| {
                skin.options
                    .iter()
                    .filter(|choice| choice.slot == "arrows")
                    .step_by(3)
                    .take(24)
                    .map(|choice| NoteskinPreviewRequest {
                        name: Arc::from(if choice.id == "base" {
                            skin.id.clone()
                        } else {
                            format!("{}?arrows={}", skin.id, choice.id)
                        }),
                        parts: 1,
                        priority: NoteskinPreviewPriority::Visible,
                    })
            })
            .collect();
        assert!(choices.len() > MAX_RUNTIMES);
        // Compare construction only; neither path retains a full runtime in the
        // gameplay cache. Alternate order with filesystem/compiler caches warm.
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        {
            let name = &choices[0].name;
            let partial = noteskin::load_itg_preview(&style, name, native_parts(1)).unwrap();
            assert!(partial.receptor_off.is_empty());
            let gameplay = noteskin::load_itg_skin_cached(&style, name).unwrap();
            assert!(
                !gameplay.receptor_off.is_empty(),
                "preview must not populate the gameplay cache"
            );
            assert!(!Arc::ptr_eq(&partial, &gameplay));
            let reused = noteskin::load_itg_preview(&style, name, native_parts(1)).unwrap();
            assert!(
                Arc::ptr_eq(&reused, &gameplay),
                "reuse a resident full gameplay runtime"
            );
        }
        for request in &choices {
            drop(noteskin::load_itg_skin(&style, &request.name).unwrap());
            drop(noteskin::load_itg_preview(&style, &request.name, native_parts(1)).unwrap());
        }
        let mut preparation = [Duration::ZERO; 2];
        let mut slots = [0usize; 2];
        for round in 0..3 {
            for (index, request) in choices.iter().enumerate() {
                for mode in [0, 1].map(|mode| (mode + index + round) % 2) {
                    let started = Instant::now();
                    let skin = if mode == 0 {
                        Arc::new(noteskin::load_itg_skin(&style, &request.name).unwrap())
                    } else {
                        noteskin::load_itg_preview(&style, &request.name, native_parts(1)).unwrap()
                    };
                    preparation[mode] += started.elapsed();
                    skin.for_each_slot(|_| slots[mode] += 1);
                    if mode == 1 {
                        assert!(!skin.notes.is_empty());
                        assert!(skin.receptor_off.is_empty());
                        assert!(skin.hold_columns.is_empty());
                        assert!(skin.mines.is_empty());
                        assert!(skin.mine_hit_explosion.is_none());
                    }
                }
            }
        }
        let samples = choices.len() * 3;
        eprintln!(
            "Workshop runtime preparation ({samples} loads, mean): full {:.3}ms, arrows {:.3}ms; slot references {}/{}",
            preparation[0].as_secs_f64() * 1000.0 / samples as f64,
            preparation[1].as_secs_f64() * 1000.0 / samples as f64,
            slots[0] / samples,
            slots[1] / samples
        );
        let mut peak = 0;
        let mut max_tick = Duration::ZERO;
        for (page, choices) in choices.chunks(8).enumerate() {
            service.requests = choices.to_vec();
            let started = Instant::now();
            let mut first_ready = None;
            loop {
                let tick = Instant::now();
                service.update_cache(4, &mut assets, &mut backend);
                max_tick = max_tick.max(tick.elapsed());
                if gpu {
                    backend.draw(&frame, assets.textures(), true).unwrap();
                }
                peak = peak.max(service.resident_bytes);
                assert!(service.runtimes.len() <= MAX_RUNTIMES);
                assert!(service.resident_bytes <= TEXTURE_BUDGET);
                assert!(service.failed_skins.is_empty());
                assert!(service.failed_textures.is_empty());
                if first_ready.is_none()
                    && service
                        .runtimes
                        .get(&choices[0].name)
                        .is_some_and(|runtime| runtime.ready_parts(&assets) & 1 != 0)
                {
                    first_ready = Some(started.elapsed());
                }
                assert!(
                    service.deferred_textures.is_empty(),
                    "eight Workshop arrows fit the native texture budget"
                );
                assert!(service.pending.len() <= MAX_JOBS);
                if service.pending.is_empty()
                    && choices.iter().all(|request| {
                        service.runtimes.get(&request.name).is_some_and(|runtime| {
                            runtime.ready_parts(&assets) & request.parts == request.parts
                        })
                    })
                {
                    break;
                }
                assert!(
                    started.elapsed() < Duration::from_secs(180),
                    "preview worker stalled"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            eprintln!(
                "Workshop page {page}: {} visible variants ready in {:.3}s, first {:.3}s, {:.1} MiB resident",
                choices.len(),
                started.elapsed().as_secs_f64(),
                first_ready.unwrap().as_secs_f64(),
                service.resident_bytes as f64 / 1048576.0
            );
        }
        // A screen's state can be dropped while the service retains ready entries.
        let name = service.requests[0].name.clone();
        let skin = service.runtimes[&name].skin.clone();
        let requests = service.requests.clone();
        service.requests.clear();
        service.update_cache(4, &mut assets, &mut backend);
        service.requests = requests;
        let started = Instant::now();
        service.update_cache(4, &mut assets, &mut backend);
        assert!(
            service.pending.is_empty(),
            "warm re-entry performs no runtime load or decode"
        );
        assert!(Arc::ptr_eq(&skin, &service.runtimes[&name].skin));
        eprintln!(
            "Workshop warm re-entry {:.3}ms, peak {:.1} MiB, largest service tick {:.3}ms",
            started.elapsed().as_secs_f64() * 1000.0,
            peak as f64 / 1048576.0,
            max_tick.as_secs_f64() * 1000.0
        );
        // Compare a ready native upload to the ordinary source decode.
        let (key, model) = &service.runtimes[&name].textures[0];
        let handle = assets.texture_context().texture_handle(key);
        let job = deadsync_assets::textures::texture_decode_job(key, *model);
        let source = deadlib_assets::decode_texture_image(&job.path, &job.hints).unwrap();
        if !gpu {
            let Some(deadlib_render::Texture::Software(texture)) = assets.textures().get(&handle)
            else {
                panic!("software native texture")
            };
            assert_eq!(texture.image, source, "preview keeps original pixels");
        }

        let expanded_parts = 1 | (1 << 8);
        service.requests[0].parts = expanded_parts;
        service.update_cache(4, &mut assets, &mut backend);
        assert_eq!(
            service.runtimes[&name].ready_parts(&assets),
            1,
            "a missing component must not become ready via an empty texture list"
        );
        assert!(
            Arc::ptr_eq(&skin, &service.runtimes[&name].skin),
            "the old snapshot stays visible while native components load"
        );
        let started = Instant::now();
        loop {
            service.update_cache(4, &mut assets, &mut backend);
            if gpu {
                backend.draw(&frame, assets.textures(), true).unwrap();
            }
            let runtime = &service.runtimes[&name];
            assert_eq!(
                runtime.ready_parts(&assets) & 1,
                1,
                "a ready arrow must remain visible throughout component expansion"
            );
            assert!(service.failed_skins.is_empty());
            assert!(service.failed_textures.is_empty());
            assert!(service.resident_bytes <= TEXTURE_BUDGET);
            if service.pending.is_empty() && runtime.ready_parts(&assets) == expanded_parts {
                assert!(!Arc::ptr_eq(&skin, &runtime.skin));
                assert!(!runtime.skin.notes.is_empty());
                assert!(!runtime.skin.mines.is_empty());
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "component expansion stalled"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        eprintln!(
            "Workshop component expansion {:.3}ms; arrow remained ready",
            started.elapsed().as_secs_f64() * 1000.0
        );

        // The first page has been evicted. Warm an adjacent choice using the
        // normal runtime/decode path, then select it without any new work.
        service.requests = choices[..2].to_vec();
        service.requests[0].priority = NoteskinPreviewPriority::Focused;
        service.requests[1].priority = NoteskinPreviewPriority::Nearby;
        let started = Instant::now();
        loop {
            service.update_cache(4, &mut assets, &mut backend);
            if gpu {
                backend.draw(&frame, assets.textures(), true).unwrap();
            }
            assert!(service.resident_bytes <= TEXTURE_BUDGET);
            if service.pending.is_empty()
                && service.requests.iter().all(|request| {
                    service
                        .runtimes
                        .get(&request.name)
                        .is_some_and(|runtime| runtime.ready_parts(&assets) & 1 != 0)
                })
            {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "nearby preview was not prepared"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let nearby = service.requests[1].name.clone();
        let skin = service.runtimes[&nearby].skin.clone();
        service.requests.remove(0);
        service.requests[0].priority = NoteskinPreviewPriority::Focused;
        let started = Instant::now();
        service.update_cache(4, &mut assets, &mut backend);
        assert!(
            service.pending.is_empty(),
            "selecting a prefetched neighbor performs no load/decode"
        );
        assert!(Arc::ptr_eq(&skin, &service.runtimes[&nearby].skin));
        assert_eq!(service.runtimes[&nearby].ready_parts(&assets) & 1, 1);
        eprintln!(
            "Workshop prefetched style change {:.3}ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}
