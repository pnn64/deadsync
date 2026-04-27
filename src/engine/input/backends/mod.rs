use std::time::Instant;

#[cfg(windows)]
pub(super) use super::RawKeyboardEvent;
pub(super) use super::{
    GpSystemEvent, PadBackend, PadCode, PadDir, PadEvent, PadId, uuid_from_bytes,
};

const DIRS: [PadDir; 4] = [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right];

#[inline(always)]
pub(super) fn emit_dir_edges(
    emit_pad: &mut impl FnMut(PadEvent),
    id: PadId,
    dir_state: &mut [bool; 4],
    timestamp: Instant,
    host_nanos: u64,
    want: [bool; 4],
) {
    for i in 0..DIRS.len() {
        if dir_state[i] == want[i] {
            continue;
        }
        dir_state[i] = want[i];
        emit_pad(PadEvent::Dir {
            id,
            timestamp,
            host_nanos,
            dir: DIRS[i],
            pressed: want[i],
        });
    }
}

#[cfg(target_os = "freebsd")]
pub(super) mod devd;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub(super) mod evdev;
#[cfg(target_os = "freebsd")]
pub(super) mod hidraw;
#[cfg(target_os = "macos")]
pub(super) mod iohid;
#[cfg(windows)]
pub(super) mod w32_raw_input;
#[cfg(windows)]
pub(super) mod wgi;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_dir_edges_updates_only_changed_dirs() {
        let mut events = Vec::new();
        let timestamp = Instant::now();
        let mut dir_state = [false; 4];

        emit_dir_edges(
            &mut |event| events.push(event),
            PadId(7),
            &mut dir_state,
            timestamp,
            42,
            [true, false, true, false],
        );
        assert_eq!(dir_state, [true, false, true, false]);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0],
            PadEvent::Dir {
                id: PadId(7),
                timestamp: ts,
                host_nanos: 42,
                dir: PadDir::Up,
                pressed: true,
            } if ts == timestamp
        ));
        assert!(matches!(
            events[1],
            PadEvent::Dir {
                id: PadId(7),
                timestamp: ts,
                host_nanos: 42,
                dir: PadDir::Left,
                pressed: true,
            } if ts == timestamp
        ));

        events.clear();
        emit_dir_edges(
            &mut |event| events.push(event),
            PadId(7),
            &mut dir_state,
            timestamp,
            42,
            [true, false, true, false],
        );
        assert!(events.is_empty());
    }
}
