//! Preparation callbacks retained by the non-startup capture scheduler.
use super::*;

#[derive(Clone, Default)]
pub(super) struct State {
    pub(super) command: Option<Vec<u8>>,
    pub(super) interval: f64,
    pub(super) remaining: Option<f64>,
    pub(super) running: bool,
}

fn invoke(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    command: &[u8],
    queued: Option<f32>,
) -> Result<(), Error> {
    let (prior, prior_clock) = {
        let mut guard = store.lock().unwrap();
        let prior_clock = if let Some(remaining) = queued {
            guard.replay_clock.as_mut().and_then(|clock| {
                clock.reset(actor);
                clock.dispatched(actor, remaining)
            })
        } else {
            guard.replay_clock.as_ref().and_then(|clock| clock.dispatch)
        };
        let time = guard.replay_frame.map(|(_, time)| time);
        let target = guard.actors.get_mut(&actor).unwrap();
        let capture = target.capture.as_mut().unwrap();
        capture.reset_command();
        capture.recurring.command = None;
        capture.recurring.running = true;
        target.queue.clear();
        let prior = guard.replay_dispatch;
        if queued.is_some() {
            guard.replay_dispatch = Some(song_immediate::Dispatch {
                seconds: time,
                delay: 0.,
            });
        }
        (prior, prior_clock)
    };
    let result = (|| {
        song_messages::timing(call, actor, 0., None)?;
        for key in [
            "__songlua_capture_cursor",
            "__songlua_capture_tween_time_left",
        ] {
            song_replay::raw_set(call, actor, key, Value::Number(0.))?;
        }
        song_replay::raw_set(call, actor, "__songlua_capture_opt2", Value::Nil)?;
        // This scheduler invokes the actor's own named command. ActorFrame's
        // recursive PlayCommand dispatch is a separate queue path.
        run_named(call, store, actor, command, None)?;
        song_pose::advance_position(call, store, actor)
    })();
    let mut guard = store.lock().unwrap();
    guard.replay_dispatch = prior;
    if let Some(clock) = &mut guard.replay_clock {
        clock.dispatch = prior_clock;
    }
    let recurring = &mut guard
        .actors
        .get_mut(&actor)
        .unwrap()
        .capture
        .as_mut()
        .unwrap()
        .recurring;
    recurring.running = false;
    drop(guard);
    match result {
        Err(error) => diagnostics::report(
            call,
            store,
            actor,
            diagnostics::Callback::Command(command),
            error,
        ),
        Ok(()) => Ok(()),
    }
}

pub(super) fn advance(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    delta: f32,
) -> Result<(), Error> {
    let (command, mut interval, mut left) = {
        let guard = store.lock().unwrap();
        if guard.replay_frame.is_none()
            || guard.clock.live.load(std::sync::atomic::Ordering::Relaxed)
        {
            return Ok(());
        }
        let actor = &guard.actors[&actor];
        if !actor.registered {
            return Ok(());
        }
        let Some(capture) = &actor.capture else {
            return Ok(());
        };
        let recurring = &capture.recurring;
        let Some(command) = &recurring.command else {
            return Ok(());
        };
        (
            command.clone(),
            recurring.interval,
            recurring.remaining.unwrap_or(recurring.interval) as f32,
        )
    };
    // The native zero-interval branch runs once per active update, including
    // a zero or negative delta, and carries no queued publication scope.
    if interval <= f64::EPSILON {
        return invoke(call, store, actor, &command, None);
    }
    let mut delta = delta.max(0.);
    let mut runs = 0;
    while delta > 0. && runs < 64 {
        if left > 0. {
            let used = left.min(delta);
            left -= used;
            delta -= used;
            if delta == 0. {
                break;
            }
        }
        invoke(call, store, actor, &command, Some(delta))?;
        {
            let mut guard = store.lock().unwrap();
            let recurring = &mut guard
                .actors
                .get_mut(&actor)
                .unwrap()
                .capture
                .as_mut()
                .unwrap()
                .recurring;
            if recurring.command.is_none() {
                recurring.remaining = None;
                return Ok(());
            }
            interval = recurring.interval;
        }
        runs += 1;
        if interval <= f64::EPSILON {
            break;
        }
        left = interval as f32;
    }
    store
        .lock()
        .unwrap()
        .actors
        .get_mut(&actor)
        .unwrap()
        .capture
        .as_mut()
        .unwrap()
        .recurring
        .remaining = Some(f64::from(left));
    Ok(())
}
