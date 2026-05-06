use super::state::PlayerOptionMasks;
use super::*;
use crate::game::profile::{PlayerSide, Profile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum RowId {
    TypeOfSpeedMod,
    SpeedMod,
    Mini,
    Spacing,
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
    JudgmentTiltMinThreshold,
    JudgmentTiltMaxThreshold,
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
    FAPlusWindowOptions,
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
    /// Opt-in init contract. When `Some`, the row's initial cursor position is
    /// derived directly from a `Profile` via `init_numeric_row_from_binding`.
    /// `None` means the row's selection is initialized elsewhere (today: a
    /// hand-written block in `apply_profile_defaults`).
    pub init: Option<NumericInit>,
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
    /// Opt-in init contract. When `Some`, the row's initial cursor position is
    /// derived directly from a `Profile` via `init_cycle_row_from_binding`.
    /// `None` means the row's selection is initialized elsewhere (today: a
    /// hand-written block in `apply_profile_defaults`).
    pub init: Option<CycleInit>,
}

/// # Adding a new mask row
///
/// 1. Add the bitflags type to `state.rs` (or use one from `game::profile`)
///    and a field on `PlayerOptionMasks`.
/// 2. Build the row in the appropriate pane's `build_*_rows` (or its row
///    catalogue) with `behavior: RowBehavior::Bitmask(MY_BINDING)`.
/// 3. Declare `const MY_BINDING: BitmaskBinding` in that pane's module.
///    Pick one of two variants:
///
///    a. `BitmaskBinding::Generic { init, writeback }` — preferred for
///    "clean" rows that project a single mask field onto the profile
///    and persist for one side with no fan-out, no derived recomputes,
///    and no visibility sync. The generic `toggle_bitmask_row_generic`
///    in `choice.rs` drives input; `writeback.bit_mapping` declares
///    how choice indices map to bits.
///
///    b. `BitmaskBinding::HandRolled { init, toggle }` — for rows whose
///    toggle fans out to multiple profile fields, recomputes derived
///    state, or calls `sync_selected_rows_with_visibility`. `toggle`
///    is a hand-rolled `toggle_*_row` fn in `choice.rs`. `init` is
///    optional; production bindings always supply it, and `None` is
///    reserved for synthetic bindings in tests.
///
/// 4. `BitmaskInit` shape:
///    - `from_profile` reads the relevant profile fields and emits
///      `mask.bits() as u32`.
///    - `set_active` uses `from_bits_retain` plus a `debug_assert_eq!`
///      width check (so unknown bits in profile-sourced masks are
///      preserved, matching legacy direct-assignment semantics).
///    - `cursor: CursorInit::FirstActiveBit` for normal rows, or
///      `CursorInit::Fixed(0)` for pinned-cursor rows like FA+ Options.
#[derive(Clone, Copy, Debug)]
pub enum BitmaskBinding {
    /// Generic, fully-declarative bitmask binding. The generic toggle
    /// dispatcher uses `init` + `writeback` to flip a bit and persist it
    /// without any per-row code. `Row::bitmask` debug-asserts that
    /// `choices.len() == writeback.bit_mapping.required_choices()`.
    Generic {
        init: BitmaskInit,
        writeback: BitmaskWriteback,
    },
    /// Hand-rolled bitmask binding. The dispatcher invokes `toggle`
    /// directly. `init` is optional but expected for production rows so
    /// that `init_bitmask_row_from_binding` can derive the initial cursor
    /// and mask bits from the player's profile.
    HandRolled {
        init: Option<BitmaskInit>,
        toggle: fn(&mut State, usize),
    },
}

impl BitmaskBinding {
    /// Borrow the optional `BitmaskInit`, regardless of variant.
    /// `Generic` always has init; `HandRolled` may or may not.
    #[inline]
    pub fn init(&self) -> Option<&BitmaskInit> {
        match self {
            BitmaskBinding::Generic { init, .. } => Some(init),
            BitmaskBinding::HandRolled { init, .. } => init.as_ref(),
        }
    }
}

/// Declarative writeback contract for a `BitmaskBinding::Generic`.
/// Together with `BitmaskInit`, this lets the generic toggle fully replace
/// a hand-rolled `toggle_*_row` for "clean" bitmask rows that have no
/// fan-out and no visibility-sync side effects.
#[derive(Clone, Copy, Debug)]
pub struct BitmaskWriteback {
    /// Project the row's bits onto the in-memory profile. Implementations
    /// typically reconstruct the typed mask via `from_bits_truncate`.
    pub project_to_profile: fn(&mut Profile, u32),
    /// Persist the row's bits for the given side. Called only when
    /// `persist_ctx` says the active player_idx should write through.
    pub persist_for_side: fn(PlayerSide, u32),
    /// Declarative choice-index-to-bit mapping. The generic toggle
    /// resolves the focused row's selected choice index through this
    /// mapping; out-of-range indices yield `None` and produce a no-op.
    pub bit_mapping: BitMapping,
}

/// Declarative mapping from a row's choice index to the bit to toggle.
///
/// Replaces an earlier `fn(usize, &Row) -> Option<u32>` callback so the
/// bit width of a row is visible at the binding declaration site rather
/// than hidden behind code. `Row::bitmask` debug-asserts that the row's
/// `choices.len()` matches the mapping's `required_choices()`.
#[derive(Clone, Copy, Debug)]
pub enum BitMapping {
    /// `choice_index i` maps to `1 << i` for `i < width`. Use for rows
    /// where each choice corresponds to bit `i` of the mask.
    Sequential { width: u8 },
    /// `choice_index i` maps to `1 << (offset + i)` for `i < width`. Use
    /// for child rows that share a mask with a parent and expose a
    /// contiguous sub-range of bits.
    #[allow(dead_code)]
    SequentialOffset { offset: u8, width: u8 },
    /// `choice_index i` maps to `bits[i]`. Use for rows whose choices
    /// don't correspond to a contiguous bit range.
    #[allow(dead_code)]
    Explicit(&'static [u32]),
}

impl BitMapping {
    /// Resolve the bit (as u32) for the given row choice index, or `None`
    /// if the index is out of range. Returning `None` (or `Some(0)`)
    /// causes the generic toggle to no-op.
    #[inline]
    pub fn bit_for_choice(self, choice_index: usize) -> Option<u32> {
        match self {
            BitMapping::Sequential { width } => {
                if choice_index < width as usize {
                    Some(1u32 << choice_index)
                } else {
                    None
                }
            }
            BitMapping::SequentialOffset { offset, width } => {
                if choice_index < width as usize {
                    Some(1u32 << (offset as usize + choice_index))
                } else {
                    None
                }
            }
            BitMapping::Explicit(bits) => bits.get(choice_index).copied(),
        }
    }

    /// The number of choices a row using this mapping must declare.
    /// Enforced by a debug assertion at `Row::bitmask` construction so
    /// "row has more choices than the mask exposes" cannot silently
    /// drop bits, and "row has fewer choices than the mask exposes"
    /// cannot silently leave bits unreachable.
    #[inline]
    pub fn required_choices(self) -> usize {
        match self {
            BitMapping::Sequential { width } => width as usize,
            BitMapping::SequentialOffset { width, .. } => width as usize,
            BitMapping::Explicit(bits) => bits.len(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BitmaskInit {
    /// Compute the row's initial bits from the player's profile. Returned
    /// as `u32` for type erasure across the 17 different mask widths.
    pub from_profile: fn(&Profile) -> u32,
    /// Read the row's current bits from a `PlayerOptionMasks`. Used to
    /// compute the cursor index from the *stored* (post-`set_active`) value.
    pub get_active: fn(&PlayerOptionMasks) -> u32,
    /// Write the row's bits into a `PlayerOptionMasks`. Bindings should use
    /// `from_bits_retain` (not `from_bits_truncate`) so unknown bits in
    /// profile-sourced masks are preserved — matching the legacy
    /// direct-assignment behaviour (`masks.x = profile.x`).
    pub set_active: fn(&mut PlayerOptionMasks, u32),
    /// Cursor placement policy at init time.
    pub cursor: CursorInit,
}

#[derive(Clone, Copy, Debug)]
pub enum CursorInit {
    /// Cursor lands on the first set bit, or `0` if no bits are set. Used
    /// by every mask row except pinned rows like `FAPlusOptions`.
    FirstActiveBit,
    /// Cursor is pinned to a fixed index regardless of which bits are
    /// active. Used by `FAPlusOptions` (always 0).
    Fixed(usize),
}

impl BitmaskInit {
    /// Compute the cursor index for a row of `choices_len` choices given
    /// its currently-active bits (as `u32`).
    #[inline]
    pub fn init_cursor_index(&self, active_bits: u32, choices_len: usize) -> usize {
        match self.cursor {
            CursorInit::Fixed(idx) => idx,
            CursorInit::FirstActiveBit => {
                if active_bits == 0 {
                    0
                } else {
                    (0..choices_len)
                        .find(|i| (active_bits & (1u32 << *i as u32)) != 0)
                        .unwrap_or(0)
                }
            }
        }
    }
}

/// Apply a `BitmaskBinding`'s init contract to a row: compute the bits
/// from the profile, write them into `masks`, then place the row's cursor
/// based on the bits as **read back from `masks`** via `get_active` (so a
/// binding's `set_active` semantics — including any masking applied by
/// `from_bits_retain` — are reflected in cursor placement). Returns
/// `true` when the binding had an `init` contract and was applied;
/// `false` when the binding has no init (a synthetic test binding or a
/// future row that has not yet been wired).
pub fn init_bitmask_row_from_binding(
    row: &mut Row,
    binding: &BitmaskBinding,
    profile: &Profile,
    masks: &mut PlayerOptionMasks,
    player_idx: usize,
) -> bool {
    let Some(init) = binding.init() else {
        return false;
    };
    let bits = (init.from_profile)(profile);
    (init.set_active)(masks, bits);
    let stored = (init.get_active)(masks);
    row.selected_choice_index[player_idx] = init.init_cursor_index(stored, row.choices.len());
    true
}

/// Opt-in init contract for a `CycleBinding` row. The function returns the
/// initial cursor index for a row given the current `Profile`. The helper
/// `init_cycle_row_from_binding` clamps the returned index to
/// `row.choices.len() - 1`, so implementations can return a raw
/// `position(...).unwrap_or(0)` without separate clamping.
///
/// **Scope:** only `CycleBinding::Bool` and `CycleBinding::Index` rows.
/// `CustomBinding` rows whose init logic depends on translated strings or
/// runtime asset lookups (e.g. `NoteSkin`, `JudgmentFont`, `MineSkin`,
/// `ReceptorSkin`, `TapExplosionSkin`, `HoldJudgment`) are intentionally not
/// covered by this contract; they continue to be initialized in
/// `apply_profile_defaults`.
#[derive(Clone, Copy, Debug)]
pub struct CycleInit {
    pub from_profile: fn(&Profile) -> usize,
}

/// Opt-in init contract for a `NumericBinding` row. `from_profile` reads the
/// row's `i32` value from the profile; `format` renders that value the same
/// way the row's `choices` were generated (e.g. `|v| format!("{v}%")`,
/// `|v| format!("{v}ms")`, `|v| v.to_string()`), so the rendered string can
/// be looked up in `Row::choices`.
///
/// **Scope:** only `NumericBinding` rows. Numeric profile fields whose row
/// does not exist (or whose init depends on runtime asset state) remain in
/// `apply_profile_defaults`.
#[derive(Clone, Copy, Debug)]
pub struct NumericInit {
    pub from_profile: fn(&Profile) -> i32,
    pub format: fn(i32) -> String,
}

/// Apply a `ChoiceBinding`'s init contract to a row: compute the desired
/// cursor index from the profile and clamp it to the row's choices length.
/// Returns `true` when the binding had an `init` contract and was applied;
/// `false` when the binding has no init.
pub fn init_cycle_row_from_binding<T: Copy + 'static>(
    row: &mut Row,
    binding: &ChoiceBinding<T>,
    profile: &Profile,
    player_idx: usize,
) -> bool {
    let Some(init) = binding.init.as_ref() else {
        return false;
    };
    let max = row.choices.len().saturating_sub(1);
    row.selected_choice_index[player_idx] = (init.from_profile)(profile).min(max);
    true
}

/// Apply a `NumericBinding`'s init contract to a row: read the profile value,
/// format it via `init.format`, and place the cursor on the matching entry in
/// `Row::choices`. If no entry matches the formatted value, the row's
/// existing selection is left unchanged — this matches the legacy behaviour
/// of `apply_profile_defaults` for numeric rows (e.g. `Mini`, `Spacing`,
/// `NoteFieldOffsetX/Y`, `JudgmentOffsetX/Y`), which all do
/// `if let Some(idx) = row.choices.iter().position(...) { row.selected_choice_index[idx] = ... }`.
/// Returns `true` when the binding had an `init` contract and was applied
/// (even if the format produced no match); `false` when the binding has no
/// init.
pub fn init_numeric_row_from_binding(
    row: &mut Row,
    binding: &NumericBinding,
    profile: &Profile,
    player_idx: usize,
) -> bool {
    let Some(init) = binding.init.as_ref() else {
        return false;
    };
    let needle = (init.format)((init.from_profile)(profile));
    if let Some(idx) = row.choices.iter().position(|c| c == &needle) {
        row.selected_choice_index[player_idx] = idx;
    }
    true
}

#[derive(Clone, Copy, Debug)]
pub struct CustomBinding {
    pub apply: fn(&mut State, usize, RowId, isize, NavWrap) -> Outcome,
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
        index_binding!($table, $default, $field, $persist, $vis, None)
    };
    ($table:expr, $default:expr, $field:ident, $persist:expr, $vis:expr, $init:expr) => {
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
            init: $init,
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

/// Expand a help `LookupKey` into the pre-split `Vec<String>` shape that
/// `Row::help` expects.
#[inline]
pub(super) fn expand_help(help: LookupKey) -> Vec<String> {
    help.get().split("\\n").map(|s| s.to_string()).collect()
}

impl Row {
    /// Construct a `RowBehavior::Numeric` row with the standard defaults
    /// (`selected_choice_index = [0; PLAYER_SLOTS]`,
    /// `choice_difficulty_indices = None`, `mirror_across_players = false`).
    /// Override defaults via the chain methods below.
    pub fn numeric(
        id: RowId,
        name: LookupKey,
        help: LookupKey,
        binding: NumericBinding,
        choices: Vec<String>,
    ) -> Self {
        Self::base(id, RowBehavior::Numeric(binding), name, help, choices)
    }

    /// Construct a `RowBehavior::Cycle` row.
    pub fn cycle(
        id: RowId,
        name: LookupKey,
        help: LookupKey,
        binding: CycleBinding,
        choices: Vec<String>,
    ) -> Self {
        Self::base(id, RowBehavior::Cycle(binding), name, help, choices)
    }

    /// Construct a `RowBehavior::Bitmask` row.
    ///
    /// For `BitmaskBinding::Generic` bindings, debug-asserts that the
    /// number of choices matches the writeback's bit-mapping width. This
    /// catches drift between the declared bit width and the choice list
    /// at row-construction time rather than silently dropping bits at
    /// toggle time.
    pub fn bitmask(
        id: RowId,
        name: LookupKey,
        help: LookupKey,
        binding: BitmaskBinding,
        choices: Vec<String>,
    ) -> Self {
        if let BitmaskBinding::Generic { writeback, .. } = &binding {
            let required = writeback.bit_mapping.required_choices();
            debug_assert_eq!(
                choices.len(),
                required,
                "bitmask row {:?}: choices.len()={} but bit_mapping requires {}",
                id,
                choices.len(),
                required,
            );
        }
        Self::base(id, RowBehavior::Bitmask(binding), name, help, choices)
    }

    /// Construct a `RowBehavior::Custom` row. See the `CustomBinding` shape
    /// in `row.rs` for the apply-fn signature.
    pub fn custom(
        id: RowId,
        name: LookupKey,
        help: LookupKey,
        binding: CustomBinding,
        choices: Vec<String>,
    ) -> Self {
        Self::base(id, RowBehavior::Custom(binding), name, help, choices)
    }

    /// Construct an Exit row. All three pane Exit rows are byte-identical;
    /// this no-arg constructor centralizes the boilerplate.
    pub fn exit() -> Self {
        Self::base(
            RowId::Exit,
            RowBehavior::Exit,
            lookup_key("Common", "Exit"),
            // Exit rows historically have an empty help line, not a
            // translated string. Preserve that by skipping `expand_help`.
            lookup_key("Common", "Exit"),
            vec![tr("Common", "Exit").to_string()],
        )
        .with_help_lines(vec![String::new()])
    }

    /// Set every slot's initial cursor to the same index. Used when a row
    /// has a meaningful "default position" (e.g. the zero offset for HUD
    /// offset rows).
    pub fn with_initial_choice_index(mut self, idx: usize) -> Self {
        self.selected_choice_index = [idx; PLAYER_SLOTS];
        self
    }

    /// Set per-player initial cursor positions. Used by Stepchart, where
    /// each player's initial difficulty selection is independent.
    pub fn with_initial_choice_indices(mut self, idxs: [usize; PLAYER_SLOTS]) -> Self {
        self.selected_choice_index = idxs;
        self
    }

    /// Attach a `choice_difficulty_indices` lookup table. Currently used
    /// only by Stepchart to map UI choices back to underlying difficulty
    /// indices.
    pub fn with_choice_difficulty_indices(mut self, idxs: Vec<usize>) -> Self {
        self.choice_difficulty_indices = Some(idxs);
        self
    }

    /// Mark the row as mirrored across all player slots. Used by
    /// `WhatComesNext` so a change on one player propagates to all.
    pub fn with_mirror_across_players(mut self) -> Self {
        self.mirror_across_players = true;
        self
    }

    /// Escape hatch for rows whose help text is not a translated string
    /// (currently only the Exit row's empty placeholder line). Prefer the
    /// `help: LookupKey` parameter on the public constructors.
    fn with_help_lines(mut self, lines: Vec<String>) -> Self {
        self.help = lines;
        self
    }

    fn base(
        id: RowId,
        behavior: RowBehavior,
        name: LookupKey,
        help: LookupKey,
        choices: Vec<String>,
    ) -> Self {
        Self {
            id,
            behavior,
            name,
            choices,
            selected_choice_index: [0; PLAYER_SLOTS],
            help: expand_help(help),
            choice_difficulty_indices: None,
            mirror_across_players: false,
        }
    }
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
        || id == RowId::FAPlusWindowOptions
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
