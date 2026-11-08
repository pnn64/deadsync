//! Loading policy for authored callbacks that cannot use offline playback.
use crate::SongLuaCompileContext;
use crate::input::RUNTIME_REQUIRED as REQUIRED;

pub(crate) fn compile<T>(
    context: &SongLuaCompileContext,
    mut work: impl FnMut(&SongLuaCompileContext) -> Result<T, String>,
) -> Result<T, String> {
    match work(context) {
        Err(error) if !context.live && error == REQUIRED => {
            let mut runtime = context.clone();
            runtime.live = true;
            runtime.defer_initial_live_update = true;
            runtime.screen_loading = true;
            work(&runtime)
        }
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn successful_compilation_keeps_the_requested_mode() {
        let context = SongLuaCompileContext::new(Path::new("."), "policy");
        let mut calls = 0;
        assert_eq!(
            compile(&context, |context| {
                calls += 1;
                assert!(!context.live);
                Ok(19)
            }),
            Ok(19)
        );
        assert_eq!(calls, 1);
    }

    #[test]
    fn runtime_rejection_retries_with_loading_and_clock_handoff() {
        let mut context = SongLuaCompileContext::new(Path::new("."), "policy");
        context.random_seed = 0x1234_5678;
        context.song_music_rate = 1.375;
        let mut calls = Vec::new();
        let result = compile(&context, |context| {
            calls.push(context.live);
            assert_eq!(context.random_seed, 0x1234_5678);
            assert_eq!(context.song_music_rate.to_bits(), 1.375f32.to_bits());
            if context.live {
                assert!(context.defer_initial_live_update);
                assert!(context.screen_loading);
                Ok(23)
            } else {
                Err(REQUIRED.into())
            }
        });
        assert_eq!(result, Ok(23));
        assert_eq!(calls, [false, true]);
        assert!(!context.live);
    }

    #[test]
    fn unrelated_errors_and_live_rejections_are_not_retried() {
        for (live, error) in [(false, "missing asset"), (true, REQUIRED)] {
            let mut context = SongLuaCompileContext::new(Path::new("."), "policy");
            context.live = live;
            let mut calls = 0;
            let result: Result<(), _> = compile(&context, |_| {
                calls += 1;
                Err(error.into())
            });
            assert_eq!(result, Err(error.into()));
            assert_eq!(calls, 1);
        }
    }

    #[test]
    fn retry_errors_remain_visible() {
        let context = SongLuaCompileContext::new(Path::new("."), "policy");
        let mut calls = 0;
        let result: Result<(), _> = compile(&context, |context| {
            calls += 1;
            Err(if context.live {
                "runtime asset error"
            } else {
                REQUIRED
            }
            .into())
        });
        assert_eq!(result, Err("runtime asset error".into()));
        assert_eq!(calls, 2);
    }
}
