// Frozen from 303dab2c6; only test visibility changed.
pub(super) struct TextAttrCursor<'a> {
    attributes: &'a [actors::TextAttribute],
    scratch: &'a mut TextAttrScratch,
    active_max: Option<usize>,
    next_start: usize,
    next_end: usize,
}

impl<'a> TextAttrCursor<'a> {
    pub(super) fn new(
        attributes: &'a [actors::TextAttribute],
        scratch: &'a mut TextAttrScratch,
    ) -> Option<Self> {
        if attributes.is_empty() {
            return None;
        }

        let TextAttrScratch {
            start_order,
            end_order,
            active,
        } = scratch;
        start_order.clear();
        end_order.clear();
        active.clear();
        // Moving ranges can increase overlap without increasing their count.
        active.reserve(attributes.len());
        start_order.extend(0..attributes.len());
        end_order.extend(0..attributes.len());

        // Equal-boundary events are consumed together; active_max preserves
        // original attribute precedence independently of their event order.
        start_order.sort_unstable_by_key(|&index| attributes[index].start);
        end_order.sort_unstable_by_key(|&index| attr_end(&attributes[index]));

        Some(Self {
            attributes,
            scratch,
            active_max: None,
            next_start: 0,
            next_end: 0,
        })
    }

    #[inline(always)]
    fn push_active(&mut self, attr_index: usize) {
        self.scratch.active.push(attr_index);
        self.active_max = Some(
            self.active_max
                .map_or(attr_index, |max| max.max(attr_index)),
        );
    }

    #[inline(always)]
    fn remove_active(&mut self, attr_index: usize) {
        let Some(index) = self
            .scratch
            .active
            .iter()
            .position(|&index| index == attr_index)
        else {
            return;
        };
        self.scratch.active.swap_remove(index);
        if self.active_max == Some(attr_index) {
            self.active_max = self.scratch.active.iter().copied().max();
        }
    }

    #[inline(always)]
    pub(super) fn colors_for(&mut self, char_index: usize) -> [[f32; 4]; 4] {
        while self.next_end < self.scratch.end_order.len()
            && attr_end(&self.attributes[self.scratch.end_order[self.next_end]]) <= char_index
        {
            let attr_index = self.scratch.end_order[self.next_end];
            self.remove_active(attr_index);
            self.next_end += 1;
        }

        while self.next_start < self.scratch.start_order.len()
            && self.attributes[self.scratch.start_order[self.next_start]].start <= char_index
        {
            let attr_index = self.scratch.start_order[self.next_start];
            let attr = &self.attributes[attr_index];
            if char_index < attr_end(attr) {
                self.push_active(attr_index);
            }
            self.next_start += 1;
        }

        self.active_max
            .map(|index| self.attributes[index].colors())
            .unwrap_or([[1.0; 4]; 4])
    }

    #[cfg(test)]
    fn tint_for(&mut self, char_index: usize) -> [f32; 4] {
        self.colors_for(char_index)[0]
    }
}
