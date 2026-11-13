//! Compiler-owned prediction of fallback queue clocks over canonical frames.
use super::*;

#[derive(Default)]
struct Queue {
    progress: Option<Arc<[[f32; 2]]>>,
    epoch: usize,
    frame: usize,
    remaining: f64,
    zero_frame: Option<usize>,
    steps: VecDeque<Option<f64>>,
    blocked: bool,
    dispatch: Option<(usize, f64)>,
}
#[derive(Clone, Copy)]
pub(super) struct Phase {
    pub order: usize,
    pub delta: f64,
}
#[derive(Clone, Copy)]
pub(super) struct Dispatch {
    pub actor: TableId,
    pub frame: usize,
    pub remaining: f64,
}
pub(super) struct Publication {
    pub cursor: Option<f32>,
    pub progress: Option<Arc<[[f32; 2]]>>,
    pub advance: f32,
}
pub(super) struct State {
    times: Vec<f64>,
    actor_frames: Option<Vec<[f32; 2]>>,
    pub frame: usize,
    epoch: usize,
    native: HashSet<TableId>,
    clocks: HashMap<TableId, Queue>,
    timing: HashMap<TableId, (f32, f32)>,
    recurring_frames: HashMap<TableId, usize>,
    pub orders: Arc<HashMap<TableId, usize>>,
    pub phase: Option<Phase>,
    pub dispatch: Option<Dispatch>,
}
impl State {
    pub(super) fn promote(&mut self, actor: TableId) {
        self.native.insert(actor);
        self.clocks.remove(&actor);
    }
    pub fn new(
        frames: &[(f64, f64)],
        actor_frames: Option<&[[f32; 2]]>,
        native: HashSet<TableId>,
    ) -> Self {
        let mut seconds = 0.;
        Self {
            times: frames
                .iter()
                .map(|(_, dt)| {
                    seconds += dt;
                    seconds
                })
                .collect(),
            actor_frames: actor_frames.map(<[_]>::to_vec),
            frame: 0,
            epoch: 0,
            native,
            clocks: HashMap::new(),
            timing: HashMap::new(),
            recurring_frames: HashMap::new(),
            orders: Arc::default(),
            phase: None,
            dispatch: None,
        }
    }
    pub fn time(&self, frame: usize) -> f64 {
        self.times.get(frame).copied().unwrap_or_else(|| {
            self.times.last().copied().unwrap_or(0.)
                + frame.saturating_sub(self.times.len()).saturating_add(1) as f64 / 60.
        })
    }
    fn delta(&self, frame: usize) -> f64 {
        if let Some(delta) = self.actor_frames.as_ref().and_then(|f| f.get(frame)) {
            return f64::from(delta[0]);
        }
        if frame == 0 {
            0.
        } else {
            f64::from(self.time(frame) as f32 - self.time(frame - 1) as f32)
        }
    }
    fn global_remainder(&self, frame: usize, remaining: f64) -> f64 {
        self.actor_frames
            .as_ref()
            .and_then(|f| f.get(frame))
            .map_or(remaining, |[delta, span]| {
                if *delta > 0. {
                    remaining * f64::from(*span) / f64::from(*delta)
                } else {
                    0.
                }
            })
    }
    fn order(&self, id: TableId) -> usize {
        self.orders.get(&id).copied().unwrap_or(0)
    }
    pub fn later(&self, id: TableId) -> bool {
        self.phase.is_some_and(|phase| self.order(id) > phase.order)
    }
    pub fn enter(&mut self, order: usize, delta: f32) {
        self.phase = Some(Phase {
            order,
            delta: f64::from(delta),
        });
        self.epoch += 1;
    }
    pub fn reset(&mut self, id: TableId) {
        self.clocks.remove(&id);
        self.timing.remove(&id);
        self.recurring_frames.remove(&id);
    }

    pub fn fallback(&self, id: TableId) -> bool {
        !self.native.contains(&id)
    }
    pub fn queue_len(&self, id: TableId) -> Option<usize> {
        self.fallback(id)
            .then(|| self.clocks.get(&id).map_or(0, |c| c.steps.len()))
    }
    pub fn pending_dispatch(&self, id: TableId) -> Option<(usize, f64)> {
        self.clocks.get(&id)?.dispatch
    }
    pub fn ready(&self, id: TableId) -> bool {
        self.pending_dispatch(id)
            .is_some_and(|(frame, _)| frame <= self.frame)
            && self.phase.is_none_or(|phase| self.order(id) <= phase.order)
    }
    pub fn waiting(&self, id: TableId) -> bool {
        self.clocks.get(&id).is_some_and(|clock| clock.blocked)
            || (self.pending_dispatch(id).is_some() && !self.ready(id))
    }
    pub fn cursor(&self, id: TableId) -> f32 {
        self.timing.get(&id).map_or(0., |t| t.0)
    }
    pub fn consumed_recurring(&self, id: TableId) -> bool {
        self.recurring_frames.get(&id) == Some(&self.frame)
    }
    pub fn mark_recurring(&mut self, id: TableId) {
        if self.clocks.contains_key(&id) {
            self.recurring_frames.insert(id, self.frame);
        }
    }
    pub fn immediate_advance(&self, id: TableId) -> f32 {
        self.dispatch.map_or_else(
            || {
                self.phase
                    .filter(|phase| self.order(id) > phase.order)
                    .map_or(0., |phase| phase.delta as f32)
            },
            |d| (self.time(self.frame) - self.time(d.frame)) as f32,
        )
    }
    pub fn actor_frame_advance(&self, id: TableId) -> f32 {
        // A message command's playback phase belongs to actor traversal,
        // independently of the queued callback's dispatch timestamp.
        self.phase
            .filter(|phase| self.order(id) > phase.order)
            .map_or(0., |phase| phase.delta as f32)
    }
    pub fn prime(&mut self, id: TableId) {
        if let Some(mut clock) = self.clocks.remove(&id) {
            if clock.dispatch.is_none() {
                while let Some(step) = clock.steps.pop_front() {
                    advance(self, &mut clock, step.unwrap_or(0.), None);
                    if clock.blocked {
                        break;
                    }
                    if step.is_none() {
                        clock.dispatch = Some((clock.frame, clock.remaining));
                        break;
                    }
                }
            }
            self.clocks.insert(id, clock);
        }
    }
    pub fn hurry(&mut self, id: TableId, factor: f32) {
        if let Some((cursor, duration)) = self.timing.get_mut(&id) {
            *cursor = (*cursor + duration.max(0.)) * factor;
            *duration = 0.;
        }
        if let Some(clock) = self.clocks.get_mut(&id) {
            for d in clock.steps.iter_mut().flatten() {
                *d = f64::from(*d as f32 * factor);
            }
            clock.progress = None;
        }
    }
    pub fn tween(&mut self, id: TableId, duration: Option<f64>) {
        if self.native.contains(&id) {
            return;
        }
        let (cursor, prior) = self.timing.entry(id).or_default();
        *cursor += prior.max(0.);
        *prior = 0.;
        if let Some(delay) = self.record(id, duration) {
            self.timing.get_mut(&id).unwrap().0 = delay;
        }
        if let Some(duration) = duration {
            self.timing.get_mut(&id).unwrap().1 = duration as f32;
        } else {
            self.prime(id);
        }
    }

    fn record(&mut self, id: TableId, duration: Option<f64>) -> Option<f32> {
        let phase = self.phase?;
        let later = self.order(id) > phase.order;
        let (frame, remaining) = self
            .dispatch
            .map(|d| {
                (
                    d.frame,
                    if d.actor == id {
                        d.remaining
                    } else if self.order(id) > self.order(d.actor) {
                        self.delta(d.frame)
                    } else {
                        0.
                    },
                )
            })
            .unwrap_or((self.frame, if later { phase.delta } else { 0. }));
        let mut clock = self.clocks.remove(&id).unwrap_or(Queue {
            epoch: self.epoch,
            frame,
            remaining,
            ..Default::default()
        });
        let changed = clock.epoch != self.epoch;
        if changed && clock.dispatch.is_none() {
            while let Some(step) = clock.steps.pop_front() {
                advance(self, &mut clock, step.unwrap_or(0.), None);
            }
            clock.epoch = self.epoch;
        }
        if clock.steps.is_empty()
            && (clock.frame < frame || (clock.frame == frame && clock.remaining > remaining))
        {
            clock.frame = frame;
            clock.remaining = remaining;
        }
        let delay = changed.then(|| {
            (self.time(clock.frame) - clock.remaining - (self.time(frame) - remaining)).max(0.)
                as f32
        });
        clock.steps.push_back(duration);
        clock.progress = None;
        self.clocks.insert(id, clock);
        delay
    }
    pub fn flush(&mut self, id: TableId) {
        if let Some((cursor, duration)) = self.timing.get_mut(&id) {
            *cursor += duration.max(0.);
            *duration = 0.;
        }
    }
    pub fn dispatched(&mut self, id: TableId, remaining: f32) -> Option<Dispatch> {
        let prior = self.dispatch;
        let mut time = (self.frame, f64::from(remaining));
        if !self.native.contains(&id)
            && let Some(mut clock) = self.clocks.remove(&id)
        {
            if let Some(dispatch) = clock.dispatch.take() {
                time = dispatch;
            }
            self.timing
                .insert(id, (clock.steps.iter().flatten().sum::<f64>() as f32, 0.));
            clock.epoch = self.epoch;
            self.clocks.insert(id, clock);
        }
        self.dispatch = Some(Dispatch {
            actor: id,
            frame: time.0,
            remaining: time.1,
        });
        prior
    }
    fn progress(&mut self, id: TableId) -> Option<Arc<[[f32; 2]]>> {
        let mut clock = self.clocks.remove(&id)?;
        if clock.progress.is_none() && !clock.steps.is_empty() {
            let mut sample = Queue {
                epoch: clock.epoch,
                frame: clock.frame,
                remaining: clock.remaining,
                zero_frame: clock.zero_frame,
                blocked: clock.blocked,
                ..Default::default()
            };
            let mut samples = Vec::new();
            for (index, duration) in clock.steps.iter().enumerate() {
                advance(
                    self,
                    &mut sample,
                    duration.unwrap_or(0.),
                    (index + 1 == clock.steps.len()).then_some(&mut samples),
                );
            }
            if !samples.is_empty() {
                clock.progress = Some(samples.into());
            }
        }
        let progress = clock.progress.clone();
        self.clocks.insert(id, clock);
        progress
    }
    pub fn publication(&mut self, id: TableId, timed: bool) -> Publication {
        let native = self.native.contains(&id);
        let cursor = self.timing.get(&id).map_or(0., |t| t.0);
        let empty = self
            .clocks
            .get(&id)
            .is_some_and(|c| c.steps.back() == Some(&Some(0.)));
        let progress = if native {
            None
        } else if timed {
            self.progress(id)
        } else if empty {
            let progress = self.progress(id);
            let later = self.phase.is_some_and(|p| self.order(id) > p.order);
            progress.filter(|p| {
                later
                    || p.first()
                        .is_some_and(|s| s[0] > self.time(self.frame) as f32)
            })
        } else {
            None
        };
        let advance = if let Some(scope) = self.dispatch {
            let advance = if scope.actor == id {
                scope.remaining
            } else if self.order(id) > self.order(scope.actor) {
                self.delta(scope.frame)
            } else {
                0.
            };
            (self.time(self.frame) - self.time(scope.frame) + advance) as f32
        } else {
            self.phase
                .filter(|p| self.order(id) > p.order)
                .map_or(0., |p| p.delta as f32)
        };
        Publication {
            // Frame capture clears its command timing while the semantic
            // queue remains pending. A later setter retargets that queue;
            // it must not publish a fresh zero-duration scheduled write.
            cursor: (!native && timed).then_some(cursor.max(0.)),
            progress,
            advance,
        }
    }
}

fn advance(
    frames: &State,
    clock: &mut Queue,
    duration: f64,
    mut samples: Option<&mut Vec<[f32; 2]>>,
) {
    if clock.blocked || !(duration as f32).is_finite() {
        clock.blocked = true;
        return;
    }
    // Actor::UpdateTweening subtracts native floats on each frame. A double
    // clock can release a zero-time command early and change later RNG calls.
    let mut left = duration as f32;
    loop {
        while clock.remaining <= 0.0 {
            let Some(frame) = clock.frame.checked_add(1) else {
                clock.blocked = true;
                return;
            };
            clock.frame = frame;
            clock.remaining = frames.delta(clock.frame);
        }
        if duration as f32 == 0.0 {
            clock.zero_frame = Some(clock.frame);
        }
        // A callback after its child can enqueue a zero-time state on the final
        // captured frame. Its start still belongs to the next update, even when
        // that update lies beyond the replay. Omitting this sample makes the
        // pending destination look immediate and overwrites the last draw.
        if let Some(samples) = &mut samples
            && samples.is_empty()
        {
            let start = frames.time(clock.frame)
                - if left == 0.0 {
                    0.0
                } else {
                    frames.global_remainder(clock.frame, clock.remaining)
                };
            let mut start = start as f32;
            if left > 0.0 && clock.frame > 0 {
                let prior = frames.time(clock.frame - 1) as f32;
                // A tiny remainder can put the next tween strictly after the
                // prior update while both timestamps round to the same float.
                // Preserve queue order: it cannot draw on that earlier frame.
                if frames.time(clock.frame) - frames.global_remainder(clock.frame, clock.remaining)
                    > f64::from(prior)
                    && start <= prior
                {
                    start = prior.next_up();
                }
            }
            if let Some(frame) = clock.zero_frame {
                start = start.max(frames.time(frame) as f32);
            }
            samples.push([start, left]);
        }
        let elapsed = left.min(clock.remaining as f32);
        left -= elapsed;
        clock.remaining = f64::from(clock.remaining as f32 - elapsed);
        if let Some(samples) = &mut samples
            && clock.frame < frames.times.len()
            && left != duration as f32
        {
            samples.push([frames.time(clock.frame) as f32, left]);
        }
        if left == 0.0 {
            break;
        }
        if clock.frame >= frames.times.len() {
            let count = (left * 60.0).floor() as usize;
            if count > 1 {
                clock.frame = clock.frame.saturating_add(count - 1);
                left -= (count - 1) as f32 / 60.0;
            }
        }
    }
}

impl Actors {
    pub(crate) fn discard_song_replay_clock(&self, vm: &mut Vm) {
        vm.store().replay_clock = None;
    }

    pub(crate) fn set_song_replay_frames(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        frames: &[(f64, f64)],
        actor_frames: Option<&[[f32; 2]]>,
    ) -> bool {
        let store = vm.store();
        if store.replay_frame.map(|f| f.0) != Some(screen.id)
            || store.replay_advancing
            || store.clock.live.load(std::sync::atomic::Ordering::Relaxed)
        {
            return false;
        }
        let native = store
            .actors
            .iter()
            .filter(|(_, a)| a.scene == screen.id && a.capture.is_none())
            .map(|(id, _)| *id)
            .collect();
        store.replay_clock = Some(State::new(frames, actor_frames, native));
        true
    }
    pub(crate) fn set_song_replay_frame(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        frame: usize,
    ) -> Option<f64> {
        let store = vm.store();
        if store.replay_frame?.0 != screen.id {
            return None;
        }
        let clock = store.replay_clock.as_mut()?;
        clock.frame = frame;
        Some(clock.time(frame))
    }
}
