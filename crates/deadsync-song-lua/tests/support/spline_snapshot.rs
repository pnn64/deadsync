//! Preserve numeric bits and coefficient sharing without recording heap addresses.
#![allow(dead_code)]

use deadsync_gameplay::{SongLuaColumnSplineTrack, SongLuaSplineData};
use deadsync_song_lua::{SongLuaUpdateModState, SongStaticUpdateOutput};
use std::sync::Arc;

#[derive(Debug)]
pub struct Spline {
    constant: bool,
    looping: bool,
    subtract_song_beat: bool,
    beats_per_t: u32,
    receptor_t: u32,
    coefficients: Vec<[[u32; 4]; 3]>,
}

#[derive(Debug)]
pub struct Frame {
    second: u32,
    position: Option<Spline>,
    zoom: Option<Spline>,
}

#[derive(Debug)]
pub struct Track {
    player: usize,
    column: usize,
    time_offset: u32,
    frames: Vec<Frame>,
}

fn spline(value: &SongLuaSplineData) -> Spline {
    Spline {
        constant: value.constant,
        looping: value.looping,
        subtract_song_beat: value.subtract_song_beat,
        beats_per_t: value.beats_per_t.to_bits(),
        receptor_t: value.receptor_t.to_bits(),
        coefficients: value
            .coefficients
            .iter()
            .map(|coefficient| coefficient.map(|axis| axis.map(f32::to_bits)))
            .collect(),
    }
}

pub fn records(tracks: &[SongLuaColumnSplineTrack]) -> Vec<Track> {
    tracks
        .iter()
        .map(|track| Track {
            player: track.player,
            column: track.column,
            time_offset: track.time_offset.to_bits(),
            frames: track
                .frames
                .iter()
                .map(|frame| Frame {
                    second: frame.second.to_bits(),
                    position: frame.position.as_ref().map(spline),
                    zoom: frame.zoom.as_ref().map(spline),
                })
                .collect(),
        })
        .collect()
}

pub fn tracks(tracks: &[SongLuaColumnSplineTrack]) -> impl std::fmt::Debug {
    let coefficients = tracks
        .iter()
        .flat_map(|track| track.frames.iter())
        .flat_map(|frame| [&frame.position, &frame.zoom])
        .filter_map(|spline| spline.as_ref().map(|spline| &spline.coefficients))
        .collect::<Vec<_>>();
    let sharing = coefficients
        .iter()
        .map(|left| {
            coefficients
                .iter()
                .map(|right| Arc::ptr_eq(left, right))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    (records(tracks), sharing)
}

pub fn update_output(output: &SongStaticUpdateOutput) -> impl std::fmt::Debug + '_ {
    let eases = output
        .eases
        .iter()
        .map(|ease| {
            (
                [ease.start, ease.limit, ease.from, ease.to].map(f32::to_bits),
                ease.approach_speed.map(f32::to_bits),
            )
        })
        .collect::<Vec<_>>();
    let transforms = output
        .column_transforms
        .iter()
        .map(|transform| {
            [
                transform.start,
                transform.limit,
                transform.from_y,
                transform.to_y,
            ]
            .map(f32::to_bits)
        })
        .collect::<Vec<_>>();
    (
        &output.eases,
        eases,
        &output.column_transforms,
        transforms,
        records(&output.column_splines),
    )
}

pub fn states(states: &[SongLuaUpdateModState; 2]) -> [Vec<(&str, u32)>; 2] {
    states.each_ref().map(|state| {
        state
            .iter()
            .map(|(name, value)| (name.as_str(), value.to_bits()))
            .collect()
    })
}
