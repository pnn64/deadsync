// Frozen from 83fbce5457d789be942be4059eef4fdbb3191c34; test-only baseline.
use super::*;

#[derive(Clone, Debug)]
pub struct OriginalKeymap {
    pub(super) map: HashMap<VirtualAction, Vec<InputBinding>>,
    pub(super) key_rev: Box<[Vec<VirtualAction>]>,
    pub(super) key_rev_extra: HashMap<KeyCode, Vec<VirtualAction>>,
    pub(super) pad_dir_rev: [Vec<VirtualAction>; 4],
    pub(super) pad_dir_on_rev: HashMap<(usize, PadDir), Vec<VirtualAction>>,
    pub(super) pad_code_rev: HashMap<u32, Vec<PadCodeRev>>,
}

impl Default for OriginalKeymap {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            key_rev: new_key_rev(),
            key_rev_extra: HashMap::new(),
            pad_dir_rev: std::array::from_fn(|_| Vec::new()),
            pad_dir_on_rev: HashMap::new(),
            pad_code_rev: HashMap::new(),
        }
    }
}

impl OriginalKeymap {
    #[inline(always)]
    fn key_actions(&self, code: KeyCode) -> &[VirtualAction] {
        match dense_key_ix(code) {
            Some(ix) => &self.key_rev[ix],
            None => self.key_rev_extra.get(&code).map_or(&[], Vec::as_slice),
        }
    }

    #[inline(always)]
    fn remove_rev(&mut self, action: VirtualAction, prev: &[InputBinding]) {
        for b in prev {
            match *b {
                InputBinding::Key(code) => {
                    if let Some(ix) = dense_key_ix(code) {
                        let v = &mut self.key_rev[ix];
                        v.retain(|a| *a != action);
                    } else if let Some(v) = self.key_rev_extra.get_mut(&code) {
                        v.retain(|a| *a != action);
                        if v.is_empty() {
                            self.key_rev_extra.remove(&code);
                        }
                    }
                }
                InputBinding::PadDir(dir) => {
                    self.pad_dir_rev[dir.ix()].retain(|a| *a != action);
                }
                InputBinding::PadDirOn { device, dir } => {
                    let key = (device, dir);
                    if let Some(v) = self.pad_dir_on_rev.get_mut(&key) {
                        v.retain(|a| *a != action);
                        if v.is_empty() {
                            self.pad_dir_on_rev.remove(&key);
                        }
                    }
                }
                InputBinding::GamepadCode(binding) => {
                    if let Some(v) = self.pad_code_rev.get_mut(&binding.code_u32) {
                        v.retain(|e| {
                            e.act != action || e.device != binding.device || e.uuid != binding.uuid
                        });
                        if v.is_empty() {
                            self.pad_code_rev.remove(&binding.code_u32);
                        }
                    }
                }
            }
        }
    }

    #[inline(always)]
    fn add_rev(&mut self, action: VirtualAction, inputs: &[InputBinding]) {
        for b in inputs {
            match *b {
                InputBinding::Key(code) => {
                    if let Some(ix) = dense_key_ix(code) {
                        self.key_rev[ix].push(action);
                    } else {
                        self.key_rev_extra.entry(code).or_default().push(action);
                    }
                }
                InputBinding::PadDir(dir) => self.pad_dir_rev[dir.ix()].push(action),
                InputBinding::PadDirOn { device, dir } => self
                    .pad_dir_on_rev
                    .entry((device, dir))
                    .or_default()
                    .push(action),
                InputBinding::GamepadCode(binding) => self
                    .pad_code_rev
                    .entry(binding.code_u32)
                    .or_default()
                    .push(PadCodeRev {
                        act: action,
                        device: binding.device,
                        uuid: binding.uuid,
                    }),
            }
        }
    }

    #[inline(always)]
    pub fn bind(&mut self, action: VirtualAction, inputs: &[InputBinding]) {
        if let Some(prev) = self.map.insert(action, inputs.to_vec()) {
            self.remove_rev(action, &prev);
        }
        self.add_rev(action, inputs);
    }

    #[inline]
    pub(crate) fn bindings_for_action(&self, action: VirtualAction) -> &[InputBinding] {
        self.map.get(&action).map_or(&[], Vec::as_slice)
    }

    /// Compares ordered bindings for every action, treating missing and empty
    /// lists alike, as INI serialization does. Reverse lookup storage is ignored.
    #[must_use]
    pub fn has_same_bindings(&self, other: &Self) -> bool {
        crate::ALL_VIRTUAL_ACTIONS
            .iter()
            .all(|&action| self.bindings_for_action(action) == other.bindings_for_action(action))
    }

    /// Returns the first keyboard key bound to this virtual action, if any.
    /// This reflects the first `KeyCode::...` token listed for the action
    /// in `deadsync.ini` (or the hardcoded default keymap).
    #[inline(always)]
    #[must_use]
    pub fn first_key_binding(&self, action: VirtualAction) -> Option<KeyCode> {
        self.map.get(&action).and_then(|bindings| {
            bindings.iter().find_map(|b| {
                if let InputBinding::Key(code) = b {
                    Some(*code)
                } else {
                    None
                }
            })
        })
    }

    /// Returns the raw binding at the given index for this virtual action,
    /// preserving the order parsed from deadsync.ini.
    #[inline(always)]
    #[must_use]
    pub fn binding_at(&self, action: VirtualAction, index: usize) -> Option<InputBinding> {
        self.map
            .get(&action)
            .and_then(|bindings| bindings.get(index))
            .copied()
    }

    #[inline(always)]
    #[must_use]
    pub fn keycode_mapped(&self, code: KeyCode) -> bool {
        !self.key_actions(code).is_empty()
    }

    #[inline(always)]
    pub fn keycode_has_action(&self, code: KeyCode, keep: impl Fn(VirtualAction) -> bool) -> bool {
        for &action in self.key_actions(code) {
            if keep(action) {
                return true;
            }
        }
        false
    }

    #[inline(always)]
    #[must_use]
    pub fn raw_key_event_mapped(&self, ev: &RawKeyboardEvent) -> bool {
        self.keycode_mapped(ev.code)
    }

    #[inline(always)]
    pub fn raw_key_event_has_action(
        &self,
        ev: &RawKeyboardEvent,
        keep: impl Fn(VirtualAction) -> bool,
    ) -> bool {
        self.keycode_has_action(ev.code, keep)
    }

    #[inline(always)]
    #[must_use]
    pub fn pad_event_mapped(&self, ev: &PadEvent) -> bool {
        match *ev {
            PadEvent::Dir { id, dir, .. } => {
                let dev = usize::from(id);
                !self.pad_dir_rev[dir.ix()].is_empty()
                    || self.pad_dir_on_rev.contains_key(&(dev, dir))
            }
            PadEvent::RawButton { id, code, uuid, .. } => {
                let dev = usize::from(id);
                let Some(entries) = self.pad_code_rev.get(&code.into_u32()) else {
                    return false;
                };
                for entry in entries {
                    if let Some(d_expected) = entry.device
                        && d_expected != dev
                    {
                        continue;
                    }
                    if let Some(u_expected) = entry.uuid
                        && u_expected != uuid
                    {
                        continue;
                    }
                    return true;
                }
                false
            }
            PadEvent::RawAxis { .. } => false,
        }
    }
}
