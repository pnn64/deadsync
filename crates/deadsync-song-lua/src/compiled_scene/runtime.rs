//! The executable result of the ECS lowering. No Lua values, callbacks or VM.
//! State is owned by one gameplay screen; capacity is fixed by the loaded level.
use crate::{
    SongLuaInputEvent, SongLuaInputType, SongLuaLiveOutput, SongLuaLiveRequest, SongLuaLiveTarget,
    SongLuaOverlayActor, SongLuaOverlayKind, SongLuaOverlayMeshVertex, SongLuaOverlayState,
    SongLuaSoundEvent,
};
use std::{path::PathBuf, sync::Arc};

type V3 = [f64; 3];

#[derive(Debug, Clone)]
pub struct CompiledSceneRestart {
    pub joined: [bool; 2],
    pub steps: [Option<usize>; 2],
    pub remaining: i32,
    pub(crate) random: Box<crate::compat::SongLuaRandom>,
}
#[derive(Debug)]
pub enum CompiledMusicAction {
    Start(PathBuf),
    Stop,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Entity {
    pub position: V3,
    pub rotation: Option<[f64; 2]>,
    pub spin: Option<[f64; 2]>,
    pub physics: Option<Physics>,
    pub ball: Option<Ball>,
    pub half_box: Option<V3>,
    pub box_slippy: bool,
    pub moves: Vec<(V3, f64)>,
    pub move_index: usize,
    pub move_progress: f64,
    pub input: Option<usize>,
    pub follow: Option<(usize, V3)>,
    pub camera: Option<usize>,
    pub checkpoint: Option<V3>,
    pub goal: Option<V3>,
    pub mesh: Option<Mesh>,
    pub color: Option<[f32; 4]>,
    pub translate: Option<[f64; 2]>,
}
#[derive(Debug, Clone, Copy)]
pub(super) struct Physics {
    pub velocity: V3,
    pub gravity: bool,
    pub collision: bool,
}
#[derive(Debug, Clone, Copy)]
pub(super) struct Ball {
    pub radius: f64,
    pub bounce_height: f64,
    pub spawn: V3,
    pub slippy: bool,
}
#[derive(Debug, Clone)]
pub(super) struct Mesh {
    pub points: Vec<V3>,
    pub corners: Vec<(usize, [f32; 2])>,
}
#[derive(Debug, Clone, Copy)]
pub(super) struct Binding {
    pub player: usize,
    pub entity: usize,
    pub actor: usize,
}

#[derive(Debug, Default)]
pub struct CompiledSongScene {
    pub(super) entities: Vec<Entity>,
    pub(super) bindings: Vec<Binding>,
    pub(super) states: Vec<SongLuaOverlayState>,
    pub(super) paths: Vec<String>,
    pub(super) targets: Vec<(usize, SongLuaLiveTarget)>,
    pub(super) meshes: Vec<Option<Arc<[SongLuaOverlayMeshVertex]>>>,
    pub(super) texts: Vec<Option<Arc<str>>>,
    pub(super) timers: Vec<usize>,
    pub(super) foregrounds: Vec<usize>,
    pub(super) screen: [f64; 2],
    pub(super) joined: [bool; 2],
    pub(super) sounds: [PathBuf; 6],
    pub(super) random: crate::compat::SongLuaRandom,
    pub(super) steps_by_difficulty: Vec<(crate::SongLuaDifficulty, usize)>,
    pub(super) step_chart_indices: Vec<Option<usize>>,
    pub(super) selected_charts: [Option<usize>; 2],
    pub(super) challenge_chart: Option<usize>,
    pub(super) enabled: [bool; 2],
    pub(super) start_second: f32,
    pub(super) music_rate: f32,
    pub(super) active: bool,
    pub(super) last_music: f32,
    pub(super) update_left: f32,
    pub(super) timer_left: f32,
    pub(super) frame: u64,
    pub(super) remaining: i32,
    pub(super) running: bool,
    pub(super) winner: Option<usize>,
    pub(super) finish_left: Option<f32>,
    pub(super) restart: Option<CompiledSceneRestart>,
    pub(super) held: [[bool; 4]; 2],
    pub(super) points: Vec<V3>,
    pub(super) join_mode: bool,
    pub(super) join_players: [Option<usize>; 2],
    pub(super) timer_only: bool,
    pub(super) bruh_mode: bool,
    pub(super) initial_remaining: i32,
}

impl CompiledSongScene {
    pub(super) fn initialize(&mut self) {
        self.active = false;
        self.last_music = 0.0;
        self.update_left = 0.0;
        self.timer_left = if self.bruh_mode { 8.0 } else { 0.0 };
        self.frame = 0;
        self.remaining = if self.join_mode {
            8
        } else {
            self.initial_remaining
        };
        self.running = true;
        self.winner = None;
        self.finish_left = None;
        self.restart = None;
        self.held = [[false; 4]; 2];
        self.points.resize(
            self.entities
                .iter()
                .filter_map(|e| e.mesh.as_ref().map(|m| m.points.len()))
                .max()
                .unwrap_or(0),
            [0.0; 3],
        );
    }
    pub fn bind_targets(&mut self, targets: Vec<(usize, SongLuaLiveTarget)>) {
        self.targets = targets;
    }
    pub fn visual_bindings(&self) -> Vec<(String, SongLuaLiveTarget)> {
        self.targets
            .iter()
            .map(|&(i, t)| (self.paths[i].clone(), t))
            .collect()
    }
    pub fn input(&mut self, input: &SongLuaInputEvent<'_>) {
        if !self.active {
            return;
        }
        let player = input.player.unwrap_or(0).min(1);
        if self.join_mode {
            if input.button == "Up" && input.kind != SongLuaInputType::Release {
                self.joined[player] = true;
                if let Some(i) = self.join_players[player] {
                    self.texts[i] = Some("Joined!".into());
                }
                if self.joined.iter().all(|v| *v) {
                    self.join_restart();
                }
            }
            return;
        }
        let button = match input.button {
            "Left" => 0,
            "Right" => 1,
            "Down" => 2,
            "Up" => 3,
            _ => return,
        };
        self.held[player][button] = input.kind != SongLuaInputType::Release;
    }
    pub fn take_restart(&mut self) -> Option<CompiledSceneRestart> {
        self.restart.take()
    }
    pub fn restart_pending(&self) -> bool {
        self.restart.is_some()
    }
    /// GAMESTATE selection can differ from the note data loaded by the screen.
    pub fn selected_chart_indices(&self) -> [Option<usize>; 2] {
        self.selected_charts
    }
    fn join_restart(&mut self) {
        if !self.joined.iter().any(|v| *v) {
            self.joined = [true; 2];
        }
        let step = self
            .steps_by_difficulty
            .iter()
            .find(|(d, _)| *d == crate::SongLuaDifficulty::Beginner)
            .map(|(_, i)| *i);
        self.restart = Some(CompiledSceneRestart {
            joined: self.joined,
            steps: self.enabled.map(|v| v.then_some(step).flatten()),
            remaining: 120,
            random: Box::new(self.random.clone()),
        });
        self.select_restart_charts();
    }
    fn select_restart_charts(&mut self) {
        if let Some(restart) = &self.restart {
            for p in 0..2 {
                if self.enabled[p] {
                    self.selected_charts[p] = restart.steps[p]
                        .and_then(|i| self.step_chart_indices.get(i).copied().flatten());
                }
            }
        }
    }
    fn sound(&self, sound: usize, output: &mut SongLuaLiveOutput) {
        output.sounds.push(SongLuaSoundEvent {
            beat: 0.0,
            path: self.sounds[sound].clone(),
            source_message: None,
            queued_startup: false,
        });
    }
    pub fn advance(&mut self, beat: f64, music: f32) -> SongLuaLiveOutput {
        let mut output = SongLuaLiveOutput::default();
        if beat < 0.0 && !self.active {
            for &i in &self.foregrounds {
                self.states[i].visible = false;
            }
            return output;
        }
        let delta = if !self.active {
            self.active = true;
            // The verified wrapper's OnCommand restores the selected
            // difficulty to Challenge without replacing loaded note data.
            for p in 0..2 {
                if self.enabled[p] && self.challenge_chart.is_some() {
                    self.selected_charts[p] = self.challenge_chart;
                }
            }
            for &i in &self.foregrounds {
                self.states[i].visible = true;
            }
            if !self.timer_only {
                output.compiled_music = Some(CompiledMusicAction::Start(self.sounds[0].clone()));
            }
            (music - self.start_second).max(0.0) / self.music_rate
        } else {
            (music - self.last_music).max(0.0) / self.music_rate
        };
        self.last_music = music;
        if self.bruh_mode {
            let mut dt = delta;
            if self.running && consume(&mut self.timer_left, &mut dt) {
                self.running = false;
                output.request = Some(SongLuaLiveRequest::Transition("SM_DoNextScreen".into()));
            }
            return output;
        }
        if self.join_mode {
            let mut dt = delta;
            while consume(&mut self.timer_left, &mut dt) {
                if self.remaining < 0 {
                    self.join_restart();
                    break;
                }
                let text: Arc<str> = self.remaining.to_string().into();
                for &i in &self.timers {
                    self.texts[i] = Some(text.clone());
                }
                self.remaining -= 1;
                self.timer_left = 1.0;
            }
            if self.restart.is_some() {
                output.request = Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into()));
            }
            return output;
        }
        // Actor::UpdateTweening uses f32 subtraction and requires positive
        // leftover delta to execute a queued zero-time command.
        let mut delta_update = delta;
        while self.finish_left.is_none() && consume(&mut self.update_left, &mut delta_update) {
            if self.running {
                self.tick(&mut output);
                self.update_left = (1.0_f64 / 60.0) as f32;
            } else {
                if self.finish_left.is_none() {
                    self.finish_left = Some(3.0);
                }
                break;
            }
        }
        let mut delta_timer = delta;
        while consume(&mut self.timer_left, &mut delta_timer) && self.remaining > 0 {
            self.remaining -= 1;
            let text: Arc<str> =
                format!("{}:{:02}", self.remaining / 60, self.remaining % 60).into();
            for &i in &self.timers {
                self.texts[i] = Some(text.clone());
            }
            self.timer_left = 1.0;
            if self.remaining == 0 {
                output.request = Some(SongLuaLiveRequest::Transition("SM_DoNextScreen".into()));
            }
        }
        if let Some(left) = &mut self.finish_left {
            if consume(left, &mut delta_update) && self.restart.is_none() {
                let mut steps = [None; 2];
                for p in 0..2 {
                    if self.enabled[p] {
                        let difficulty = if self.winner == Some(p) {
                            if self.joined.iter().all(|v| *v) {
                                crate::SongLuaDifficulty::Easy
                            } else {
                                crate::SongLuaDifficulty::Hard
                            }
                        } else {
                            crate::SongLuaDifficulty::Medium
                        };
                        steps[p] = self
                            .steps_by_difficulty
                            .iter()
                            .find(|(d, _)| *d == difficulty)
                            .map(|(_, i)| *i);
                    }
                }
                self.restart = Some(CompiledSceneRestart {
                    joined: self.joined,
                    steps,
                    remaining: self.remaining,
                    random: Box::new(self.random.clone()),
                });
                self.select_restart_charts();
                output.request = Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into()));
            }
        }
        output
    }
    fn tick(&mut self, output: &mut SongLuaLiveOutput) {
        // Preserve authored system and entity order; computations stay double
        // until values cross the native actor setter boundary.
        for e in &mut self.entities {
            if let (Some(r), Some(s)) = (&mut e.rotation, e.spin) {
                r[0] += s[0];
                r[1] += s[1];
            }
            if !e.moves.is_empty() {
                e.move_progress += 1.0;
                if e.move_progress >= e.moves[e.move_index].1 {
                    e.move_progress = 0.0;
                    e.move_index = (e.move_index + 1) % e.moves.len();
                }
                e.physics.as_mut().unwrap().velocity = e.moves[e.move_index].0;
            }
        }
        if self.frame + 1 >= 60 {
            for e in &mut self.entities {
                let Some(player) = e.input else { continue };
                let h = self.held[player];
                let dx = f64::from(h[1] as u8) - f64::from(h[0] as u8);
                let dz = f64::from(h[3] as u8) - f64::from(h[2] as u8);
                let magnitude = (dx * dx + dz * dz).sqrt();
                let (dx, dz) = if magnitude == 0.0 {
                    (dx, dz)
                } else {
                    (dx / magnitude, dz / magnitude)
                };
                let accel = if e.ball.unwrap().slippy { 0.003 } else { 0.01 };
                let v = &mut e.physics.as_mut().unwrap().velocity;
                for (axis, d) in [(0, dx), (2, dz)] {
                    let target = 0.3 * d;
                    if target == 0.0
                        || (target > 0.0) != (v[axis] > 0.0)
                        || target.abs() > v[axis].abs()
                    {
                        v[axis] = if v[axis] < target {
                            (v[axis] + accel).min(target)
                        } else {
                            (v[axis] - accel).max(target)
                        };
                    }
                }
            }
        }
        for e in &mut self.entities {
            if let Some(p) = &mut e.physics {
                p.collision = false;
            }
        }
        for _ in 0..2 {
            for e in &mut self.entities {
                if let Some(p) = &mut e.physics {
                    if p.gravity {
                        p.velocity[1] = (p.velocity[1] - 0.5 * 0.01).max(-0.25);
                    }
                    for a in 0..3 {
                        e.position[a] += 0.5 * p.velocity[a];
                    }
                }
            }
            for box_index in 0..self.entities.len() {
                let entity = &self.entities[box_index];
                let Some(half) = entity.half_box else {
                    continue;
                };
                let position = entity.position;
                let rotation = entity.rotation;
                let velocity = entity.physics.map_or([0.0; 3], |p| p.velocity);
                let slippy = entity.box_slippy;
                for ball in &mut self.entities {
                    let Some(mut data) = ball.ball else { continue };
                    let mut closest = std::array::from_fn(|a| ball.position[a] - position[a]);
                    rotate(&mut closest, rotation, true);
                    for a in 0..3 {
                        closest[a] = closest[a].min(half[a]).max(-half[a]);
                    }
                    rotate(&mut closest, rotation, false);
                    let mut direction: V3 =
                        std::array::from_fn(|a| closest[a] + position[a] - ball.position[a]);
                    let distance = (direction[0] * direction[0]
                        + direction[1] * direction[1]
                        + direction[2] * direction[2])
                        .sqrt();
                    if distance > 0.0 && distance <= data.radius {
                        for a in 0..3 {
                            direction[a] /= distance;
                            ball.position[a] -= (data.radius - distance) * direction[a];
                        }
                        let physics = ball.physics.as_mut().unwrap();
                        let relative: V3 =
                            std::array::from_fn(|a| physics.velocity[a] - velocity[a]);
                        let mut t = relative[0] * direction[0]
                            + relative[1] * direction[1]
                            + relative[2] * direction[2];
                        if t > 0.0 {
                            physics.collision = true;
                            t = (1.0 + 0.5) * t + 0.1;
                            for a in 0..3 {
                                direction[a] = t * direction[a];
                                physics.velocity[a] = relative[a] + velocity[a] - direction[a];
                            }
                            if direction[1] < 0.0 {
                                data.bounce_height = ball.position[1];
                            }
                            data.slippy = slippy;
                            ball.ball = Some(data);
                        }
                    }
                }
            }
        }
        for e in &mut self.entities {
            if let Some(ball) = &mut e.ball {
                if e.position[1] < -50.0 {
                    e.position = ball.spawn;
                    ball.bounce_height = e.position[1];
                    e.physics.as_mut().unwrap().velocity = [0.0; 3];
                }
            }
        }
        for i in 0..self.entities.len() {
            if let Some((target, offset)) = self.entities[i].follow {
                let target = &self.entities[target];
                let position = target.position;
                let y = target.ball.unwrap().bounce_height.min(position[1]);
                for (a, value) in [position[0], y, position[2]].into_iter().enumerate() {
                    self.entities[i].position[a] =
                        0.9 * self.entities[i].position[a] + 0.1 * (value + offset[a]);
                }
            }
        }
        for i in 0..self.entities.len() {
            let e = &self.entities[i];
            let position = e.position;
            if let Some(half) = e.checkpoint {
                let mut checkpoint = 0;
                for ball in &mut self.entities {
                    if let Some(b) = &mut ball.ball {
                        if inside(ball.position, position, half) && b.spawn != position {
                            b.spawn = position;
                            checkpoint += 1;
                        }
                    }
                }
                for _ in 0..checkpoint {
                    self.sound(1, output);
                }
            }
        }
        for goal in &self.entities {
            if let Some(half) = goal.goal {
                for ball in &self.entities {
                    if ball.ball.is_some() && inside(ball.position, goal.position, half) {
                        self.winner = ball.input;
                        self.running = false;
                        output.compiled_music = Some(CompiledMusicAction::Stop);
                        self.sound(2, output);
                    }
                }
            }
        }
        if self
            .entities
            .iter()
            .any(|e| e.physics.is_some_and(|p| p.collision))
        {
            let sound = 3 + (3.0 * self.random.real()).floor() as usize;
            self.sound(sound.min(5), output);
        }
        self.render();
        self.frame += 1;
    }
    fn render(&mut self) {
        let multiplayer = self.joined.iter().all(|v| *v);
        for binding in &self.bindings {
            let camera = self
                .entities
                .iter()
                .find(|e| e.camera == Some(binding.player))
                .unwrap();
            let e = &self.entities[binding.entity];
            let offset = if multiplayer {
                if binding.player == 0 {
                    -0.27 * self.screen[1]
                } else {
                    0.25 * self.screen[1]
                }
            } else {
                0.0
            };
            let low = if multiplayer && binding.player == 1 {
                0.0
            } else {
                -self.screen[1]
            };
            let high = if multiplayer && binding.player == 0 {
                10.0
            } else {
                self.screen[1]
            };
            let state = &mut self.states[binding.actor];
            if let Some(mesh) = &e.mesh {
                for (dest, source) in self.points.iter_mut().zip(&mesh.points) {
                    *dest = *source;
                    rotate(dest, e.rotation, false);
                    for a in 0..3 {
                        dest[a] += e.position[a] - camera.position[a];
                    }
                    rotate(dest, camera.rotation, true);
                    let z = dest[2];
                    if z <= 0.0 {
                        *dest = [f64::NAN; 3];
                    } else {
                        dest[0] = dest[0] * 1000.0 / z;
                        dest[1] = (-dest[1] * 1000.0 / z + offset).min(high).max(low);
                        dest[2] = -z;
                    }
                }
                let vertices = Arc::make_mut(self.meshes[binding.actor].as_mut().unwrap());
                for (v, &(point, uv)) in vertices.iter_mut().zip(&mesh.corners) {
                    let p = self.points[point];
                    v.pos = [p[0] as f32, p[1] as f32];
                    v.z = p[2] as f32;
                    v.uv = uv;
                }
                state.visible = true;
                if let Some(color) = e.color {
                    state.diffuse = color;
                }
                if let Some(uv) = e.translate {
                    state.texcoord_offset = Some([
                        (uv[0] * self.frame as f64) as f32,
                        (uv[1] * self.frame as f64) as f32,
                    ]);
                }
            } else if let Some(ball) = e.ball {
                let mut p = std::array::from_fn(|a| e.position[a] - camera.position[a]);
                rotate(&mut p, camera.rotation, true);
                let z = p[2];
                if z <= 0.0 {
                    state.visible = false;
                } else {
                    let x = p[0] * 1000.0 / z;
                    let y = -p[1] * 1000.0 / z + offset;
                    state.x = x as f32;
                    state.y = y as f32;
                    state.z = (-z) as f32;
                    // zoomto{width,height} divides by the loaded sprite's
                    // intrinsic dimensions. Ball artwork is not assumed 32px.
                    let size = ball.radius * 2.0 * 1000.0 / z;
                    let intrinsic = state.size.unwrap_or([32.0, 32.0]);
                    state.zoom_x = (size as f32) / intrinsic[0];
                    state.zoom_y = (size as f32) / intrinsic[1];
                    state.visible = low <= y && y <= high;
                }
            }
        }
    }
    pub fn write_visuals<N, M>(
        &self,
        visuals: &mut deadsync_gameplay::SongLuaRuntimeVisuals<
            SongLuaOverlayActor<SongLuaOverlayKind<N, M, deadlib_present::actors::TextAttribute>>,
            crate::SongLuaCapturedActor,
            crate::gameplay::SongLuaRuntimeOverlayStateDelta,
        >,
    ) -> Result<(), String> {
        for &(global, target) in &self.targets {
            let actor = match target {
                SongLuaLiveTarget::Root(i) => visuals.overlays.get_mut(i),
                SongLuaLiveTarget::Background(l, i) => visuals
                    .background_visual_layers
                    .get_mut(l)
                    .and_then(|l| l.overlays.get_mut(i)),
                SongLuaLiveTarget::Foreground(l, i) => visuals
                    .foreground_visual_layers
                    .get_mut(l)
                    .and_then(|l| l.overlays.get_mut(i)),
            }
            .ok_or("compiled scene binding is outside its layer")?;
            actor.initial_state = self.states[global];
            if let (SongLuaOverlayKind::ActorMultiVertex { vertices, .. }, Some(mesh)) =
                (&mut actor.kind, &self.meshes[global])
            {
                *vertices = mesh.clone();
            }
            if let (SongLuaOverlayKind::BitmapText { text, .. }, Some(value)) =
                (&mut actor.kind, &self.texts[global])
            {
                *text = value.clone();
            }
        }
        Ok(())
    }
}

fn consume(left: &mut f32, delta: &mut f32) -> bool {
    if *delta <= 0.0 {
        return false;
    }
    if *left > 0.0 {
        let used = left.min(*delta);
        *left -= used;
        *delta -= used;
    }
    *delta > 0.0
}
fn inside(p: V3, c: V3, h: V3) -> bool {
    (p[0] - c[0]).abs() < h[0] && (p[1] - c[1]).abs() < h[1] && (p[2] - c[2]).abs() < h[2]
}
fn rotate(v: &mut V3, rotation: Option<[f64; 2]>, inverse: bool) {
    let Some([pitch, yaw]) = rotation else { return };
    let sp = pitch.sin();
    let cp = pitch.cos();
    let sy = yaw.sin();
    let cy = yaw.cos();
    let [x, y, z] = *v;
    *v = if inverse {
        [
            cy * x - sy * z,
            sp * sy * x + cp * y + sp * cy * z,
            cp * sy * x - sp * y + cp * cy * z,
        ]
    } else {
        [
            cy * x + sp * sy * y + cp * sy * z,
            cp * y - sp * z,
            -sy * x + sp * cy * y + cp * cy * z,
        ]
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_countdown_matches_native_full_song_clock() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/interactive_song_lua/fof-timeout-clock.json.zst"
        ));
        let data = zstd::stream::decode_all(bytes.as_slice()).unwrap();
        let native: serde_json::Value = serde_json::from_slice(&data).unwrap();
        let mut scene = CompiledSongScene {
            timer_only: true,
            initial_remaining: 120,
            music_rate: 1.0,
            start_second: native["clock_inputs"]["zero_beat_music_seconds"]
                .as_f64()
                .unwrap() as f32,
            timers: vec![0],
            texts: vec![None],
            ..Default::default()
        };
        scene.initialize();
        let mut requested = None;
        for frame in native["frames"].as_array().unwrap() {
            let seconds = frame[1].as_f64().unwrap() as f32;
            let output = scene.advance(frame[0].as_f64().unwrap(), seconds);
            if matches!(output.request,Some(SongLuaLiveRequest::Transition(ref name)) if name=="SM_DoNextScreen")
            {
                requested = Some(seconds);
                break;
            }
        }
        assert_eq!(
            requested,
            Some(native["expected_request"]["seconds"].as_f64().unwrap() as f32)
        );
        assert_eq!(scene.texts[0].as_deref(), Some("0:00"));
    }

    #[test]
    fn compiled_goal_queues_one_delayed_reload_with_source_chart_choices() {
        let mut scene = CompiledSongScene {
            initial_remaining: 120,
            music_rate: 1.0,
            joined: [true, false],
            enabled: [true, false],
            steps_by_difficulty: vec![
                (crate::SongLuaDifficulty::Hard, 4),
                (crate::SongLuaDifficulty::Medium, 3),
            ],
            ..Default::default()
        };
        scene.initialize();
        scene.running = false;
        scene.winner = Some(0);
        assert!(scene.advance(1.0, 0.1).request.is_none());
        assert!(scene.advance(2.0, 2.99).request.is_none());
        assert!(matches!(
            scene.advance(3.0, 3.2).request,
            Some(SongLuaLiveRequest::NewScreen(_))
        ));
        assert_eq!(scene.restart.as_ref().unwrap().steps, [Some(4), None]);
        assert!(scene.advance(4.0, 3.3).request.is_none());
    }

    #[test]
    fn compiled_result_screen_requests_its_transition_once_after_eight_seconds() {
        let mut scene = CompiledSongScene {
            bruh_mode: true,
            music_rate: 1.0,
            ..Default::default()
        };
        scene.initialize();
        assert!(scene.advance(0.0, 0.0).request.is_none());
        assert!(scene.advance(1.0, 7.99).request.is_none());
        assert!(matches!(
            scene.advance(2.0, 8.01).request,
            Some(SongLuaLiveRequest::Transition(_))
        ));
        assert!(scene.advance(3.0, 8.02).request.is_none());
    }
}
