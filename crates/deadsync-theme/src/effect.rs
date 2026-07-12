/// Work requested by a concrete theme after handling input or updating a
/// screen.
///
/// `S` is the concrete theme's screen identity and `R` is its runtime request
/// payload. Keeping both generic lets themes define different screen graphs
/// and optional runtime capabilities without expanding this contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeEffect<S, R> {
    None,
    /// Consume the current input edge without scheduling shell work.
    ConsumeInput,
    /// Execute multiple effects in order. Runtime owners must route each
    /// nested effect normally so redirects observe the current state.
    Batch(Vec<Self>),
    Navigate(S),
    /// Navigate immediately without the current screen's out-transition.
    NavigateNoFade(S),
    Exit,
    Shutdown,
    Runtime(R),
}

/// The flow-only subset of [`ThemeEffect`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeFlowEvent<S> {
    Navigate(S),
    NavigateNoFade(S),
    Exit,
    Shutdown,
}

impl<S: Copy, R> ThemeEffect<S, R> {
    /// Return the effect as a flow event, or `None` for non-flow effects.
    #[inline(always)]
    pub fn flow_event(&self) -> Option<ThemeFlowEvent<S>> {
        match self {
            Self::Navigate(screen) => Some(ThemeFlowEvent::Navigate(*screen)),
            Self::NavigateNoFade(screen) => Some(ThemeFlowEvent::NavigateNoFade(*screen)),
            Self::Exit => Some(ThemeFlowEvent::Exit),
            Self::Shutdown => Some(ThemeFlowEvent::Shutdown),
            Self::None | Self::ConsumeInput | Self::Batch(_) | Self::Runtime(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ThemeEffect, ThemeFlowEvent};
    use crate::ThemeScreenId;

    const MENU: ThemeScreenId = ThemeScreenId::new("menu");

    #[test]
    fn extracts_only_flow_effects() {
        let navigate: ThemeEffect<_, ()> = ThemeEffect::Navigate(MENU);
        assert_eq!(navigate.flow_event(), Some(ThemeFlowEvent::Navigate(MENU)));

        let runtime = ThemeEffect::<ThemeScreenId, u8>::Runtime(7);
        assert_eq!(runtime.flow_event(), None);
        assert_eq!(
            ThemeEffect::<ThemeScreenId, ()>::ConsumeInput.flow_event(),
            None
        );
        assert_eq!(
            ThemeEffect::<ThemeScreenId, ()>::Batch(vec![ThemeEffect::Navigate(MENU)]).flow_event(),
            None
        );
    }

    #[test]
    fn batch_preserves_effect_order() {
        let batch = ThemeEffect::<ThemeScreenId, u8>::Batch(vec![
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
}
