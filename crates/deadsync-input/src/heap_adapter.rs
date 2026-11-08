pub struct Fixture {
    store: DebounceStore,
    base: Instant,
    counter: u64,
}
impl Fixture {
    pub fn new(n: usize, base: Instant) -> Self {
        let mut store = DebounceStore::new();
        store.prepare_slots(n);
        let mut out = Self {
            store,
            base,
            counter: 0,
        };
        out.fill();
        out
    }
    fn fill(&mut self) {
        for slot in (0..self.store.slots.len()).rev() {
            let due = self.base + Duration::from_nanos(1_000_000_000 + slot as u64);
            self.store.refresh_due_slot(slot, None, Some(due));
        }
    }
    pub fn update(&mut self, slot: usize, tick: Option<u64>) {
        let old = self.store.slots[slot].due_at;
        let new = tick.map(|tick| self.base + Duration::from_nanos(tick));
        self.store.refresh_due_slot(slot, old, new);
    }
    pub fn cancel_root(&mut self) {
        if let Some(&slot) = self.store.due_slots.first() {
            self.store.unschedule_slot(slot);
        }
    }
    pub fn run(&mut self, kind: usize) {
        let n = self.store.slots.len();
        match kind {
            0 => {
                for slot in 0..n {
                    self.counter += 1;
                    self.store.slots[slot].due_at =
                        Some(self.base + Duration::from_nanos(1_000_000_000 - self.counter));
                    let ix = self.store.slots[slot].scheduled_ix as usize;
                    self.store.sift_up(ix);
                }
            }
            1 => {
                for _ in 0..n {
                    self.counter += 1;
                    let slot = self.store.due_slots[0];
                    self.store.slots[slot].due_at =
                        Some(self.base + Duration::from_nanos(2_000_000_000 + self.counter));
                    self.store.sift_down(0);
                }
            }
            2 => {
                while !self.store.due_slots.is_empty() {
                    self.cancel_root();
                }
                self.fill();
            }
            3 => {
                for slot in (0..n).step_by(2).chain((1..n).step_by(2)) {
                    self.store.unschedule_slot(slot);
                }
                self.fill();
            }
            _ => unreachable!(),
        }
        std::hint::black_box(&self.store.due_slots);
    }
    pub fn snapshot(&self) -> Vec<u64> {
        let mut out = vec![
            self.store.epoch as u64,
            self.store.active_len as u64,
            self.store.slots.len() as u64,
            self.store.slots.capacity() as u64,
            self.store.due_slots.len() as u64,
            self.store.due_slots.capacity() as u64,
        ];
        out.extend(self.store.due_slots.iter().map(|&x| x as u64));
        for slot in &self.store.slots {
            assert!(slot.state.is_none());
            out.extend([
                slot.epoch as u64,
                slot.scheduled_ix as u64,
                u64::from(slot.due_at.is_some()),
                slot.due_at
                    .map_or(0, |t| t.duration_since(self.base).as_nanos() as u64),
            ]);
        }
        out
    }
}

pub struct EventFixture {
    store: DebounceStore,
    base: Instant,
    cycle: u64,
}
impl EventFixture {
    pub fn new(n: usize, base: Instant) -> Self {
        let mut store = DebounceStore::new();
        store.prepare_slots(n);
        Self {
            store,
            base,
            cycle: 0,
        }
    }
    pub fn run(&mut self, mut visit: impl FnMut(&DebouncedEdge)) {
        let press = self.base + Duration::from_millis(self.cycle * 100);
        self.cycle += 1;
        let window = Duration::from_millis(20);
        for (pressed, now) in [(true, press), (false, press + Duration::from_millis(1))] {
            for slot in (0..self.store.slots.len()).rev() {
                let edges = debounce_input_edge_in_store_mut(
                    &mut self.store,
                    slot,
                    1u32.rotate_left(slot as u32),
                    if slot & 1 == 0 {
                        InputSource::Keyboard
                    } else {
                        InputSource::Gamepad
                    },
                    pressed,
                    now,
                    slot as u64,
                    DebounceWindows::uniform(window),
                    || now,
                );
                for edge in [edges.first, edges.second].into_iter().flatten() {
                    visit(&edge);
                }
            }
        }
        for now in [press + window, press + window + window] {
            while let Some(edge) =
                next_due_edge(&mut self.store, now, DebounceWindows::uniform(window))
            {
                visit(&edge);
            }
        }
    }
    pub fn trace(&mut self) -> Vec<u64> {
        let base = self.base;
        let mut out = Vec::new();
        self.run(|edge| {
            out.extend([
                u64::from(edge.action_mask),
                u64::from(edge.input_slot),
                u64::from(edge.pressed),
                match edge.source {
                    InputSource::Keyboard => 0,
                    InputSource::Gamepad => 1,
                },
                edge.timestamp.duration_since(base).as_nanos() as u64,
                edge.timestamp_host_nanos,
                edge.stored_at.duration_since(base).as_nanos() as u64,
                edge.emitted_at.duration_since(base).as_nanos() as u64,
            ])
        });
        out.extend([
            self.store.epoch as u64,
            self.store.active_len as u64,
            self.store.slots.len() as u64,
            self.store.slots.capacity() as u64,
            self.store.due_slots.len() as u64,
            self.store.due_slots.capacity() as u64,
        ]);
        for slot in &self.store.slots {
            assert!(slot.state.is_none());
            out.extend([
                slot.epoch as u64,
                slot.scheduled_ix as u64,
                u64::from(slot.due_at.is_some()),
            ]);
        }
        out
    }
}
