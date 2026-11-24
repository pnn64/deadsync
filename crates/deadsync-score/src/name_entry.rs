//! A clock-driven ranking-name editor, independent of input devices and presentation.

#[derive(Clone, Copy, Debug)]
pub struct TimedEntryPolicy {
    pub alphabet: &'static [u8],
    pub characters: usize,
    pub characters_per_second: f32,
    pub timeout_seconds: f32,
}

#[derive(Clone, Debug)]
pub struct EntryPlayer {
    pub active: bool,
    pub done: bool,
    pub letters: Vec<u8>,
    pub last_press: Vec<Option<f32>>,
}

#[derive(Clone, Debug)]
pub struct TimedNameEntry {
    pub policy: TimedEntryPolicy,
    pub elapsed: f32,
    pub players: [EntryPlayer; 2],
}

impl TimedNameEntry {
    pub fn new(policy: TimedEntryPolicy, active: [bool; 2], initial: [&str; 2]) -> Self {
        assert!(!policy.alphabet.is_empty() && policy.alphabet.is_ascii());
        assert!(policy.characters > 0 && policy.characters <= 32);
        assert!(policy.characters_per_second.is_finite() && policy.characters_per_second > 0.0);
        Self {
            policy,
            elapsed: 0.0,
            players: std::array::from_fn(|side| {
                let mut letters = vec![b' '; policy.characters];
                for (slot, byte) in initial[side]
                    .bytes()
                    .filter(u8::is_ascii)
                    .take(policy.characters)
                    .enumerate()
                {
                    letters[slot] = byte.to_ascii_uppercase();
                }
                EntryPlayer {
                    active: active[side],
                    done: !active[side],
                    letters,
                    last_press: vec![None; policy.characters],
                }
            }),
        }
    }

    pub fn advance(&mut self, delta: f32) {
        if delta.is_finite() && delta > 0.0 {
            self.elapsed += delta;
        }
        if self.elapsed >= self.policy.timeout_seconds {
            for player in &mut self.players {
                player.done = true;
            }
        }
    }

    pub fn current_index(&self) -> usize {
        (self.elapsed * self.policy.characters_per_second).round() as usize
            % self.policy.alphabet.len()
    }

    pub fn select(&mut self, side: usize, slot: usize) -> bool {
        let letter = self.policy.alphabet[self.current_index()];
        let Some(player) = self
            .players
            .get_mut(side)
            .filter(|player| player.active && !player.done)
        else {
            return false;
        };
        let Some(target) = player.letters.get_mut(slot) else {
            return false;
        };
        *target = letter;
        player.last_press[slot] = Some(self.elapsed);
        true
    }

    pub fn finish(&mut self, side: usize) -> bool {
        let Some(player) = self
            .players
            .get_mut(side)
            .filter(|player| player.active && !player.done)
        else {
            return false;
        };
        player.done = true;
        true
    }

    pub fn complete(&self) -> bool {
        self.players.iter().all(|player| player.done)
    }

    pub fn names(&self) -> [Option<String>; 2] {
        self.players.each_ref().map(|player| {
            if !player.active {
                return None;
            }
            let name = std::str::from_utf8(&player.letters)
                .expect("ASCII name editor")
                .trim();
            (!name.is_empty()).then(|| name.to_owned())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const POLICY: TimedEntryPolicy = TimedEntryPolicy {
        alphabet: b"    ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        characters: 4,
        characters_per_second: 2.5,
        timeout_seconds: 24.0,
    };

    #[test]
    fn columns_choose_the_nearest_timed_character_and_keep_each_side_independent() {
        let mut entry = TimedNameEntry::new(POLICY, [true, true], ["OLD", "P2"]);
        entry.advance(1.6);
        assert!(entry.select(1, 0));
        entry.advance(0.4);
        assert!(entry.select(1, 3));
        assert_eq!(entry.names(), [Some("OLD".into()), Some("A2 B".into())]);
        assert!(entry.finish(1));
        assert!(!entry.select(1, 1));
        assert!(!entry.complete());
        entry.advance(22.0);
        assert!(entry.complete());
    }
}
