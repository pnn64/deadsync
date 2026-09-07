pub(super) fn rebuild<T>(
    watch: Option<T>,
    device_fds: impl IntoIterator<Item = i32>,
    key_fds: impl IntoIterator<Item = i32>,
    registrations: &mut Vec<T>,
    mut registration: impl FnMut(i32) -> T,
) -> (usize, usize) {
    registrations.clear();
    let device_offset = usize::from(watch.is_some());
    registrations.extend(watch);
    registrations.extend(device_fds.into_iter().map(&mut registration));
    let key_offset = registrations.len();
    registrations.extend(key_fds.into_iter().map(registration));
    (device_offset, key_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct Registration {
        fd: i32,
        events: i16,
        revents: i16,
    }

    fn registration(fd: i32) -> Registration {
        Registration {
            fd,
            events: 1,
            revents: 0,
        }
    }

    #[test]
    fn rebuild_preserves_watcher_device_and_key_offsets() {
        let watch = Registration {
            fd: 10,
            events: 1,
            revents: 0,
        };
        let mut registrations = vec![registration(-1)];

        let offsets = rebuild(
            Some(watch),
            [20, 21],
            [30, 31, 32],
            &mut registrations,
            registration,
        );

        assert_eq!(offsets, (1, 3));
        assert_eq!(
            registrations,
            vec![
                registration(10),
                registration(20),
                registration(21),
                registration(30),
                registration(31),
                registration(32),
            ]
        );

        let offsets = rebuild(
            None,
            [40],
            std::iter::empty(),
            &mut registrations,
            registration,
        );
        assert_eq!(offsets, (0, 1));
        assert_eq!(registrations, vec![registration(40)]);
    }

    #[test]
    fn rebuild_supports_watcher_and_device_only_layouts() {
        let watch = Registration {
            fd: 10,
            events: 1,
            revents: 0,
        };
        let mut registrations = Vec::new();

        let offsets = rebuild(
            Some(watch),
            [20, 21],
            std::iter::empty(),
            &mut registrations,
            registration,
        );

        assert_eq!(offsets, (1, 3));
        assert_eq!(
            registrations,
            vec![registration(10), registration(20), registration(21)]
        );
    }
}
