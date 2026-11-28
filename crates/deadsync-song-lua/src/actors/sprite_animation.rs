//! Retained Sprite::Update state. Live sprites advance on their own actor clock;
//! rendering only publishes the resulting frame and texture coordinates.
use super::*;

#[derive(Clone)]
pub(super) struct State {
    pub(super) states: Arc<[crate::SongLuaSpriteState]>,
    rects: Arc<[[f32; 4]]>,
    pub(super) index: usize,
    seconds: f32,
    custom: Option<[f32; 4]>,
    image_to_texture: [f32; 2],
    texture_size: [f32; 2],
    render_uses_backing_coords: bool,
    resource: Option<String>,
    movie: Option<Arc<Mutex<Movie>>>,
    skip_movie_update: bool,
    pub(super) use_effect_clock_for_texcoords: bool,
}

impl State {
    pub(super) fn is_movie(&self) -> bool {
        self.movie.is_some()
    }

    pub(super) fn reset(&mut self) {
        self.set_state(0);
        self.skip_movie_update = true;
    }

    pub(super) fn set_state(&mut self, index: u32) -> u32 {
        self.index = (index as usize).min(self.states.len().saturating_sub(1));
        self.seconds = 0.0;
        self.index as u32
    }

    pub(super) fn set_delays(&mut self, delay: f32) {
        for state in Arc::make_mut(&mut self.states) {
            state.delay = delay;
        }
    }

    pub(super) fn length(&self) -> f32 {
        self.states
            .iter()
            .fold(0.0, |length, state| length + state.delay)
    }

    pub(super) fn active_rect(&self) -> [f32; 4] {
        self.custom.unwrap_or_else(|| {
            self.rects
                .get(self.index)
                .copied()
                .unwrap_or([0.0, 0.0, 1.0, 1.0])
        })
    }

    pub(super) fn image_rect(&self, rect: [f32; 4]) -> [f32; 4] {
        std::array::from_fn(|i| rect[i] * self.image_to_texture[i % 2])
    }

    pub(super) fn pixel_offset(&self, delta: [f32; 2]) -> [f32; 2] {
        std::array::from_fn(|i| delta[i] / self.texture_size[i])
    }

    pub(super) fn set_rect(&mut self, rect: [f32; 4]) {
        self.custom = Some(rect);
    }

    fn set_states(&mut self, states: Vec<crate::SongLuaSpriteState>, sheet: (u32, u32)) {
        let width = self.image_to_texture[0] / sheet.0.max(1) as f32;
        let height = self.image_to_texture[1] / sheet.1.max(1) as f32;
        self.rects = states
            .iter()
            .map(|state| {
                // RageTexture wraps authored frame numbers independently of
                // Sprite's logical animation-state index.
                let frame = state.frame % crate::sprite_frame_count(Some(sheet));
                let left = (frame % sheet.0.max(1)) as f32 * width;
                let top = (frame / sheet.0.max(1)) as f32 * height;
                [left, top, left + width, top + height]
            })
            .collect();
        self.states = states.into();
        self.index = self.index.min(self.states.len().saturating_sub(1));
    }
}

pub(super) struct Movie {
    position: f32,
    rate: f32,
    looping: bool,
}

// RageTextureManager gives sprites with the same texture ID one movie clock.
// Sprites keep individual animate and decode flags. Loop and rate belong to
// the shared texture, including when set through a sprite that never updates.
fn bind_movie(
    store: &mut Store,
    actor: TableId,
    resource: Option<&str>,
) -> Option<Arc<Mutex<Movie>>> {
    let Some(resource) = resource else {
        store.movie_sprites.remove(&actor);
        return None;
    };
    store.movie_sprites.insert(actor);
    if let Some(movie) = store
        .movie_textures
        .get(resource)
        .and_then(std::sync::Weak::upgrade)
    {
        return Some(movie);
    }
    store
        .movie_textures
        .retain(|_, movie| movie.strong_count() != 0);
    let movie = Arc::new(Mutex::new(Movie {
        position: 0.0,
        rate: 1.0,
        looping: true,
    }));
    store
        .movie_textures
        .insert(resource.to_owned(), Arc::downgrade(&movie));
    Some(movie)
}

impl Movie {
    fn for_actor(actor: &Actor) -> Option<std::sync::MutexGuard<'_, Self>> {
        Some(
            actor
                .sprite_animation
                .as_ref()?
                .movie
                .as_ref()?
                .lock()
                .unwrap(),
        )
    }
}

pub(super) fn movie_sample(actor: &Actor) -> Option<(f32, bool)> {
    let movie = Movie::for_actor(actor)?;
    Some((movie.position, movie.looping))
}

pub(super) fn movie_render_state(actor: &Actor, render: &mut crate::SongLuaOverlayState) {
    if let Some(movie) = Movie::for_actor(actor) {
        render.movie_position_seconds = movie.position;
        render.sprite_loop = movie.looping;
        render.sprite_playback_rate = movie.rate;
    }
}

pub(super) fn movie_loop(actor: &Actor, looping: bool) {
    if let Some(mut movie) = Movie::for_actor(actor) {
        movie.looping = looping;
    }
}

pub(super) fn movie_rate(actor: &Actor, rate: f32) {
    if let Some(mut movie) = Movie::for_actor(actor) {
        movie.rate = rate;
    }
}

fn resource(call: &mut HostCall<'_>, actor: TableId) -> Result<Option<String>, Error> {
    if sprites::default_texture(call, actor)? {
        return Ok(Some("default".to_owned()));
    }
    if let Some(name) = assets::field(call, actor, b"__songlua_aft_capture_name")? {
        return Ok(Some(format!("aft:{name}")));
    }
    if let Some(path) = assets::texture_path(call, actor)? {
        return Ok(Some(format!(
            "file:{}",
            textures::collapse(&crate::file_path_string(&path))
        )));
    }
    // RageTextureManager still binds a bitmap texture when loading a missing
    // file; RageBitmapTexture substitutes its dummy image. File discovery must
    // not turn that authored texture into an untextured Sprite.
    Ok(assets::field(call, actor, b"Texture")?
        .filter(|texture| !texture.trim().is_empty())
        .map(|texture| format!("file:{}", textures::collapse(texture.trim()))))
}

fn texture_size(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<([f32; 2], [f32; 2], bool), Error> {
    if let Some(name) = assets::field(call, actor, b"__songlua_aft_capture_name")? {
        let owners = {
            let store = call.store();
            store
                .actor_order
                .iter()
                .copied()
                .filter(|id| store.actors[id].state.class == "ActorFrameTexture")
                .collect::<Vec<_>>()
        };
        let mut viewport = None;
        for owner in owners {
            let texture = framebuffers::read(call, owner)?;
            if texture.capture_name == name && texture.capture_size.is_some() {
                viewport = texture.capture_size;
                break;
            }
        }
        // SetTexture copies the retained handle's source dimensions into size.
        // They also cover a handle whose original AFT scene has been released.
        let image = viewport.unwrap_or(call.store().actors[&actor].state.size);
        let allocation =
            image.map(|n| (n as u32).checked_next_power_of_two().unwrap_or(u32::MAX) as f32);
        return Ok((
            std::array::from_fn(|i| image[i] / allocation[i]),
            allocation,
            true,
        ));
    }
    let Some(image) = sprites::actor_upload_size(call, actor)? else {
        return Ok(([1.0; 2], [1.0; 2], false));
    };
    let allocation = image.map(|n| {
        (n as u32)
            .checked_next_power_of_two()
            .unwrap_or(u32::MAX)
            .max(8) as f32
    });
    Ok((
        std::array::from_fn(|i| image[i] / allocation[i]),
        allocation,
        false,
    ))
}

fn default_states(sheet: (u32, u32)) -> Vec<crate::SongLuaSpriteState> {
    (0..crate::sprite_frame_count(Some(sheet)))
        .map(|frame| crate::SongLuaSpriteState { frame, delay: 0.1 })
        .collect()
}

pub(super) fn initialize(
    call: &mut HostCall<'_>,
    actor: TableId,
    template: bool,
) -> Result<(), Error> {
    let movie = sprites::is_video(call, actor)?;
    if !call
        .store()
        .clock
        .live
        .load(std::sync::atomic::Ordering::Relaxed)
        && !movie
    {
        return Ok(());
    }
    if template && let Some(state) = &call.store().actors[&actor].sprite_animation {
        if state.is_movie() {
            call.store().movie_sprites.insert(actor);
        }
        return Ok(());
    }
    let sheet = sprites::sheet(call, actor)?.unwrap_or((1, 1));
    let resource = resource(call, actor)?;
    let states = if resource.is_some() {
        let states = sprites::states(call, actor)?;
        if states.is_empty() {
            default_states(sheet)
        } else {
            states
        }
    } else {
        // LoadFromNode leaves an untextured Sprite's state list empty, so its
        // first SetTexture can create frames using that texture's allocation.
        Vec::new()
    };
    let (image_to_texture, texture_size, render_uses_backing_coords) = texture_size(call, actor)?;
    let movie = bind_movie(call.store(), actor, resource.as_deref().filter(|_| movie));
    let mut state = State {
        states: Arc::from([]),
        rects: Arc::from([]),
        index: 0,
        seconds: 0.0,
        custom: None,
        image_to_texture,
        texture_size,
        render_uses_backing_coords,
        resource,
        movie,
        skip_movie_update: true,
        use_effect_clock_for_texcoords: false,
    };
    state.set_states(states, sheet);
    call.store()
        .actors
        .get_mut(&actor)
        .unwrap()
        .sprite_animation = Some(state);
    Ok(())
}

pub(super) fn load(call: &mut HostCall<'_>, actor: TableId, preserve: bool) -> Result<(), Error> {
    if call.store().actors[&actor].sprite_animation.is_none() {
        if call.store().clock.live.load(std::sync::atomic::Ordering::Relaxed) {
            return initialize(call, actor, false);
        }
        return Ok(());
    }
    let resource = resource(call, actor)?;
    let movie = sprites::is_video(call, actor)?;
    let (image_to_texture, texture_size, render_uses_backing_coords) = texture_size(call, actor)?;
    let sheet = sprites::sheet(call, actor)?.unwrap_or((1, 1));
    let store = call.store();
    let movie = bind_movie(store, actor, resource.as_deref().filter(|_| movie));
    let instance = store.actors.get_mut(&actor).unwrap();
    let state = instance.sprite_animation.as_mut().unwrap();
    // Unloading an existing, different texture calls SetState(0). Reloading
    // the same texture preserves the current frame's accumulated time.
    if state.resource.is_some() && state.resource != resource {
        state.set_state(0);
        instance.state.render.movie_position_seconds = 0.0;
    }
    state.resource = resource;
    state.movie = movie;
    state.image_to_texture = image_to_texture;
    state.texture_size = texture_size;
    state.render_uses_backing_coords = render_uses_backing_coords;
    if !preserve || state.states.is_empty() {
        state.set_states(default_states(sheet), sheet);
    }
    instance.state.render.sprite_texture = state.resource.is_some();
    Ok(())
}

pub(super) fn set_states(
    call: &mut HostCall<'_>,
    actor: TableId,
    states: Vec<crate::SongLuaSpriteState>,
) -> Result<(), Error> {
    if states.is_empty() || call.store().actors[&actor].sprite_animation.is_none() {
        return Ok(());
    }
    let sheet = sprites::sheet(call, actor)?.unwrap_or((1, 1));
    let instance = call.store().actors.get_mut(&actor).unwrap();
    let state = instance.sprite_animation.as_mut().unwrap();
    state.set_states(states, sheet);
    state.set_state(0);
    instance.state.render.sprite_state_index = Some(0);
    let key = call.intern_static(b"__songlua_state_sprite_state_index")?;
    call.raw_table_set(actor, key, Value::Number(0.0))?;
    Ok(())
}

pub(super) fn render(actor: &Actor, render: &mut crate::SongLuaOverlayState) {
    movie_render_state(actor, render);
    let Some(state) = &actor.sprite_animation else {
        return;
    };
    if state.resource.is_none() {
        return;
    }
    // Keep the native active rect in backing-texture units until this boundary.
    // Image sprites render in image units, while AFT sprites sample their POT
    // render target directly. Neither may advance this snapshot again.
    let rect = state.active_rect();
    render.custom_texture_rect = Some(if state.render_uses_backing_coords {
        rect
    } else {
        std::array::from_fn(|i| rect[i] / state.image_to_texture[i % 2])
    });
    render.sprite_state_index = Some(state.index as u32);
    render.sprite_animate = false;
    render.texcoord_velocity = None;
}

fn update_animation_state(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    loop {
        let finished = {
            let Some(state) = call
                .store()
                .actors
                .get_mut(&actor)
                .and_then(|actor| actor.sprite_animation.as_mut())
            else {
                return Ok(());
            };
            if state.states.len() <= 1 {
                return Ok(());
            }
            let delay = state.states[state.index].delay;
            if !(delay > 0.0001 && state.seconds + 0.0001 > delay) {
                return Ok(());
            }
            state.seconds -= delay;
            state.index = (state.index + 1) % state.states.len();
            state.index == 0
        };
        if finished {
            // The callback can change the state list, current frame or timer.
            run_named(call, actor, b"AnimationFinished", None)?;
        }
    }
}

pub(super) fn set_seconds(
    call: &mut HostCall<'_>,
    actor: TableId,
    seconds: f32,
) -> Result<bool, Error> {
    let Some(instance) = call.store().actors.get_mut(&actor) else {
        return Ok(false);
    };
    let Some(state) = instance.sprite_animation.as_mut() else {
        return Ok(false);
    };
    state.set_state(0);
    state.seconds = seconds;
    let movie = state.is_movie();
    if movie {
        seek_movie(call, actor, seconds)?;
    }
    update_animation_state(call, actor)?;
    Ok(true)
}

pub(super) fn seek_movie(
    call: &mut HostCall<'_>,
    actor: TableId,
    seconds: f32,
) -> Result<(), Error> {
    // MovieTexture_Generic::SetPosition only supports rewind. Sprite still
    // applies nonzero SetSecondsIntoAnimation values to its sheet animation,
    // but the native decoder ignores the corresponding movie seek.
    if seconds != 0.0 || !sprites::is_video(call, actor)? {
        return Ok(());
    }
    action_capture::prepare_write(call, actor)?;
    sprites::set(
        call,
        actor,
        "__songlua_state_movie_position_seconds",
        Value::Number(seconds as f64),
    )?;
    let instance = call.store().actors.get_mut(&actor).unwrap();
    instance.state.render.movie_position_seconds = seconds;
    if let Some(mut movie) = Movie::for_actor(instance) {
        movie.position = seconds;
    }
    capture::write(
        call,
        actor,
        crate::SongLuaOverlayStateDelta {
            movie_position_seconds: Some(seconds),
            ..Default::default()
        },
        true,
    );
    Ok(())
}

pub(super) fn enable_animation(call: &mut HostCall<'_>, actor: TableId, enabled: bool) {
    let instance = call.store().actors.get_mut(&actor).unwrap();
    if enabled
        && !instance.state.render.sprite_animate
        && let Some(state) = &mut instance.sprite_animation
    {
        state.skip_movie_update = true;
    }
}

pub(super) fn advance(call: &mut HostCall<'_>, actor: TableId, delta: f32) -> Result<(), Error> {
    let Some(instance) = call.store().actors.get_mut(&actor) else {
        return Ok(());
    };
    let effect_delta = effects::delta(instance);
    if let Some(state) = &mut instance.sprite_animation {
        let skip = std::mem::replace(&mut state.skip_movie_update, false);
        if !skip
            && instance.state.render.sprite_animate
            && instance.state.render.sprite_texture
            && instance.state.render.decode_movie
            && let Some(movie) = &state.movie
        {
            let mut movie = movie.lock().unwrap();
            movie.position += effect_delta.max(0.0) * movie.rate;
        }
    }
    if !instance.state.render.sprite_animate || !instance.state.render.sprite_texture {
        return Ok(());
    }
    let Some(state) = &mut instance.sprite_animation else {
        return Ok(());
    };
    state.seconds += effect_delta;
    if state.seconds < 0.0 {
        let length = state.length();
        if length > 0.0 {
            state.seconds = state.seconds.rem_euclid(length);
        }
    }
    update_animation_state(call, actor)?;
    let instance = call.store().actors.get_mut(&actor).unwrap();
    let Some(velocity) = instance
        .state
        .render
        .texcoord_velocity
        .filter(|v| *v != [0.0; 2])
    else {
        return Ok(());
    };
    let state = instance.sprite_animation.as_mut().unwrap();
    let delta = if state.use_effect_clock_for_texcoords {
        effect_delta
    } else {
        delta
    };
    let mut rect = state.active_rect();
    for i in 0..4 {
        rect[i] += delta * velocity[i % 2];
    }
    if instance.state.render.texture_wrapping {
        let offset = [rect[0].floor(), rect[1].floor()];
        for i in 0..4 {
            rect[i] -= offset[i % 2];
        }
    }
    state.custom = Some(rect);
    instance.state.render.texture_wrapping = true;
    // Sprite::SetCustomTextureCoords enables wrapping after the optional wrap.
    host_compat::raw_flag(call, actor, "__songlua_state_texture_wrapping", true)?;
    Ok(())
}
