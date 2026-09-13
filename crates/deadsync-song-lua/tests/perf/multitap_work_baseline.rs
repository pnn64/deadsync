// Frozen from 34aca2540 (0.5.1190), before this performance pass.
use super::super::*;

pub fn push_overlay_sample_eases(
    out: &mut Vec<SongLuaOverlayEase>,
    overlay_index: usize,
    baseline: SongLuaOverlayState,
    samples: &[(f32, SongLuaOverlayState)],
) {
    if let Some((start, state)) = samples.first().copied() {
        push_overlay_sample_instant_state(out, overlay_index, start, baseline, state);
    }
    for window in samples.windows(2) {
        let [(start, from), (end, to)] = [window[0], window[1]];
        if end <= start {
            continue;
        }
        match (from.visible, to.visible) {
            (true, true) => {
                push_overlay_sample_linear_ease(out, overlay_index, baseline, start, end, from, to);
            }
            (false, true) => {
                push_overlay_sample_instant_state(out, overlay_index, end, baseline, to);
            }
            (true, false) => push_overlay_sample_instant_visible(out, overlay_index, end, false),
            (false, false) => {}
        }
    }
}

pub fn push_multitap_explosion_eases(
    out: &mut Vec<SongLuaOverlayEase>,
    overlay_index: usize,
    baseline: SongLuaOverlayState,
    context: &SongLuaCompileContext,
    descs: &[MultitapDesc],
    lane: usize,
) {
    let mut beats = descs
        .iter()
        .filter(|desc| desc.lane == lane)
        .flat_map(|desc| {
            [
                multitap_visible_start(desc.taps[0]),
                desc.taps[desc.taps.len() - 1].next_up(),
            ]
        })
        .collect::<Vec<_>>();
    beats.sort_by(f32::total_cmp);
    beats.dedup();
    let samples = beats
        .into_iter()
        .map(|beat| {
            let visible = descs
                .iter()
                .any(|desc| desc.lane == lane && calc_multitap_phase(desc, beat).visible);
            (
                beat,
                multitap_explosion_state(baseline, context, lane, visible),
            )
        })
        .collect::<Vec<_>>();
    push_overlay_sample_eases(out, overlay_index, baseline, &samples);
}

pub fn calc_multitap_phase(desc: &MultitapDesc, beat: f32) -> MultitapPhase {
    let mut out = MultitapPhase {
        pos: 0.0,
        squish: 0.0,
        lin: 0.0,
        qtc: 0,
        visible: false,
    };
    if beat > desc.taps[desc.taps.len() - 1] {
        return out;
    }
    out.pos = desc.taps[0] - beat;
    out.qtc = calc_multitap_qtzn(Some(desc.taps[0]));
    out.visible = out.pos < MULTITAP_PREVISIBLE_BEATS;
    let mut elasticity = desc
        .peak
        .zip(desc.taps.get(1).copied())
        .map(|(peak, second)| peak / (second - desc.taps[0]))
        .unwrap_or(MULTITAP_BASE_BOUNCE);
    for index in 0..desc.taps.len() {
        if beat <= desc.taps[index] || index + 1 >= desc.taps.len() {
            break;
        }
        let gap = desc.taps[index + 1] - desc.taps[index];
        if gap <= f32::EPSILON {
            continue;
        }
        elasticity = desc
            .peak
            .map(|peak| peak / gap)
            .unwrap_or(elasticity * MULTITAP_ELASTICITY);
        let t = beat - desc.taps[index];
        out.pos = elasticity * t * (gap - t) / gap;
        let velocity = elasticity * 2.0f32.mul_add(-t, gap) / gap;
        out.squish = MULTITAP_SQUISHY * (velocity.abs() - 0.5);
        out.lin = t / gap;
        out.qtc = calc_multitap_qtzn(desc.taps.get(index + 1).copied());
        out.visible = true;
    }
    out
}
