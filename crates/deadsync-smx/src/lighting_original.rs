const BYTES_PER_PAD_25: usize = 675;
const LIGHT_BRIGHTNESS_GAMMA: f32 = 1.8;

const LIGHT_BRIGHTNESS_MIN_FACTOR: f32 = 0.012;

const PLAYER_LIGHT_FRAME_BYTES: usize = 2 * BYTES_PER_PAD_25;

fn brightness_factor(pct: u8) -> f32 {
    let pct = pct.min(100);
    if pct == 0 {
        return 0.0;
    }
    let curve = (f32::from(pct) / 100.0).powf(LIGHT_BRIGHTNESS_GAMMA);
    (1.0 - LIGHT_BRIGHTNESS_MIN_FACTOR).mul_add(curve, LIGHT_BRIGHTNESS_MIN_FACTOR)
}

pub(crate) fn apply_brightness(frame: &mut [u8], pct: [u8; 2]) {
    let half = frame.len() / 2;
    let factor = [brightness_factor(pct[0]), brightness_factor(pct[1])];
    for (i, b) in frame.iter_mut().enumerate() {
        let f = factor[usize::from(i >= half)];
        *b = f32::from(*b).mul_add(f, 0.5) as u8;
    }
}

pub(super) struct BrightnessScale {
    pub(super) pct: [u8; 2],
    pub(super) table: [[u8; 256]; 2],
}

impl BrightnessScale {
    const UNSET: u8 = u8::MAX;

    pub(super) const fn new() -> Self {
        Self {
            pct: [Self::UNSET; 2],
            table: [[0; 256]; 2],
        }
    }

    pub(super) fn apply(&mut self, frame: &mut [u8], pct: [u8; 2]) {
        let half = frame.len() / 2;
        for slot in 0..2 {
            let pct = pct[slot].min(100);
            if self.pct[slot] != pct {
                let factor = brightness_factor(pct);
                for (value, scaled) in self.table[slot].iter_mut().enumerate() {
                    *scaled = (value as f32).mul_add(factor, 0.5) as u8;
                }
                self.pct[slot] = pct;
            }
            let range = slot * half..(slot + 1) * half;
            for value in &mut frame[range] {
                *value = self.table[slot][usize::from(*value)];
            }
        }
    }
}

pub(super) fn player_light_frame(
    colors: [Option<[u8; 3]>; 2],
    brightness: [u8; 2],
) -> [u8; PLAYER_LIGHT_FRAME_BYTES] {
    let mut frame = [0; PLAYER_LIGHT_FRAME_BYTES];
    for (pad, color) in colors.iter().enumerate() {
        let Some(rgb) = color else { continue };
        let base = pad * BYTES_PER_PAD_25;
        for led in frame[base..base + BYTES_PER_PAD_25].as_chunks_mut::<3>().0 {
            *led = *rgb;
        }
    }
    if brightness != [100, 100] {
        apply_brightness(&mut frame, brightness);
    }
    frame
}
pub mod panels {
    pub const PADS: usize = 2;
    const DIR_TO_PANEL: [usize; 4] = [3, 7, 1, 5];
    const PUMP_TO_PANEL: [usize; 5] = [6, 0, 4, 2, 8];
    pub fn smx_panel_for_col(
        cols_per_player: usize,
        num_players: usize,
        column: usize,
    ) -> Option<(usize, usize)> {
        if cols_per_player == 0 || column >= cols_per_player.saturating_mul(num_players) {
            return None;
        }
        let local = column % cols_per_player;
        let pump = matches!(cols_per_player, 5 | 10);
        let lanes_per_pad = if pump { 5 } else { 4 };
        let (pad, local_col) = if cols_per_player > lanes_per_pad {
            // A field spanning both pads (doubles, or each Routine participant):
            // split at the native pad lane count.
            (local / lanes_per_pad, local % lanes_per_pad)
        } else {
            let pad = column / cols_per_player;
            if pad >= PADS {
                return None;
            }
            (pad, local)
        };
        let panel = if pump {
            PUMP_TO_PANEL.get(local_col)
        } else {
            DIR_TO_PANEL.get(local_col)
        }?;
        Some((pad, *panel))
    }
}
