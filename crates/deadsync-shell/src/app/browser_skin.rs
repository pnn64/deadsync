//! The reader's own noteskin, for the Content Browser's chart preview.
//!
//! Loaded once the browser is open rather than on the first preview: a Lua
//! skin can take a noticeable moment to compile, and the window should not
//! open on bare squares. Everything slow runs on one worker -- the compile,
//! the model geometry, and the decode of every texture the window draws, which
//! is the window's own pieces rather than the whole skin. The application
//! thread only uploads, one decoded texture per frame, the split and the rate
//! Player Options' previews use, so a heavy skin never stalls a frame. Nothing
//! is evicted on the way out: the runtime cache and the textures are the ones
//! gameplay uses too.

use deadlib_assets::AssetManager;
use deadlib_render::Backend;
use deadlib_render_core::SamplerDesc;
use deadsync_assets::noteskin::{self, Noteskin, Style};
use deadsync_theme_simply_love::screens::content_browser::{self as browser, PreviewSkinModels};
use image::RgbaImage;
use std::sync::{Arc, mpsc};

/// Decoded textures the worker may hold ready ahead of the uploads. Small, so
/// a skin's pixels wait on disk rather than in memory while the application
/// thread takes them one a frame.
const DECODED_AHEAD: usize = 2;

/// What the worker hands over, in order: the skin, each texture, then done.
/// The geometry is boxed: it is far bigger than any other message.
enum Loaded {
    Skin(Arc<Noteskin>, Box<PreviewSkinModels>),
    Texture(String, Arc<RgbaImage>, SamplerDesc),
    Done,
    Failed(String),
}

#[derive(Default)]
pub(super) struct Service {
    /// The skin the browser wants, set on the way in.
    wanted: Option<String>,
    /// The skin being loaded; a different wanted name starts over.
    name: Option<String>,
    loading: Option<mpsc::Receiver<Loaded>>,
    skin: Option<Arc<Noteskin>>,
    /// The skin's model geometry, until the browser takes it.
    models: Option<PreviewSkinModels>,
    /// Every texture the window draws with is resident.
    ready: bool,
    /// The textures the window draws with, as uploaded.
    keys: Vec<String>,
    /// Check, once, that those textures are still resident: set on the way in.
    verify: bool,
}

impl Service {
    /// The skin to have ready: read from the joined players on the way into
    /// the browser, so nothing reads profiles per frame.
    pub(super) fn want(&mut self, name: String) {
        self.wanted = Some(name);
        self.verify = true;
    }

    /// Keep the wanted skin loading and uploading. Returns it once every
    /// texture the window draws with is resident, and `None` until then -- or
    /// for good, when it would not load, and the window falls back to plain
    /// shapes.
    pub(super) fn update(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
    ) -> Option<Arc<Noteskin>> {
        // Compared in place: the name is only copied when it changes, not on
        // every frame the browser is open.
        let wanted = self.wanted.as_deref()?;
        // Once a visit, as soon as the skin is ready: a renderer switch drops
        // every texture, and the window would otherwise draw this skin's
        // arrows with nothing behind them. Loading again only uploads what is
        // missing, and the compiled skin comes from the runtime cache.
        let mut gone = false;
        if self.ready && self.verify {
            self.verify = false;
            gone = self
                .keys
                .iter()
                .any(|key| !assets.has_uploaded_texture_key(key));
        }
        if gone || self.name.as_deref() != Some(wanted) {
            let wanted = wanted.to_owned();
            // Dropping the old receiver ends the old worker at its next send.
            *self = Self {
                wanted: Some(wanted.clone()),
                name: Some(wanted.clone()),
                ..Self::default()
            };
            let (tx, rx) = mpsc::sync_channel(DECODED_AHEAD);
            let spawned = std::thread::Builder::new()
                .name("browser-noteskin".to_owned())
                .spawn(move || load(&wanted, &tx));
            if spawned.is_ok() {
                self.loading = Some(rx);
            }
        }

        // One message a frame: a texture costs an upload.
        if let Some(rx) = self.loading.as_ref() {
            match rx.try_recv() {
                Ok(Loaded::Skin(skin, models)) => {
                    self.skin = Some(skin);
                    self.models = Some(*models);
                }
                Ok(Loaded::Texture(key, image, sampler)) => {
                    // Up already -- gameplay's, with gameplay's sampler -- stays.
                    if !assets.has_uploaded_texture_key(&key)
                        && let Err(error) = assets
                            .update_texture_for_key_with_sampler(backend, &key, &image, sampler)
                    {
                        log::warn!("Chart preview: texture '{key}' did not upload: {error}");
                    }
                    self.keys.push(key);
                }
                Ok(Loaded::Done) => {
                    self.ready = self.skin.is_some();
                    self.loading = None;
                }
                Ok(Loaded::Failed(error)) => {
                    log::warn!(
                        "Chart preview: noteskin '{}' did not load: {error}",
                        self.name.as_deref().unwrap_or_default()
                    );
                    self.loading = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.loading = None,
            }
        }

        if !self.ready {
            return None;
        }
        self.skin.clone()
    }

    /// The ready skin's model geometry, handed over once: the browser keeps
    /// it from then on.
    pub(super) fn take_models(&mut self) -> Option<PreviewSkinModels> {
        if !self.ready {
            return None;
        }
        self.models.take()
    }
}

/// The worker: compile the skin, build what the window needs of it, and
/// decode its textures one at a time as the application thread takes them.
/// Stops at the first send nobody is waiting for.
fn load(name: &str, tx: &mpsc::SyncSender<Loaded>) {
    let style = Style {
        num_cols: 4,
        num_players: 1,
    };
    let skin = match noteskin::load_itg_skin_cached(&style, name) {
        Ok(skin) => skin,
        Err(error) => {
            let _ = tx.send(Loaded::Failed(error));
            return;
        }
    };
    let textures = browser::preview_skin_textures(&skin);
    let models = browser::preview_skin_models(&skin);
    if tx
        .send(Loaded::Skin(Arc::clone(&skin), Box::new(models)))
        .is_err()
    {
        return;
    }
    for (key, model) in textures {
        let key = deadsync_assets::textures::canonical_texture_key(key.as_ref());
        match decode(&key, model) {
            Ok(Some((image, sampler))) => {
                if tx.send(Loaded::Texture(key, image, sampler)).is_err() {
                    return;
                }
            }
            Ok(None) => {}
            Err(error) => log::warn!("Chart preview: texture '{key}' did not decode: {error}"),
        }
    }
    let _ = tx.send(Loaded::Done);
}

/// One texture's pixels and sampler, exactly as gameplay's loader would
/// decode them: a generated texture's own image, else the file through the
/// gameplay decoder with the key's hints. A model's texture repeats, as
/// gameplay samples it. `None` for a key with nothing behind it to decode.
fn decode(key: &str, model: bool) -> Result<Option<(Arc<RgbaImage>, SamplerDesc)>, String> {
    if key.is_empty() {
        return Ok(None);
    }
    if let Some(generated) = deadlib_assets::generated_texture(key) {
        let sampler = if model {
            deadsync_assets::textures::model_texture_sampler(key)
        } else {
            generated.sampler
        };
        return Ok(Some((generated.image, sampler)));
    }
    // Generator-owned keys have no file; their generator uploads them.
    if key.starts_with("__") {
        return Ok(None);
    }
    let job = deadsync_assets::textures::texture_decode_job(key, model);
    let image =
        deadlib_assets::decode_texture_image(&job.path, &job.hints).map_err(|e| e.to_string())?;
    Ok(Some((Arc::new(image), job.sampler)))
}

/// The noteskin the chart preview draws with: the first joined player's -- the
/// original uses Player 1's -- else the machine default, for a browser opened
/// from the title menu with nobody joined.
pub(super) fn preview_noteskin_name() -> String {
    use deadsync_profile::compat as profile;
    for side in [
        deadsync_profile::PlayerSide::P1,
        deadsync_profile::PlayerSide::P2,
    ] {
        if profile::is_session_side_joined(side) {
            return profile::get_for_side(side).noteskin.to_string();
        }
    }
    deadsync_config::runtime::machine_default_noteskin()
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_render_core::SamplerWrap;

    /// The worker hands over the skin, then every texture the window draws,
    /// decoded -- a model's set to repeat, as gameplay samples it -- then
    /// done, and nothing the window does not draw.
    #[test]
    fn the_worker_decodes_only_what_the_window_draws() {
        crate::tests::init_paths();
        let (tx, rx) = mpsc::sync_channel(DECODED_AHEAD);
        let worker = std::thread::spawn(move || load("cyber", &tx));
        let Ok(Loaded::Skin(skin, _)) = rx.recv() else {
            panic!("the skin comes first");
        };
        let wanted: Vec<String> = browser::preview_skin_textures(&skin)
            .iter()
            .map(|(key, _)| deadsync_assets::textures::canonical_texture_key(key.as_ref()))
            .collect();
        let (mut textures, mut repeating) = (0, 0);
        loop {
            match rx.recv().expect("the worker finishes") {
                Loaded::Texture(key, image, sampler) => {
                    textures += 1;
                    assert!(image.width() > 0 && image.height() > 0, "{key}");
                    assert!(wanted.contains(&key), "'{key}' is not drawn by the window");
                    repeating += usize::from(sampler.wrap == SamplerWrap::Repeat);
                }
                Loaded::Done => break,
                Loaded::Skin(..) => panic!("one skin"),
                Loaded::Failed(error) => panic!("{error}"),
            }
        }
        worker.join().expect("the worker exits");
        assert_eq!(textures, wanted.len(), "every one decoded");
        assert!(repeating > 0, "cyber's models repeat their textures");

        assert!(matches!(decode("", false), Ok(None)));
        assert!(matches!(decode("__no_generator", true), Ok(None)));
    }

    /// A worker nobody is listening to any more stops rather than decoding
    /// the rest of a skin into nothing.
    #[test]
    fn an_abandoned_worker_stops() {
        crate::tests::init_paths();
        let (tx, rx) = mpsc::sync_channel(DECODED_AHEAD);
        let worker = std::thread::spawn(move || load("default", &tx));
        assert!(matches!(rx.recv(), Ok(Loaded::Skin(..))));
        drop(rx);
        worker.join().expect("the worker exits");
    }
}
