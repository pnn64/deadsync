/// Work requested by a concrete theme after handling input or updating a
/// screen.
///
/// `S` is the concrete theme's screen identity and `R` is its runtime request
/// payload. Keeping both generic lets themes define different screen graphs
/// and optional runtime capabilities without expanding this contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeEffect<S, R> {
    None,
    /// Execute multiple effects in order. Construct batches through
    /// [`ThemeEffect::batch`] or [`ThemeEffect::sequence`] so this payload stays
    /// flat. Runtime owners must route each effect normally so redirects
    /// observe the current state.
    Batch(Vec<Self>),
    Navigate(S),
    /// Navigate immediately without the current screen's out-transition.
    NavigateNoFade(S),
    Exit,
    Shutdown,
    Runtime(R),
}

/// Result of theme-owned raw input handling.
///
/// Consumption controls whether the shell continues mapping the same physical
/// edge. Scheduled work remains an independent effect sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeInputResult<S, R> {
    pub consumed: bool,
    pub effect: ThemeEffect<S, R>,
}

impl<S, R> ThemeInputResult<S, R> {
    #[inline(always)]
    #[must_use]
    pub const fn ignored() -> Self {
        Self {
            consumed: false,
            effect: ThemeEffect::None,
        }
    }

    #[inline(always)]
    pub const fn consumed(effect: ThemeEffect<S, R>) -> Self {
        Self {
            consumed: true,
            effect,
        }
    }
}

/// The flow-only subset of [`ThemeEffect`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeFlowEvent<S> {
    Navigate(S),
    NavigateNoFade(S),
    Exit,
    Shutdown,
}

impl<S, R> ThemeEffect<S, R> {
    /// Append this effect to a caller-owned flat handoff buffer.
    ///
    /// This is the migration boundary for producers that still return owned
    /// batches. Direct producers should push their atomic effects into the
    /// caller's buffer instead.
    pub fn append_to(self, effects: &mut Vec<Self>) {
        self.append_flat(effects);
    }

    /// Collapse an ordered effect list into the canonical flat representation.
    ///
    /// Empty and singleton lists do not retain a batch allocation. Nested
    /// batches and no-op effects are flattened only when present, leaving the
    /// common already-flat vector untouched.
    #[must_use]
    pub fn batch(effects: Vec<Self>) -> Self {
        if effects
            .iter()
            .all(|effect| !matches!(effect, Self::None | Self::Batch(_)))
        {
            return Self::from_flat(effects);
        }

        let mut flat = Vec::with_capacity(effects.len());
        for effect in effects {
            effect.append_flat(&mut flat);
        }
        Self::from_flat(flat)
    }

    /// Preserve source order while joining two possibly empty effect results.
    pub fn sequence(first: Self, second: Self) -> Self {
        match (first, second) {
            (Self::None, Self::Batch(effects)) | (Self::Batch(effects), Self::None) => {
                Self::batch(effects)
            }
            (Self::None, second) => second,
            (first, Self::None) => first,
            (Self::Batch(mut first), Self::Batch(mut second)) => {
                first.append(&mut second);
                Self::batch(first)
            }
            (Self::Batch(mut effects), second) => {
                effects.push(second);
                Self::batch(effects)
            }
            (first, Self::Batch(mut effects)) => {
                effects.insert(0, first);
                Self::batch(effects)
            }
            (first, second) => Self::Batch(vec![first, second]),
        }
    }

    fn append_flat(self, flat: &mut Vec<Self>) {
        match self {
            Self::None => {}
            Self::Batch(effects) => {
                for effect in effects {
                    effect.append_flat(flat);
                }
            }
            effect => flat.push(effect),
        }
    }

    fn from_flat(mut effects: Vec<Self>) -> Self {
        match effects.len() {
            0 => Self::None,
            1 => effects.pop().expect("one flat theme effect"),
            _ => Self::Batch(effects),
        }
    }
}

impl<S: Copy, R> ThemeEffect<S, R> {
    /// Return the effect as a flow event, or `None` for non-flow effects.
    #[inline(always)]
    pub const fn flow_event(&self) -> Option<ThemeFlowEvent<S>> {
        match self {
            Self::Navigate(screen) => Some(ThemeFlowEvent::Navigate(*screen)),
            Self::NavigateNoFade(screen) => Some(ThemeFlowEvent::NavigateNoFade(*screen)),
            Self::Exit => Some(ThemeFlowEvent::Exit),
            Self::Shutdown => Some(ThemeFlowEvent::Shutdown),
            Self::None | Self::Batch(_) | Self::Runtime(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ThemeEffect, ThemeFlowEvent, ThemeInputResult};
    use crate::ThemeScreenId;

    const MENU: ThemeScreenId = ThemeScreenId::new("menu");

    #[test]
    fn extracts_only_flow_effects() {
        let navigate: ThemeEffect<_, ()> = ThemeEffect::Navigate(MENU);
        assert_eq!(navigate.flow_event(), Some(ThemeFlowEvent::Navigate(MENU)));

        let runtime = ThemeEffect::<ThemeScreenId, u8>::Runtime(7);
        assert_eq!(runtime.flow_event(), None);
        assert_eq!(
            ThemeEffect::<ThemeScreenId, ()>::Batch(vec![ThemeEffect::Navigate(MENU)]).flow_event(),
            None
        );
    }

    #[test]
    fn input_consumption_is_independent_of_scheduled_work() {
        let ignored = ThemeInputResult::<ThemeScreenId, u8>::ignored();
        assert!(!ignored.consumed);
        assert_eq!(ignored.effect, ThemeEffect::None);

        let consumed = ThemeInputResult::consumed(ThemeEffect::<ThemeScreenId, u8>::Runtime(7));
        assert!(consumed.consumed);
        assert_eq!(consumed.effect, ThemeEffect::Runtime(7));
    }

    #[test]
    fn batch_preserves_effect_order() {
        let batch = ThemeEffect::<ThemeScreenId, u8>::batch(vec![
            ThemeEffect::Runtime(1),
            ThemeEffect::Navigate(MENU),
            ThemeEffect::Runtime(2),
        ]);

        assert_eq!(
            batch,
            ThemeEffect::Batch(vec![
                ThemeEffect::Runtime(1),
                ThemeEffect::Navigate(MENU),
                ThemeEffect::Runtime(2),
            ])
        );
    }

    #[test]
    fn batch_flattens_nested_effects_and_discards_noops() {
        let batch = ThemeEffect::<ThemeScreenId, u8>::batch(vec![
            ThemeEffect::None,
            ThemeEffect::Runtime(1),
            ThemeEffect::Batch(vec![
                ThemeEffect::None,
                ThemeEffect::Navigate(MENU),
                ThemeEffect::Batch(vec![ThemeEffect::Runtime(2)]),
            ]),
        ]);

        assert_eq!(
            batch,
            ThemeEffect::Batch(vec![
                ThemeEffect::Runtime(1),
                ThemeEffect::Navigate(MENU),
                ThemeEffect::Runtime(2),
            ])
        );
    }

    #[test]
    fn append_to_writes_one_flat_caller_owned_sequence() {
        let mut effects = Vec::with_capacity(3);
        ThemeEffect::<ThemeScreenId, u8>::Batch(vec![
            ThemeEffect::Runtime(1),
            ThemeEffect::Batch(vec![ThemeEffect::None, ThemeEffect::Navigate(MENU)]),
            ThemeEffect::Runtime(2),
        ])
        .append_to(&mut effects);

        assert_eq!(
            effects,
            vec![
                ThemeEffect::Runtime(1),
                ThemeEffect::Navigate(MENU),
                ThemeEffect::Runtime(2),
            ]
        );
    }

    #[test]
    fn batch_collapses_empty_and_singleton_lists() {
        assert_eq!(
            ThemeEffect::<ThemeScreenId, u8>::batch(Vec::new()),
            ThemeEffect::None
        );
        assert_eq!(
            ThemeEffect::<ThemeScreenId, u8>::batch(vec![ThemeEffect::Runtime(7)]),
            ThemeEffect::Runtime(7)
        );
    }

    #[test]
    fn sequence_flattens_a_batched_second_effect() {
        let effect = ThemeEffect::<ThemeScreenId, u8>::sequence(
            ThemeEffect::Runtime(1),
            ThemeEffect::Batch(vec![ThemeEffect::Navigate(MENU), ThemeEffect::Runtime(2)]),
        );

        assert_eq!(
            effect,
            ThemeEffect::Batch(vec![
                ThemeEffect::Runtime(1),
                ThemeEffect::Navigate(MENU),
                ThemeEffect::Runtime(2),
            ])
        );

        assert_eq!(
            ThemeEffect::<ThemeScreenId, u8>::sequence(
                ThemeEffect::None,
                ThemeEffect::Batch(vec![ThemeEffect::Runtime(3)])
            ),
            ThemeEffect::Runtime(3)
        );
    }
}
