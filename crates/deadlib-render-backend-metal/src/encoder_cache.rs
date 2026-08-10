#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DrawKind {
    Sprite,
    Mesh,
    TexturedMesh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CullMode {
    None,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferUpdate {
    Bind,
    Offset,
}

/// Frame-local cache of state already installed on one Metal encoder.
#[derive(Debug, Default)]
pub(crate) struct EncoderCache {
    kind: Option<DrawKind>,
    pipeline: Option<(DrawKind, u8)>,
    cameras: [Option<u8>; 2],
    texture: Option<u64>,
    sampler: Option<(u64, bool)>,
    depth: Option<bool>,
    cull: Option<CullMode>,
}

impl EncoderCache {
    #[inline(always)]
    pub(crate) fn kind_changed(&mut self, kind: DrawKind) -> bool {
        update(&mut self.kind, kind)
    }

    #[inline(always)]
    pub(crate) fn instance_buffer(&mut self, kind: DrawKind) -> BufferUpdate {
        if self.kind_changed(kind) {
            // Textured-mesh instances occupy vertex slot 1, replacing the
            // sprite/mesh camera bytes installed at that slot.
            if kind == DrawKind::TexturedMesh {
                self.cameras[0] = None;
            }
            BufferUpdate::Bind
        } else {
            BufferUpdate::Offset
        }
    }

    #[inline(always)]
    pub(crate) fn pipeline_changed(&mut self, kind: DrawKind, blend: u8) -> bool {
        update(&mut self.pipeline, (kind, blend))
    }

    #[inline(always)]
    pub(crate) fn camera_changed(&mut self, slot: usize, camera: u8) -> bool {
        update(&mut self.cameras[slot], camera)
    }

    #[inline(always)]
    pub(crate) fn texture_changed(&mut self, texture: u64) -> bool {
        update(&mut self.texture, texture)
    }

    #[inline(always)]
    pub(crate) fn sampler_changed(&mut self, texture: u64, repeat: bool) -> bool {
        update(&mut self.sampler, (texture, repeat))
    }

    #[inline(always)]
    pub(crate) fn depth_changed(&mut self, depth: bool) -> bool {
        update(&mut self.depth, depth)
    }

    #[inline(always)]
    pub(crate) fn cull_changed(&mut self, cull: CullMode) -> bool {
        update(&mut self.cull, cull)
    }
}

#[inline(always)]
fn update<T: Copy + PartialEq>(current: &mut Option<T>, next: T) -> bool {
    if *current == Some(next) {
        false
    } else {
        *current = Some(next);
        true
    }
}
