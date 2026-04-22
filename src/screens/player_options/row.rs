use super::*;
use crate::game::profile::{PlayerSide, Profile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum RowId {
    TypeOfSpeedMod,
    SpeedMod,
    Mini,
    Perspective,
    NoteSkin,
    MineSkin,
    ReceptorSkin,
    TapExplosionSkin,
    JudgmentFont,
    JudgmentOffsetX,
    JudgmentOffsetY,
    ComboFont,
    ComboOffsetX,
    ComboOffsetY,
    HoldJudgment,
    BackgroundFilter,
    NoteFieldOffsetX,
    NoteFieldOffsetY,
    VisualDelay,
    GlobalOffsetShift,
    MusicRate,
    Stepchart,
    WhatComesNext,
    Exit,
    // Advanced pane
    Turn,
    Scroll,
    Hide,
    LifeMeterType,
    LifeBarOptions,
    DataVisualizations,
    DensityGraphBackground,
    TargetScore,
    ActionOnMissedTarget,
    MiniIndicator,
    IndicatorScoreType,
    GameplayExtras,
    ComboColors,
    ComboColorMode,
    CarryCombo,
    JudgmentTilt,
    JudgmentTiltIntensity,
    JudgmentBehindArrows,
    OffsetIndicator,
    ErrorBar,
    ErrorBarTrim,
    ErrorBarOptions,
    ErrorBarOffsetX,
    ErrorBarOffsetY,
    MeasureCounter,
    MeasureCounterLookahead,
    MeasureCounterOptions,
    MeasureLines,
    RescoreEarlyHits,
    EarlyDecentWayOffOptions,
    ResultsExtras,
    TimingWindows,
    FAPlusOptions,
    CustomBlueFantasticWindow,
    CustomBlueFantasticWindowMs,
    // Uncommon pane
    Insert,
    Remove,
    Holds,
    Accel,
    Effect,
    Appearance,
    Attacks,
    HideLightType,
    GameplayExtrasMore,
}

impl RowId {
    pub(super) const COUNT: usize = Self::GameplayExtrasMore as usize + 1;

    #[inline(always)]
    pub(super) const fn index(self) -> usize {
        self as usize
    }
}

// ================================ RowBehavior types ================================

/// Result of a row's reaction to a key press.
///
/// Kept tiny so every dispatcher arm can return one without ceremony. The
/// shared dispatcher reads it to decide whether to play the change-value SFX
/// and whether to re-run visibility sync.
#[derive(Clone, Copy, Debug, Default)]
pub struct Outcome {
    pub persisted: bool,
    pub changed_visibility: bool,
}

impl Outcome {
    pub const NONE: Self = Self {
        persisted: false,
        changed_visibility: false,
    };

    #[inline(always)]
    pub const fn persisted() -> Self {
        Self {
            persisted: true,
            changed_visibility: false,
        }
    }

    #[inline(always)]
    pub const fn persisted_with_visibility() -> Self {
        Self {
            persisted: true,
            changed_visibility: true,
        }
    }
}

/// Static behaviour for a numeric row whose `Row::choices` already encode
/// every legal value as a string.
#[derive(Clone, Copy, Debug)]
pub struct NumericBinding {
    pub parse: fn(&str) -> Option<i32>,
    pub apply: fn(&mut Profile, i32) -> Outcome,
    pub persist_for_side: fn(PlayerSide, i32),
}

/// How a cycle row writes its currently selected index back to the persisted
/// player profile.
#[derive(Clone, Copy, Debug)]
pub enum CycleBinding {
    Bool(ChoiceBinding<bool>),
    Index(ChoiceBinding<usize>),
}

/// A typed cycle binding. `apply` writes the new value into the profile and
/// reports back via `Outcome` whether the change should also trigger a
/// visibility re-sync. The dispatcher reads that outcome and acts on it
/// uniformly across all binding types.
#[derive(Clone, Copy, Debug)]
pub struct ChoiceBinding<T: Copy + 'static> {
    pub apply: fn(&mut Profile, T) -> Outcome,
    pub persist_for_side: fn(PlayerSide, T),
}

#[derive(Clone, Copy, Debug)]
pub struct BitmaskBinding {
    pub toggle: fn(&mut State, usize),
}

#[derive(Clone, Copy, Debug)]
pub struct CustomBinding {
    pub apply: fn(&mut State, usize, RowId, isize) -> Outcome,
}

/// What kind of row this is, and any state owned by the row's behaviour.
#[derive(Clone, Copy, Debug)]
pub enum RowBehavior {
    Numeric(NumericBinding),
    Cycle(CycleBinding),
    Bitmask(BitmaskBinding),
    Exit,
    Custom(CustomBinding),
}

// ============================== Helpers ================================

#[inline]
pub(super) fn parse_i32(s: &str) -> Option<i32> {
    s.parse::<i32>().ok()
}

#[inline]
pub(super) fn parse_i32_ms(s: &str) -> Option<i32> {
    s.trim_end_matches("ms").parse::<i32>().ok()
}

#[inline]
pub(super) fn parse_i32_percent(s: &str) -> Option<i32> {
    s.trim_end_matches('%').parse::<i32>().ok()
}

/// Build a `ChoiceBinding<usize>` for a row whose choices map 1:1 to a static
/// `[Enum; N]` variant table. Cuts per-binding boilerplate down to its data.
macro_rules! index_binding {
    ($table:expr, $default:expr, $field:ident, $persist:expr, $vis:expr) => {
        $crate::screens::player_options::row::ChoiceBinding::<usize> {
            apply: |p, i| {
                p.$field = $table.get(i).copied().unwrap_or($default);
                if $vis {
                    $crate::screens::player_options::row::Outcome::persisted_with_visibility()
                } else {
                    $crate::screens::player_options::row::Outcome::persisted()
                }
            },
            persist_for_side: |s, i| $persist(s, $table.get(i).copied().unwrap_or($default)),
        }
    };
}

pub(crate) use index_binding;

// ============================== RowMap =================================

pub struct RowMap {
    pub(super) rows: [Option<Row>; RowId::COUNT],
    pub(super) display_order: Vec<RowId>,
}

impl RowMap {
    pub(super) fn new() -> Self {
        Self {
            rows: std::array::from_fn(|_| None),
            display_order: Vec::new(),
        }
    }

    #[inline(always)]
    pub fn get(&self, id: RowId) -> Option<&Row> {
        self.rows[id.index()].as_ref()
    }

    #[inline(always)]
    pub fn get_mut(&mut self, id: RowId) -> Option<&mut Row> {
        self.rows[id.index()].as_mut()
    }

    /// Panicking accessor for rows known to exist in the current pane.
    #[inline(always)]
    pub fn row(&self, id: RowId) -> &Row {
        self.rows[id.index()].as_ref().expect("row must exist")
    }

    pub(super) fn insert(&mut self, row: Row) {
        let idx = row.id.index();
        debug_assert!(self.rows[idx].is_none(), "duplicate RowId {:?}", row.id);
        self.rows[idx] = Some(row);
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.display_order.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.display_order.is_empty()
    }

    #[inline(always)]
    pub fn display_order(&self) -> &[RowId] {
        &self.display_order
    }

    /// Get the RowId at the given display index.
    #[inline(always)]
    pub fn id_at(&self, display_idx: usize) -> RowId {
        self.display_order[display_idx]
    }

    /// Safe access by display index.
    #[inline(always)]
    pub fn get_at(&self, display_idx: usize) -> Option<&Row> {
        self.display_order
            .get(display_idx)
            .and_then(|&id| self.get(id))
    }
}

pub(super) struct RowBuilder {
    pub(super) map: RowMap,
    pub(super) order: Vec<RowId>,
}

impl RowBuilder {
    pub(super) fn new() -> Self {
        Self {
            map: RowMap::new(),
            order: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, row: Row) {
        let id = row.id;
        self.map.insert(row);
        self.order.push(id);
    }

    pub(super) fn finish(self) -> RowMap {
        let mut map = self.map;
        map.display_order = self.order;
        map
    }
}

pub struct Row {
    pub id: RowId,
    pub behavior: RowBehavior,
    pub name: LookupKey,
    pub choices: Vec<String>,
    pub selected_choice_index: [usize; PLAYER_SLOTS],
    pub help: Vec<String>,
    pub choice_difficulty_indices: Option<Vec<usize>>,
    /// When `true`, after a delta apply that persisted the row, the
    /// dispatcher copies `selected_choice_index[player_idx]` to every other
    /// slot. Also consulted by inline-nav focus commit. Use for rows whose
    /// state is conceptually shared across players (e.g. `WhatComesNext`).
    pub mirror_across_players: bool,
}

#[derive(Clone, Debug)]
pub struct FixedStepchart {
    pub label: String,
}

#[inline(always)]
pub(super) fn row_shows_all_choices_inline(id: RowId) -> bool {
    id == RowId::Perspective
        || id == RowId::Stepchart
        || id == RowId::WhatComesNext
        || id == RowId::ActionOnMissedTarget
        || id == RowId::ErrorBar
        || id == RowId::ErrorBarTrim
        || id == RowId::ErrorBarOptions
        || id == RowId::OffsetIndicator
        || id == RowId::JudgmentBehindArrows
        || id == RowId::MeasureCounter
        || id == RowId::MeasureCounterLookahead
        || id == RowId::MeasureCounterOptions
        || id == RowId::MeasureLines
        || id == RowId::TimingWindows
        || id == RowId::JudgmentTilt
        || id == RowId::MiniIndicator
        || id == RowId::IndicatorScoreType
        || id == RowId::Turn
        || id == RowId::Scroll
        || id == RowId::Hide
        || id == RowId::LifeMeterType
        || id == RowId::LifeBarOptions
        || id == RowId::DataVisualizations
        || id == RowId::DensityGraphBackground
        || id == RowId::ComboColors
        || id == RowId::ComboColorMode
        || id == RowId::CarryCombo
        || id == RowId::GameplayExtras
        || id == RowId::GameplayExtrasMore
        || id == RowId::ResultsExtras
        || id == RowId::RescoreEarlyHits
        || id == RowId::CustomBlueFantasticWindow
        || id == RowId::EarlyDecentWayOffOptions
        || id == RowId::FAPlusOptions
        || id == RowId::Insert
        || id == RowId::Remove
        || id == RowId::Holds
        || id == RowId::Accel
        || id == RowId::Effect
        || id == RowId::Appearance
        || id == RowId::Attacks
        || id == RowId::HideLightType
}

#[inline(always)]
pub(super) fn row_supports_inline_nav(row: &Row) -> bool {
    !row.choices.is_empty() && row_shows_all_choices_inline(row.id)
}

#[inline(always)]
pub(super) fn row_toggles_with_start(row: &Row) -> bool {
    matches!(row.behavior, RowBehavior::Bitmask(_))
}

#[inline(always)]
pub(super) fn row_selects_on_focus_move(id: RowId) -> bool {
    id == RowId::Stepchart
}
