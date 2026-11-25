//! Shared column handler snapshots for Baked and Runtime.
use super::*;
use deadsync_gameplay::{
    SONG_LUA_SPLINE_PAGE_POINTS as PAGE_POINTS, SongLuaColumnSplineFrame as Frame,
    SongLuaColumnSplineTrack as Track, SongLuaSplineAxisPage,
    SongLuaSplineCoefficients as Coefficients, SongLuaSplineData as Spline, SongLuaSplinePages,
    SongLuaSplinePointCoefficients as PointCoefficients, SongLuaSplineSolver,
};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::{collections::BTreeMap, hash::Hasher};

const MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;

// Arc allocates the reference counts and slice together.
pub(crate) fn arc_bytes<T>(len: usize) -> usize {
    let alignment = align_of::<T>().max(align_of::<usize>());
    let header = (2 * size_of::<usize>()).next_multiple_of(align_of::<T>());
    (header + len * size_of::<T>()).next_multiple_of(alignment)
}

fn coefficient_bits(
    rows: impl IntoIterator<Item = PointCoefficients>,
) -> impl Iterator<Item = u32> {
    rows.into_iter().flatten().flatten().map(f32::to_bits)
}

fn page_bits(rows: &[[f32; 4]]) -> impl Iterator<Item = u32> + '_ {
    rows.iter().flatten().map(|v| v.to_bits())
}

fn hash_bits(bits: impl Iterator<Item = u32>) -> u64 {
    let mut hash = rustc_hash::FxHasher::default();
    for value in bits {
        hash.write_u32(value);
    }
    hash.finish()
}

#[derive(Default)]
struct SplineStorage {
    shapes: FxHashMap<u64, SmallVec<[Coefficients; 1]>>,
    pages: FxHashMap<u64, SmallVec<[SongLuaSplineAxisPage; 1]>>,
    new_pages: Vec<u64>,
    coefficient_bytes: usize,
    frame_bytes: usize,
    logical_coefficient_bytes: u64,
    dense_unique_bytes: u64,
}

impl SplineStorage {
    fn intern(&mut self, coefficients: &mut Coefficients) -> Option<(u64, usize, usize)> {
        let Coefficients::Dense(points) = coefficients else {
            unreachable!()
        };
        let hash = hash_bits(coefficient_bits(points.iter().copied()));
        if let Some(shared) = self.shapes.get(&hash).and_then(|bucket| {
            bucket.iter().find(|shape| {
                matches!(shape, Coefficients::Dense(shared) if Arc::ptr_eq(shared, points))
                    || (shape.len() == points.len()
                        && coefficient_bits(shape.iter())
                            .eq(coefficient_bits(points.iter().copied())))
            })
        }) {
            *coefficients = shared.clone();
            return None;
        }

        let dense_bytes = arc_bytes::<PointCoefficients>(points.len());
        let bytes = if points.len() <= PAGE_POINTS {
            // A full set of page references costs more than a small dense curve.
            dense_bytes
        } else {
            let mut bytes = arc_bytes::<SongLuaSplinePages>(1);
            let first_new_page = self.new_pages.len();
            let mut pages = Vec::with_capacity(points.len().div_ceil(PAGE_POINTS));
            for chunk in points.chunks(PAGE_POINTS) {
                let mut axis = [[0.0; 4]; PAGE_POINTS];
                pages.push(std::array::from_fn(|index| {
                    for (out, point) in axis.iter_mut().zip(chunk) {
                        *out = point[index];
                    }
                    let axis = &axis[..chunk.len()];
                    let hash = hash_bits(page_bits(axis));
                    let bucket = self.pages.entry(hash).or_default();
                    if let Some(shared) = bucket.iter().find(|page| {
                        page.len() == axis.len() && page_bits(page).eq(page_bits(axis))
                    }) {
                        shared.clone()
                    } else {
                        let page: SongLuaSplineAxisPage = Arc::from(axis);
                        bytes += arc_bytes::<[f32; 4]>(page.len());
                        bucket.push(page.clone());
                        self.new_pages.push(hash);
                        page
                    }
                }));
            }
            bytes += pages.len() * size_of::<[SongLuaSplineAxisPage; 3]>();
            if bytes < dense_bytes {
                *coefficients = Coefficients::Paged(Arc::new(SongLuaSplinePages::new(
                    points.len(),
                    pages.into_boxed_slice(),
                )));
                bytes
            } else {
                for hash in self.new_pages.drain(first_new_page..).rev() {
                    forget(&mut self.pages, hash);
                }
                dense_bytes
            }
        };
        self.shapes
            .entry(hash)
            .or_default()
            .push(coefficients.clone());
        Some((hash, bytes, dense_bytes))
    }

    fn reject(&mut self, allocations: [Option<(u64, usize, usize)>; 3]) {
        for (hash, _, _) in allocations.into_iter().rev().flatten() {
            forget(&mut self.shapes, hash);
        }
        for hash in self.new_pages.drain(..).rev() {
            forget(&mut self.pages, hash);
        }
    }
}

fn forget<A: smallvec::Array>(pool: &mut FxHashMap<u64, SmallVec<A>>, hash: u64) {
    let bucket = pool.get_mut(&hash).unwrap();
    bucket.pop();
    if bucket.is_empty() {
        pool.remove(&hash);
    }
}

#[derive(Default)]
pub(super) struct Scratch {
    pub points: Vec<[f32; 3]>,
    pub solver: SongLuaSplineSolver,
    pub coefficients: Vec<[[f32; 4]; 3]>,
    pub recent: Option<Arc<[PointCoefficients]>>,
}
#[derive(Default)]
pub(super) struct Lane {
    pub frames: Vec<Frame>,
    pub last_raw: Option<Frame>,
    reflexive: [Option<bool>; 3],
    has_spline: bool,
}
#[derive(Default)]
pub(super) struct State {
    pub scratch: [Scratch; 3],
    pub sampled: Vec<(usize, usize, Frame)>,
    pub lanes: BTreeMap<(usize, usize), Lane>,
    storage: SplineStorage,
    live: bool,
}
/// Load-lifetime reader/solver buffers and owned tracks. Every coordinate is
/// validated on each sample; geometry reuse never relies on Lua table identity.
pub struct SongColumnSplineCapture {
    state: Arc<Mutex<Option<State>>>,
}
impl Default for SongColumnSplineCapture {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(Some(State::default()))),
        }
    }
}
impl SongColumnSplineCapture {
    pub(super) fn live() -> Self {
        Self {
            state: Arc::new(Mutex::new(Some(State {
                live: true,
                ..State::default()
            }))),
        }
    }

    pub(super) fn current(&self) -> [Vec<Track>; LUA_PLAYERS] {
        let state = self.state.lock().unwrap();
        let state = state.as_ref().expect("active column capture");
        let mut tracks = std::array::from_fn(|_| Vec::new());
        for (&(player, column), lane) in &state.lanes {
            if let Some(frame) = lane.frames.last().filter(|frame| has_spline(frame))
                && let Some(tracks) = tracks.get_mut(player)
            {
                tracks.push(Track {
                    player,
                    column,
                    time_offset: 0.0,
                    frames: Arc::from([frame.clone()]),
                });
            }
        }
        tracks
    }

    pub fn finish(self) -> Result<Vec<Track>, String> {
        let state = self
            .state
            .lock()
            .unwrap()
            .take()
            .ok_or("column spline capture is active or finished")?;
        Ok(state.finish())
    }
}
impl SongRuntime {
    pub fn capture_song_column_splines(
        &mut self,
        capture: &mut SongColumnSplineCapture,
        second: f32,
    ) -> Result<(), String> {
        self.actors
            .capture_column_splines(&mut self.vm, capture.state.clone(), second)
            .map_err(|e| e.to_string())
    }
    pub fn read_song_note_column_zoom_hides(
        &mut self,
    ) -> Result<Vec<crate::SongLuaNoteHideWindow>, String> {
        self.actors
            .read_column_zoom_hides(&mut self.vm)
            .map_err(|e| e.to_string())
    }
}
fn captured_spline_matches(
    previous: &Option<Spline>,
    next: &Option<Spline>,
    reflexive: &mut Option<bool>,
) -> bool {
    match (previous, next) {
        (Some(previous), Some(next)) => {
            if previous.constant != next.constant
                || previous.absolute != next.absolute
                || previous.looping != next.looping
                || previous.beats_per_t != next.beats_per_t
                || previous.receptor_t != next.receptor_t
                || previous.subtract_song_beat != next.subtract_song_beat
            {
                return false;
            }
            if previous.coefficients.ptr_eq(&next.coefficients) {
                *reflexive.get_or_insert_with(|| {
                    previous
                        .coefficients
                        .iter()
                        .flatten()
                        .flatten()
                        .all(|value| !value.is_nan())
                })
            } else {
                previous.coefficients == next.coefficients
            }
        }
        (None, None) => true,
        _ => false,
    }
}
fn captured_spline_reflexive(
    next: &Option<Spline>,
    previous: Option<&Option<Spline>>,
    previous_reflexive: Option<bool>,
) -> Option<bool> {
    if previous
        .and_then(Option::as_ref)
        .zip(next.as_ref())
        .is_some_and(|(previous, next)| previous.coefficients.ptr_eq(&next.coefficients))
    {
        previous_reflexive
    } else {
        None
    }
}

fn spline_handlers(frame: &Frame) -> [&Option<Spline>; 3] {
    [&frame.position, &frame.rotation, &frame.zoom]
}

impl State {
    pub fn ingest(&mut self, second: f32) -> Result<(), String> {
        if self.live {
            self.lanes.retain(|&(player, column), _| {
                self.sampled
                    .iter()
                    .any(|&(p, c, _)| p == player && c == column)
            });
        }
        for (player, column, mut frame) in self.sampled.drain(..) {
            let lane = self.lanes.entry((player, column)).or_default();
            let last = lane.last_raw.as_ref().or_else(|| lane.frames.last());
            let next = spline_handlers(&frame);
            if last.is_some_and(|last| {
                spline_handlers(last)
                    .into_iter()
                    .zip(next)
                    .zip(&mut lane.reflexive)
                    .all(|((last, next), reflexive)| captured_spline_matches(last, next, reflexive))
            }) {
                continue;
            }
            let raw = (!self.live).then(|| frame.clone());
            if !self.live {
                let storage = &mut self.storage;
                // Reuse a lane's existing binding or look up the exact curve
                // and its coefficient pages in the layer-wide storage.
                storage.new_pages.clear();
                let mut allocations = [None; 3];
                let mut logical = 0;
                for (axis, (spline, allocation)) in
                    [&mut frame.position, &mut frame.rotation, &mut frame.zoom]
                        .into_iter()
                        .zip(&mut allocations)
                        .enumerate()
                {
                    let Some(spline) = spline else { continue };
                    logical += (size_of::<PointCoefficients>() * spline.coefficients.len()) as u64;
                    if last.and_then(|last| spline_handlers(last)[axis].as_ref())
                        .is_some_and(|last| last.coefficients.ptr_eq(&spline.coefficients))
                        && let Some(stored) = lane.frames.last().and_then(|f| spline_handlers(f)[axis].as_ref())
                    {
                        spline.coefficients = stored.coefficients.clone();
                    } else {
                        *allocation = storage.intern(&mut spline.coefficients);
                    }
                }
                let coefficient_bytes = storage.coefficient_bytes
                    + allocations
                        .iter()
                        .flatten()
                        .map(|(_, bytes, _)| bytes)
                        .sum::<usize>();
                let has_spline = has_spline(&frame);
                let frame_bytes = if lane.has_spline {
                    arc_bytes::<Frame>(lane.frames.len() + 1)
                        - arc_bytes::<Frame>(lane.frames.len())
                } else if has_spline {
                    size_of::<Track>() + arc_bytes::<Frame>(lane.frames.len() + 1)
                } else {
                    0
                };
                let retained = coefficient_bytes + storage.frame_bytes + frame_bytes;
                if retained > MAX_RETAINED_BYTES {
                    storage.reject(allocations);
                    return Err(format!(
                        "Baked column spline tracks exceed 128 MiB of retained storage per layer at {second:.3}s ({retained} bytes, {} coefficient bytes)",
                        coefficient_bytes,
                    ));
                }
                storage.coefficient_bytes = coefficient_bytes;
                storage.logical_coefficient_bytes += logical;
                storage.dense_unique_bytes += allocations
                    .into_iter()
                    .flatten()
                    .map(|(_, _, dense)| dense as u64)
                    .sum::<u64>();
                storage.frame_bytes += frame_bytes;
                lane.has_spline |= has_spline;
            }
            let raw_next = raw.as_ref().unwrap_or(&frame);
            let next = spline_handlers(raw_next);
            lane.reflexive = std::array::from_fn(|i| {
                captured_spline_reflexive(
                    next[i],
                    last.map(|last| spline_handlers(last)[i]),
                    lane.reflexive[i],
                )
            });
            if self.live {
                lane.frames.clear();
            }
            frame.second = if self.live { f32::NEG_INFINITY } else { second };
            lane.last_raw = raw;
            lane.frames.push(frame);
        }
        Ok(())
    }
    fn finish(self) -> Vec<Track> {
        log::debug!(
            "Compiled column splines: lanes={} frames={} retained_bytes={} coefficient_bytes={} dense_unique_bytes={} unshared_coefficient_bytes={}",
            self.lanes.len(),
            self.lanes
                .values()
                .map(|lane| lane.frames.len())
                .sum::<usize>(),
            self.storage.frame_bytes + self.storage.coefficient_bytes,
            self.storage.coefficient_bytes,
            self.storage.dense_unique_bytes,
            self.storage.logical_coefficient_bytes,
        );
        let mut tracks =
            Vec::with_capacity(self.lanes.values().filter(|lane| lane.has_spline).count());
        for ((player, column), lane) in self.lanes {
            if lane.has_spline {
                tracks.push(Track {
                    player,
                    column,
                    time_offset: 0.,
                    frames: lane.frames.into(),
                });
            }
        }
        tracks
    }
}

fn has_spline(frame: &Frame) -> bool {
    frame.position.is_some() || frame.rotation.is_some() || frame.zoom.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coefficient_pages_share_unchanged_axes_and_preserve_every_float_bit() {
        let mut storage = SplineStorage::default();
        let mut first =
            vec![[[0.0, -0.0, 1.0, f32::from_bits(0x7fc00001)]; 3]; 2 * PAGE_POINTS + 1];
        first[PAGE_POINTS][1][1] = -4.5;
        let mut second = first.clone();
        second[PAGE_POINTS][1][1] = 9.0;
        let mut a: Coefficients = first.clone().into();
        let mut b: Coefficients = second.clone().into();
        storage.intern(&mut a).unwrap();
        storage.intern(&mut b).unwrap();
        assert!(coefficient_bits(a.iter()).eq(coefficient_bits(first)));
        assert!(coefficient_bits(b.iter()).eq(coefficient_bits(second)));
        let (Coefficients::Paged(a), Coefficients::Paged(b)) = (a, b) else {
            panic!("paged fixtures")
        };
        for (page, (a, b)) in a.pages().iter().zip(b.pages()).enumerate() {
            for (axis, (a, b)) in a.iter().zip(b).enumerate() {
                assert_eq!(Arc::ptr_eq(a, b), (page, axis) != (1, 1));
            }
        }
    }

    #[test]
    fn spline_budget_counts_unique_coefficients_and_frames_without_leaking_rejected_shapes() {
        let mut state = State::default();
        state.storage.coefficient_bytes = MAX_RETAINED_BYTES
            - size_of::<Track>()
            - arc_bytes::<Frame>(1)
            - arc_bytes::<[[f32; 4]; 3]>(1);
        let frame = |value| Frame {
            second: 0.0,
            position: Some(Spline {
                coefficients: vec![[[value; 4]; 3]].into(),
                absolute: false,
                constant: true,
                looping: false,
                beats_per_t: 1.0,
                receptor_t: 0.0,
                subtract_song_beat: true,
            }),
            rotation: None,
            zoom: None,
        };
        state.sampled.push((0, 0, frame(1.0)));
        assert!(state.ingest(0.0).is_ok());
        state.sampled.push((0, 0, frame(2.0)));
        assert!(
            state
                .ingest(1.0)
                .unwrap_err()
                .contains("retained storage per layer")
        );
        assert_eq!(state.storage.shapes.len(), 1);
        assert_eq!(state.lanes[&(0, 0)].frames.len(), 1);
        state.sampled.push((0, 0, frame(1.0)));
        assert!(state.ingest(2.0).is_ok());
    }
}
