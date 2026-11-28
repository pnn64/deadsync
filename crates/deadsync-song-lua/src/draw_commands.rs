//! Immutable custom draw payloads and source resolution, independent of Lua.
use crate::song_column_output::arc_bytes;
use crate::{SongLuaOverlayMeshVertex, SongLuaOverlayState};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::{fmt, hash::Hash, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bincode::Encode, bincode::Decode)]
pub enum DrawSource {
    Overlay(usize),
    Player(usize),
    Judgment(usize),
    Combo(usize),
    SongForeground,
    ScreenLayer(usize),
}

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub enum DrawOp {
    Begin {
        capture: usize,
        preserve: bool,
    },
    Finish {
        capture: usize,
    },
    Draw {
        source: DrawSource,
        state: SongLuaOverlayState,
        /// Caller draw stack, outermost first; BeginRenderingTo resets it.
        parents: Arc<[SongLuaOverlayState]>,
        /// Final colors supplied by normal ActorFrame traversal or direct wrappers.
        /// Unlike the transform stack, explicit Draw callbacks reset this scope.
        inherited_diffuse: [f32; 4],
        inherited_glow: [f32; 4],
        camera: Option<SongLuaOverlayState>,
        vertices: Option<Arc<[SongLuaOverlayMeshVertex]>>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Command<S> {
    Begin { capture: usize, preserve: bool },
    Finish { capture: usize },
    Draw { source: S, pose: usize },
}

#[derive(PartialEq)]
pub(crate) struct Pose {
    pub(crate) state: SongLuaOverlayState,
    pub(crate) parents: Arc<[SongLuaOverlayState]>,
    pub(crate) inherited_diffuse: [f32; 4],
    pub(crate) inherited_glow: [f32; 4],
    pub(crate) camera: Option<Arc<SongLuaOverlayState>>,
    pub(crate) vertices: Option<Arc<[SongLuaOverlayMeshVertex]>>,
}

#[derive(Clone)]
struct PoseRef(Arc<Pose>);

impl PartialEq for PoseRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for PoseRef {}

impl Hash for PoseRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

/// A shared ordered program referencing declared actors, plus the state captured
/// for this invocation. Target changes remain individual program instructions.
#[derive(Clone, Default)]
pub struct DrawOps {
    batches: SmallVec<[DrawBatch; 1]>,
}

#[derive(Clone)]
struct DrawBatch {
    commands: Arc<[Command<DrawSource>]>,
    poses: Arc<[PoseRef]>,
}

impl DrawOps {
    pub fn len(&self) -> usize {
        self.batches.iter().map(|batch| batch.commands.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.batches.iter().all(|batch| batch.commands.is_empty())
    }

    pub fn get(&self, mut index: usize) -> Option<DrawOp> {
        for batch in &self.batches {
            if let Some(&command) = batch.commands.get(index) {
                return Some(batch.expand(command));
            }
            index -= batch.commands.len();
        }
        None
    }

    pub fn iter(&self) -> impl Iterator<Item = DrawOp> + '_ {
        self.batches
            .iter()
            .flat_map(|batch| batch.commands.iter().map(|&command| batch.expand(command)))
    }

    pub(crate) fn append(&mut self, ops: Self) {
        self.batches.extend(ops.batches);
    }
}

impl DrawBatch {
    fn expand(&self, command: Command<DrawSource>) -> DrawOp {
        match command {
            Command::Begin { capture, preserve } => DrawOp::Begin { capture, preserve },
            Command::Finish { capture } => DrawOp::Finish { capture },
            Command::Draw { source, pose } => {
                let pose = &self.poses[pose].0;
                DrawOp::Draw {
                    source,
                    state: pose.state,
                    parents: pose.parents.clone(),
                    inherited_diffuse: pose.inherited_diffuse,
                    inherited_glow: pose.inherited_glow,
                    camera: pose.camera.as_deref().copied(),
                    vertices: pose.vertices.clone(),
                }
            }
        }
    }
}

impl fmt::Debug for DrawOps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl PartialEq for DrawOps {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl From<Vec<DrawOp>> for DrawOps {
    fn from(ops: Vec<DrawOp>) -> Self {
        let mut commands = Vec::with_capacity(ops.len());
        let mut poses = Vec::new();
        for op in ops {
            commands.push(match op {
                DrawOp::Begin { capture, preserve } => Command::Begin { capture, preserve },
                DrawOp::Finish { capture } => Command::Finish { capture },
                DrawOp::Draw {
                    source,
                    state,
                    parents,
                    inherited_diffuse,
                    inherited_glow,
                    camera,
                    vertices,
                } => {
                    let pose = poses.len();
                    poses.push(PoseRef(Arc::new(Pose {
                        state,
                        parents,
                        inherited_diffuse,
                        inherited_glow,
                        camera: camera.map(Arc::new),
                        vertices,
                    })));
                    Command::Draw { source, pose }
                }
            });
        }
        Self {
            batches: smallvec::smallvec![DrawBatch {
                commands: commands.into(),
                poses: poses.into()
            }],
        }
    }
}

/// Layer-local overlay IDs differ from the original compiled graph. Translate
/// each shared program once, retaining the same immutable draw snapshots.
#[derive(Default)]
pub(crate) struct RemappedDrawPrograms {
    programs: FxHashMap<*const Command<DrawSource>, Arc<[Command<DrawSource>]>>,
}

impl RemappedDrawPrograms {
    pub(crate) fn remap(
        &mut self,
        ops: &mut DrawOps,
        remap: impl Fn(&mut usize) -> Result<(), String>,
    ) -> Result<(), String> {
        for batch in &mut ops.batches {
            if batch.commands.is_empty() {
                continue;
            }
            let key = batch.commands.as_ptr();
            batch.commands = match self.programs.entry(key) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.get().clone(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let mut commands = batch.commands.to_vec();
                    for command in &mut commands {
                        match command {
                            Command::Begin { capture: index, .. }
                            | Command::Finish { capture: index }
                            | Command::Draw {
                                source: DrawSource::Overlay(index),
                                ..
                            } => remap(index)?,
                            _ => {}
                        }
                    }
                    entry
                        .insert(if commands.as_slice() == batch.commands.as_ref() {
                            batch.commands.clone()
                        } else {
                            commands.into()
                        })
                        .clone()
                }
            };
        }
        Ok(())
    }
}

/// Persisted as the expanded operation list. Decoding rebuilds the shared
/// program, so poses shared between invocations decode as separate values.
impl bincode::Encode for DrawOps {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        self.iter().collect::<Vec<DrawOp>>().encode(encoder)
    }
}

impl<Context> bincode::Decode<Context> for DrawOps {
    fn decode<D: bincode::de::Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Vec::<DrawOp>::decode(decoder).map(Self::from)
    }
}

impl<'de, Context> bincode::BorrowDecode<'de, Context> for DrawOps {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Vec::<DrawOp>::borrow_decode(decoder).map(Self::from)
    }
}

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub struct DrawFrame {
    pub second: f32,
    /// A command/update submission, played once before ordinary scene drawing.
    /// It does not replace the owner's normal DrawPrimitives traversal.
    pub immediate: bool,
    pub owners: Vec<usize>,
    /// Exclusive operation end for each owner, in the same order as `owners`.
    pub owner_ends: Vec<usize>,
    pub ops: DrawOps,
}

#[derive(Clone, Default)]
pub(crate) struct Context {
    pub(crate) owner: usize,
    pub(crate) parents: Arc<[SongLuaOverlayState]>,
    pub(crate) camera: Option<SongLuaOverlayState>,
    pub(crate) color: Color,
    pub(crate) inherited_color: Color,
    /// Preparation retains transparent payloads, but native draw side effects
    /// still stop at an ancestor that would have aborted Actor::Draw.
    pub(crate) native_skipped: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct Color {
    pub(crate) diffuse: [f32; 4],
    pub(crate) glow: [f32; 4],
}

impl Default for Color {
    fn default() -> Self {
        Self {
            diffuse: [1.0; 4],
            glow: [0.0; 4],
        }
    }
}

impl Color {
    pub(crate) fn pre_draw(self, state: SongLuaOverlayState) -> (Self, bool) {
        let local_diffuse = state
            .vertex_colors
            .map_or(state.diffuse, |colors| colors[0]);
        let (diffuse, glow) = crate::playback::song_lua_inherit_draw_colors(
            local_diffuse,
            state.glow,
            state.base_alpha,
            self.diffuse,
            self.glow,
        );
        let alpha = self.diffuse[3] * state.base_alpha;
        let partially_opaque = state.vertex_colors.map_or(diffuse[3] > 0.0, |colors| {
            colors.iter().any(|color| color[3] * alpha > 0.0)
        }) || glow[3] > 0.0;
        (Self { diffuse, glow }, partially_opaque)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RawCall {
    owner: usize,
    command: Command<usize>,
}

pub(crate) struct RawFrame {
    pub(crate) second: f32,
    pub(crate) immediate: bool,
    pub(crate) owners: Vec<usize>,
    calls: Arc<[RawCall]>,
    poses: Arc<[PoseRef]>,
}

#[derive(Default)]
pub(crate) struct Pending {
    calls: Vec<RawCall>,
    poses: Vec<PoseRef>,
}

/// Song-load worker only, no synchronization or gameplay maintenance. Retains
/// immutable frames until compilation hands them to song-lifetime playback.
/// Work is counted on submission, and memory is counted when retaining a
/// program, pose, or frame. Replaying an identical program still counts as work.
#[derive(Default)]
pub(crate) struct Capture {
    pub(crate) context: Option<Context>,
    pub(crate) targets: Vec<(usize, Context)>,
    pub(crate) pending: Pending,
    pub(crate) owners: Vec<usize>,
    pub(crate) frames: Vec<RawFrame>,
    pub(crate) labels: FxHashMap<usize, String>,
    pub(crate) frame_ops: usize,
    bytes: usize,
    pose_bytes: usize,
    peak_ops: usize,
    failure: Option<&'static str>,
    programs: FxHashSet<Arc<[RawCall]>>,
    pose_sequences: FxHashSet<Arc<[PoseRef]>>,
    last_pose: FxHashMap<usize, PoseRef>,
    last_parents: Option<Arc<[SongLuaOverlayState]>>,
    last_camera: Option<Arc<SongLuaOverlayState>>,
}

pub(crate) const MAX_BYTES: usize = 256 * 1024 * 1024;
const MAX_OPS: usize = 8192;
const WORK_LIMIT: &str = "Baked custom draw exceeds 8192 recorded operations per frame";
const BYTE_LIMIT: &str = "custom draw capture exceeds 256 MiB of retained storage per song";
pub(crate) const DRAW_INSTALLED_KEY: &str = "__songlua_draw_installed";

fn charge(bytes: &mut usize, allocation: usize) -> Result<(), &'static str> {
    let retained = bytes.saturating_add(allocation);
    if retained > MAX_BYTES {
        Err(BYTE_LIMIT)
    } else {
        *bytes = retained;
        Ok(())
    }
}

fn intern_slice<T: Clone + Eq + Hash>(
    pool: &mut FxHashSet<Arc<[T]>>,
    values: &[T],
    bytes: &mut usize,
) -> Result<Arc<[T]>, &'static str> {
    if let Some(shared) = pool.get(values) {
        return Ok(shared.clone());
    }
    charge(bytes, arc_bytes::<T>(values.len()))?;
    let shared: Arc<[T]> = Arc::from(values);
    pool.insert(shared.clone());
    Ok(shared)
}

impl Capture {
    #[cfg(feature = "test-support")]
    pub(crate) fn fixture_storage(&self) -> (usize, usize, usize) {
        (
            self.frames.len(),
            self.frames.iter().map(|frame| frame.calls.len()).sum(),
            self.bytes,
        )
    }
    pub(crate) fn reserve_bytes(&mut self, bytes: usize) -> Result<(), &'static str> {
        charge(&mut self.bytes, bytes).inspect_err(|&error| self.failure = Some(error))
    }

    fn submit(&mut self, live: bool) -> Result<usize, &'static str> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if !live && self.frame_ops >= MAX_OPS {
            self.failure = Some(WORK_LIMIT);
            return Err(WORK_LIMIT);
        }
        if live
            && self.pending.calls.len() >= MAX_BYTES / (size_of::<RawCall>() + size_of::<PoseRef>())
        {
            self.failure = Some(BYTE_LIMIT);
            return Err(BYTE_LIMIT);
        }
        self.frame_ops += 1;
        self.peak_ops = self.peak_ops.max(self.frame_ops);
        Ok(self.context.as_ref().unwrap().owner)
    }

    pub(crate) fn target(
        &mut self,
        target: usize,
        preserve: Option<bool>,
        live: bool,
    ) -> Result<(), &'static str> {
        let owner = self.submit(live)?;
        let command = preserve.map_or(Command::Finish { capture: target }, |preserve| {
            Command::Begin {
                capture: target,
                preserve,
            }
        });
        self.pending.calls.push(RawCall { owner, command });
        Ok(())
    }

    pub(crate) fn draw(
        &mut self,
        source: usize,
        mut pose: Pose,
        live: bool,
    ) -> Result<(), &'static str> {
        let owner = self.submit(live)?;
        // Check the source's previous pose first, then the most recent draw's
        // ancestor stack and camera (sibling actors frequently share both).
        let previous = self.last_pose.get(&source).cloned();
        let reference = if let Some(prior) = previous.as_ref().filter(|p| *p.0 == pose) {
            prior.clone()
        } else {
            let prior = previous.as_ref().map(|p| p.0.as_ref());
            let mut bytes = arc_bytes::<Pose>(1);
            if let Some(parents) = prior
                .map(|p| &p.parents)
                .into_iter()
                .chain(&self.last_parents)
                .find(|parents| **parents == pose.parents)
            {
                pose.parents = parents.clone();
            } else {
                bytes += arc_bytes::<SongLuaOverlayState>(pose.parents.len());
            }
            if let Some(camera) = &mut pose.camera {
                if let Some(shared) = prior
                    .and_then(|p| p.camera.as_ref())
                    .into_iter()
                    .chain(&self.last_camera)
                    .find(|shared| **shared == *camera)
                {
                    *camera = shared.clone();
                } else {
                    bytes += arc_bytes::<SongLuaOverlayState>(1);
                }
            }
            if let Some(vertices) = &mut pose.vertices {
                if let Some(shared) = prior
                    .and_then(|p| p.vertices.as_ref())
                    .filter(|shared| **shared == *vertices)
                {
                    *vertices = shared.clone();
                } else {
                    bytes += arc_bytes::<SongLuaOverlayMeshVertex>(vertices.len());
                }
            }
            self.reserve_bytes(bytes)?;
            self.pose_bytes += bytes;
            let reference = PoseRef(Arc::new(pose));
            self.last_pose.insert(source, reference.clone());
            reference
        };
        self.last_parents = Some(reference.0.parents.clone());
        self.last_camera = reference.0.camera.clone();
        let pose = self.pending.poses.len();
        self.pending.poses.push(reference);
        self.pending.calls.push(RawCall {
            owner,
            command: Command::Draw { source, pose },
        });
        Ok(())
    }

    pub(crate) fn record(
        &mut self,
        second: f32,
        immediate_owner: Option<usize>,
    ) -> Result<(), &'static str> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let immediate = immediate_owner.is_some();
        let owners = if let Some(owner) = immediate_owner {
            if self.pending.calls.is_empty() {
                return Ok(());
            }
            vec![owner]
        } else {
            std::mem::take(&mut self.owners)
        };
        self.reserve_bytes(size_of::<RawFrame>() + owners.capacity() * size_of::<usize>())?;
        let calls = intern_slice(&mut self.programs, &self.pending.calls, &mut self.bytes)
            .inspect_err(|&error| self.failure = Some(error))?;
        let poses = intern_slice(
            &mut self.pose_sequences,
            &self.pending.poses,
            &mut self.bytes,
        )
        .inspect_err(|&error| self.failure = Some(error))?;
        self.clear_calls();
        self.frames.push(RawFrame {
            second,
            immediate,
            owners,
            calls,
            poses,
        });
        Ok(())
    }

    pub(crate) fn clear_calls(&mut self) {
        self.pending.calls.clear();
        self.pending.poses.clear();
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.frames.is_empty() && self.pending.calls.is_empty() && self.targets.is_empty()
    }

    pub(crate) fn reset_storage(&mut self) {
        self.failure = None;
        self.frame_ops = 0;
        self.peak_ops = 0;
        self.bytes = 0;
        self.pose_bytes = 0;
        self.labels.clear();
        self.programs.clear();
        self.pose_sequences.clear();
        self.last_pose.clear();
        self.last_parents = None;
        self.last_camera = None;
    }

    pub(crate) fn take_frames(
        &mut self,
        sources: &FxHashMap<usize, DrawSource>,
    ) -> Result<Vec<(usize, DrawFrame)>, String> {
        if let Some(error) = self.failure {
            return Err(error.into());
        }
        log::debug!(
            "Compiled custom draws: peak_frame_operations={} capture_bytes={} pose_bytes={}",
            self.peak_ops,
            self.bytes,
            self.pose_bytes
        );
        convert(
            std::mem::take(&mut self.frames),
            &self.labels,
            sources,
            self.pose_bytes,
        )
    }
}

fn convert(
    raw_frames: Vec<RawFrame>,
    labels: &FxHashMap<usize, String>,
    sources: &FxHashMap<usize, DrawSource>,
    mut bytes: usize,
) -> Result<Vec<(usize, DrawFrame)>, String> {
    let overlay_index = |pointer| match sources.get(&pointer) {
        Some(DrawSource::Overlay(index)) => Ok(*index),
        _ => Err("custom draw references an uncompiled capture or callback owner".to_owned()),
    };
    let mut owners = Vec::new();
    for frame in raw_frames.iter().filter(|frame| !frame.immediate) {
        for &owner in &frame.owners {
            if !owners.contains(&owner) {
                owners.push(owner);
            }
        }
    }
    // Every normal frame carries the callback owner union, even quiet tails.
    let capacity = raw_frames.iter().try_fold(0usize, |capacity, frame| {
        let count = if frame.immediate {
            frame.owners.len()
        } else {
            // Repeated owner pointers retain their original slots.
            frame.owners.len()
                + owners
                    .iter()
                    .filter(|owner| !frame.owners.contains(owner))
                    .count()
        };
        let metadata = size_of::<(usize, DrawFrame)>() + 2 * size_of::<usize>();
        charge(&mut bytes, count.saturating_mul(metadata))?;
        Ok::<_, &'static str>(capacity + count)
    })?;
    let mut frames = Vec::with_capacity(capacity);
    let mut programs = FxHashSet::default();
    let mut pose_sequences = FxHashSet::default();
    let mut positions = FxHashMap::default();
    let mut batches = Vec::<(usize, Vec<Command<DrawSource>>, Vec<PoseRef>)>::new();
    for raw in raw_frames {
        let mut frame_owners = raw.owners;
        for &owner in owners.iter().filter(|_| !raw.immediate) {
            if !frame_owners.contains(&owner) {
                frame_owners.push(owner);
            }
        }
        positions.clear();
        batches.clear();
        for pointer in frame_owners {
            let owner = overlay_index(pointer)?;
            // As before, aliased sources route to the first matching output.
            positions.entry(owner).or_insert(batches.len());
            batches.push((owner, Vec::new(), Vec::new()));
        }
        for call in raw.calls.iter() {
            let owner = overlay_index(call.owner)?;
            let Some(&slot) = positions.get(&owner) else {
                return Err("custom draw lost its callback owner".to_owned());
            };
            let (_, commands, poses) = &mut batches[slot];
            let op = match call.command {
                Command::Begin { capture, preserve } => Command::Begin {
                    capture: overlay_index(capture)?,
                    preserve,
                },
                Command::Finish { capture } => Command::Finish {
                    capture: overlay_index(capture)?,
                },
                Command::Draw { source, pose } => {
                    let source = *sources.get(&source).ok_or_else(|| {
                        format!(
                            "custom draw references an uncompiled actor {} at {:.6}s",
                            labels.get(&source).map_or("<unnamed>", String::as_str),
                            raw.second,
                        )
                    })?;
                    let index = poses.len();
                    poses.push(raw.poses[pose].clone());
                    Command::Draw {
                        source,
                        pose: index,
                    }
                }
            };
            commands.push(op);
        }
        for (owner, commands, poses) in batches.drain(..) {
            frames.push((
                owner,
                DrawFrame {
                    second: raw.second,
                    immediate: raw.immediate,
                    owners: vec![owner],
                    owner_ends: vec![commands.len()],
                    ops: DrawOps {
                        batches: smallvec::smallvec![DrawBatch {
                            commands: intern_slice(&mut programs, &commands, &mut bytes)?,
                            poses: intern_slice(&mut pose_sequences, &poses, &mut bytes)?,
                        }],
                    },
                },
            ));
        }
    }
    log::debug!(
        "Compiled custom draw history: retained_bytes={bytes}, programs={}, pose_sequences={}",
        programs.len(),
        pose_sequences.len()
    );
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_ops_survive_the_bake_codec() {
        let draw = |x| DrawOp::Draw {
            source: DrawSource::Overlay(3),
            state: SongLuaOverlayState {
                x,
                ..Default::default()
            },
            parents: Arc::from([SongLuaOverlayState::default()]),
            inherited_diffuse: [1.0, 0.5, 0.25, 1.0],
            inherited_glow: [0.0; 4],
            camera: Some(SongLuaOverlayState::default()),
            vertices: None,
        };
        let ops = DrawOps::from(vec![
            DrawOp::Begin {
                capture: 1,
                preserve: true,
            },
            draw(12.0),
            draw(42.0),
            DrawOp::Finish { capture: 1 },
        ]);
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&ops, config).unwrap();
        let (decoded, used): (DrawOps, usize) = bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(decoded, ops);
        assert_eq!(decoded.len(), 4);
    }

    #[test]
    fn shared_program_keeps_each_invocation_pose_and_target_boundary() {
        let mut capture = Capture {
            context: Some(Context::default()),
            ..Default::default()
        };
        for (index, x) in [12.0, 42.0, 42.0].into_iter().enumerate() {
            capture.target(0, Some(false), false).unwrap();
            capture
                .draw(
                    1,
                    Pose {
                        state: SongLuaOverlayState {
                            x,
                            ..Default::default()
                        },
                        parents: Arc::from([]),
                        inherited_diffuse: [1.0; 4],
                        inherited_glow: [0.0; 4],
                        camera: None,
                        vertices: None,
                    },
                    false,
                )
                .unwrap();
            capture.target(0, None, false).unwrap();
            capture.record(index as f32, Some(0)).unwrap();
            capture.frame_ops = 0;
        }
        // An empty preserve pass is still submitted.
        capture.target(0, Some(true), false).unwrap();
        capture.target(0, None, false).unwrap();
        capture.record(2.0, Some(0)).unwrap();
        assert_eq!(capture.programs.len(), 2);
        assert_eq!(capture.pose_sequences.len(), 3);
        let frames = capture
            .take_frames(&FxHashMap::from_iter([
                (0, DrawSource::Overlay(0)),
                (1, DrawSource::Overlay(1)),
            ]))
            .unwrap();
        let first = &frames[0].1.ops.batches[0];
        let second = &frames[1].1.ops.batches[0];
        let third = &frames[2].1.ops.batches[0];
        assert!(Arc::ptr_eq(&first.commands, &second.commands));
        assert!(!Arc::ptr_eq(&first.poses, &second.poses));
        assert!(Arc::ptr_eq(&second.poses, &third.poses));
        for ((_, frame), x) in frames.iter().zip([12.0, 42.0, 42.0]) {
            assert!(frame.immediate);
            assert!(matches!(frame.ops.get(1), Some(DrawOp::Draw { state, .. }) if state.x == x));
        }
        assert_eq!(
            frames[3].1.ops.iter().collect::<Vec<_>>(),
            [
                DrawOp::Begin {
                    capture: 0,
                    preserve: true
                },
                DrawOp::Finish { capture: 0 },
            ]
        );
    }

    #[test]
    fn retained_draw_limit_is_separate_from_frame_work_limit() {
        let mut capture = Capture {
            context: Some(Context::default()),
            bytes: MAX_BYTES,
            ..Default::default()
        };
        capture.target(0, Some(false), false).unwrap();
        capture.target(0, None, false).unwrap();
        assert_eq!(capture.record(0.0, Some(0)), Err(BYTE_LIMIT));
        assert!(capture.frames.is_empty());
        assert_eq!(capture.peak_ops, 2);
    }
}
