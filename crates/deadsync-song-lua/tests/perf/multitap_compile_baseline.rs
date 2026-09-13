// Frozen from 5ba1955cd (0.5.1191); only visibility/curve helpers extracted for isolated benchmarks.
use super::super::*;

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
    // Consume states as they are produced; only adjacent samples are needed.
    let samples = beats.into_iter().map(|beat| {
        let visible = descs
            .iter()
            .any(|desc| desc.lane == lane && calc_multitap_phase(desc, beat).visible);
        (
            beat,
            multitap_explosion_state(baseline, context, lane, visible),
        )
    });
    push_overlay_sample_eases_iter(out, overlay_index, baseline, samples);
}

pub fn push_multitap_actor_eases(
    out: &mut Vec<SongLuaOverlayEase>,
    frame_index: usize,
    frame_baseline: SongLuaOverlayState,
    arrow_index: usize,
    arrow_baseline: SongLuaOverlayState,
    deco_index: usize,
    deco_baseline: SongLuaOverlayState,
    deco_children: &[(usize, SongLuaOverlayState)],
    context: &SongLuaCompileContext,
    player: usize,
    noteskin_resolver: SongLuaNoteskinResolver,
    noteskin: &str,
    desc: &MultitapDesc,
    // Signature adapter; the frozen baseline only measures rotating decorations.
    _numbered: bool,
) {
    // Preserve strict visibility/tap boundaries and the derivative change at
    // each bounce apex. The smallest representable following beat keeps the
    // authored `beat > tap` edge without introducing a sampling delay.
    let mut beats = vec![multitap_visible_start(desc.taps[0])];
    for (index, &tap) in desc.taps.iter().enumerate() {
        beats.push(tap);
        beats.push(tap.next_up());
        if let Some(&next) = desc.taps.get(index + 1) {
            beats.push(tap.midpoint(next));
        }
    }
    beats.sort_by(f32::total_cmp);
    beats.dedup();
    let mut frame_samples = Vec::new();
    let mut arrow_samples = Vec::new();
    let mut deco_samples = Vec::new();
    let mut deco_child_samples = deco_children
        .iter()
        .map(|(index, _)| (*index, Vec::new()))
        .collect::<Vec<_>>();
    for beat in beats {
        let phase = calc_multitap_phase(desc, beat);
        frame_samples.push((
            beat,
            multitap_frame_state(frame_baseline, context, player, desc.lane, beat, phase),
        ));
        push_multitap_arrow_sample(
            &mut arrow_samples,
            beat,
            arrow_baseline,
            noteskin_resolver,
            noteskin,
            desc.lane,
            phase,
        );
        deco_samples.push((
            beat,
            multitap_deco_state(deco_baseline, noteskin_resolver, noteskin, phase),
        ));
        for ((_, baseline), (_, samples)) in deco_children.iter().zip(&mut deco_child_samples) {
            samples.push((
                beat,
                multitap_deco_child_state(*baseline, noteskin_resolver, noteskin, phase),
            ));
        }
    }
    let first_ease = out.len();
    push_overlay_sample_eases(out, frame_index, frame_baseline, &frame_samples);
    // Y follows a parabola; squash and the other components are piecewise
    // linear. Split only Y out of each bounce half and keep its exact curve.
    let mut parabolas = Vec::new();
    for ease in &mut out[first_ease..] {
        if ease.limit <= f32::EPSILON || ease.start <= desc.taps[0] {
            continue;
        }
        let (Some(from), Some(to)) = (ease.from.y, ease.to.y) else {
            continue;
        };
        let mut y = ease.clone();
        y.from = SongLuaOverlayStateDelta {
            y: Some(from),
            ..Default::default()
        };
        y.to = SongLuaOverlayStateDelta {
            y: Some(to),
            ..Default::default()
        };
        y.easing = Some(if to > from { "outQuad" } else { "inQuad" }.into());
        ease.from.y = None;
        ease.to.y = None;
        parabolas.push(y);
    }
    out.extend(parabolas);
    push_overlay_sample_eases(out, arrow_index, arrow_baseline, &arrow_samples);
    push_overlay_sample_eases(out, deco_index, deco_baseline, &deco_samples);
    for ((_, baseline), (child_index, samples)) in deco_children.iter().zip(deco_child_samples) {
        push_overlay_sample_eases(out, child_index, *baseline, &samples);
    }
}

pub fn multitap_deco_state(
    baseline: SongLuaOverlayState,
    noteskin_resolver: SongLuaNoteskinResolver,
    noteskin: &str,
    phase: MultitapPhase,
) -> SongLuaOverlayState {
    if !phase.visible {
        return baseline;
    }
    let (effect_color1, effect_color2) =
        multitap_deco_color_pair(noteskin_resolver, noteskin, phase.qtc);
    let mut state = baseline;
    state.visible = true;
    state.zoom = 1.0;
    state.z = 10.0;
    state.rot_z_deg = phase.lin * 180.0;
    state.effect_mode = EffectMode::DiffuseRamp;
    state.effect_clock = EffectClock::Beat;
    state.effect_color1 = effect_color1;
    state.effect_color2 = effect_color2;
    state.effect_period = 1.0;
    state
}

pub fn multitap_deco_child_state(
    baseline: SongLuaOverlayState,
    noteskin_resolver: SongLuaNoteskinResolver,
    noteskin: &str,
    phase: MultitapPhase,
) -> SongLuaOverlayState {
    if !phase.visible {
        return baseline;
    }
    let (effect_color1, effect_color2) =
        multitap_deco_color_pair(noteskin_resolver, noteskin, phase.qtc);
    let mut state = baseline;
    state.effect_mode = EffectMode::DiffuseRamp;
    state.effect_clock = EffectClock::Beat;
    state.effect_color1 = effect_color1;
    state.effect_color2 = effect_color2;
    state.effect_period = 1.0;
    state
}

pub fn multitap_deco_color_pair(
    noteskin_resolver: SongLuaNoteskinResolver,
    noteskin: &str,
    qtzn: u8,
) -> MultitapColorPair {
    if noteskin_resolver
        .metric_b(noteskin, "", "TapNoteAnimationIsVivid")
        .unwrap_or(false)
    {
        return MULTITAP_QTZN_VIVID[0];
    }
    multitap_qtzn_color_table(noteskin)[multitap_qtzn_tex(qtzn)]
}

pub fn multitap_qtzn_color_table(noteskin: &str) -> &'static [MultitapColorPair; 8] {
    let noteskin = noteskin.to_ascii_lowercase();
    if noteskin.contains("color") {
        return &MULTITAP_QTZN_COLOR;
    }
    if noteskin.contains("rainbow") || noteskin.contains("solo") {
        return &MULTITAP_QTZN_RAINBOW;
    }
    if noteskin.contains("horse") || noteskin.contains("toonprints") {
        return &MULTITAP_QTZN_HORSE;
    }
    for key in [
        "cel",
        "cyber",
        "delta",
        "ddrlike",
        "enchantment",
        "excel",
        "metal",
        "onlyonecouples",
        "scalable",
        "spotlight",
        "vel",
        "vintage",
    ] {
        if noteskin.contains(key) {
            return &MULTITAP_QTZN_SHADOW;
        }
    }
    for key in [
        "ascii", "default", "easy", "exact", "lambda", "note", "retro", "trax",
    ] {
        if noteskin.contains(key) {
            return &MULTITAP_QTZN_NOTE;
        }
    }
    &MULTITAP_QTZN_VIVID
}

pub fn split_multitap_y_eases(
    out: &mut Vec<SongLuaOverlayEase>,
    first_ease: usize,
    first_tap: f32,
) {
    let mut parabolas = Vec::new();
    for ease in &mut out[first_ease..] {
        if ease.limit <= f32::EPSILON || ease.start <= first_tap {
            continue;
        }
        let (Some(from), Some(to)) = (ease.from.y, ease.to.y) else {
            continue;
        };
        let mut y = ease.clone();
        y.from = SongLuaOverlayStateDelta {
            y: Some(from),
            ..Default::default()
        };
        y.to = SongLuaOverlayStateDelta {
            y: Some(to),
            ..Default::default()
        };
        y.easing = Some(if to > from { "outQuad" } else { "inQuad" }.into());
        ease.from.y = None;
        ease.to.y = None;
        parabolas.push(y);
    }
    out.extend(parabolas);
}
