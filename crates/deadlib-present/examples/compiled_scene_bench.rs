use deadlib_present::actors::{Actor, SizeSpec, SpriteSource};
use deadlib_present::anim::EffectState;
use deadlib_present::compiled_scene::{
    CompileOptions, CompiledSceneScratch, NodeId, SceneCompiler, SpriteUvPatch,
};
use deadlib_present::compose::{
    ComposeScratch, TextLayoutCache, build_screen_cached_with_scratch_and_texture_context,
};
use deadlib_present::space::Metrics;
use deadlib_present::texture::{TextureContext, TextureMeta};
use deadlib_render::BlendMode;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// Run with:
// cargo run --release -p deadlib-present --example compiled_scene_bench

const SPRITES: usize = 4_096;
const ITERS: usize = 1_000;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: each operation forwards the original pointer/layout to `System` and
// only updates lock-free counters after successful allocation.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged to the system allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged to the system allocator.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` and `layout` came from the matching system allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the original allocation and layout came from `System`.
        let out = unsafe { System.realloc(ptr, layout, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(layout.size()) as u64,
                Ordering::Relaxed,
            );
        }
        out
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs.saturating_sub(before.allocs),
            reallocs: self.reallocs.saturating_sub(before.reallocs),
            bytes: self.bytes.saturating_sub(before.bytes),
        }
    }
}

struct BenchTextures;

impl TextureContext for BenchTextures {
    fn texture_registry_generation(&self) -> u64 {
        1
    }

    fn texture_dims(&self, key: &str) -> Option<TextureMeta> {
        (key == "bench").then_some(TextureMeta { w: 64, h: 64 })
    }

    fn sprite_sheet_dims(&self, _: &str) -> (u32, u32) {
        (1, 1)
    }

    fn texture_handle(&self, key: &str) -> deadlib_render::TextureHandle {
        u64::from(key == "bench")
    }
}

fn sprite(index: usize) -> Actor {
    Actor::Sprite {
        align: [0.5, 0.5],
        offset: [(index % 64) as f32 * 10.0, (index / 64) as f32 * 8.0],
        world_z: 0.0,
        size: [SizeSpec::Px(8.0), SizeSpec::Px(8.0)],
        source: SpriteSource::TextureStatic("bench"),
        tint: [1.0, 1.0, 1.0, 1.0],
        glow: [0.0; 4],
        z: (index % 8) as i16,
        cell: None,
        grid: None,
        uv_rect: None,
        visible: true,
        flip_x: false,
        flip_y: false,
        cropleft: 0.0,
        cropright: 0.0,
        croptop: 0.0,
        cropbottom: 0.0,
        fadeleft: 0.0,
        faderight: 0.0,
        fadetop: 0.0,
        fadebottom: 0.0,
        blend: BlendMode::Alpha,
        mask_source: false,
        mask_dest: false,
        rot_x_deg: 0.0,
        rot_y_deg: 0.0,
        rot_z_deg: 0.0,
        local_offset: [0.0, 0.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        texcoordvelocity: None,
        animate: false,
        state_delay: 0.0,
        scale: [1.0, 1.0],
        shadow_len: [0.0, 0.0],
        shadow_color: [0.0; 4],
        effect: EffectState::default(),
    }
}

fn measure(mut run: impl FnMut()) -> (Duration, AllocSnapshot) {
    let alloc_before = ALLOC.snapshot();
    let started = Instant::now();
    for _ in 0..ITERS {
        run();
    }
    (started.elapsed(), ALLOC.snapshot().delta(alloc_before))
}

fn main() {
    let actors: Vec<_> = (0..SPRITES).map(sprite).collect();
    let metrics = Metrics {
        left: 0.0,
        right: 640.0,
        top: 0.0,
        bottom: 480.0,
    };
    let fonts = HashMap::new();
    let textures = BenchTextures;
    let clear = [0.0, 0.0, 0.0, 1.0];

    let mut compile_text = TextLayoutCache::default();
    let scene = SceneCompiler::new(&metrics, &fonts, &textures, &mut compile_text, 1, 1)
        .compile(&actors, clear, CompileOptions::IMMUTABLE)
        .expect("benchmark fixture must compile");
    let mut compiled_scratch = CompiledSceneScratch::default();
    compiled_scratch.reserve(scene.capacity());
    let mut compiled_warm = scene.emit(&mut compiled_scratch);
    compiled_scratch.recycle_render_list(&mut compiled_warm);

    let mut legacy_text = TextLayoutCache::default();
    let mut legacy_scratch = ComposeScratch::default();
    let mut legacy_warm = build_screen_cached_with_scratch_and_texture_context(
        &actors,
        clear,
        &metrics,
        &fonts,
        0.0,
        &mut legacy_text,
        &mut legacy_scratch,
        &textures,
    );
    legacy_scratch.recycle_render_list(&mut legacy_warm);

    let (legacy_time, legacy_alloc) = measure(|| {
        let mut frame = build_screen_cached_with_scratch_and_texture_context(
            &actors,
            clear,
            &metrics,
            &fonts,
            0.0,
            &mut legacy_text,
            &mut legacy_scratch,
            &textures,
        );
        black_box(frame.objects.as_slice());
        black_box(frame.sprite_instances.as_slice());
        legacy_scratch.recycle_render_list(&mut frame);
    });
    let (compiled_time, compiled_alloc) = measure(|| {
        let mut frame = scene.emit(&mut compiled_scratch);
        black_box(frame.objects.as_slice());
        black_box(frame.sprite_instances.as_slice());
        compiled_scratch.recycle_render_list(&mut frame);
    });
    let uv_slots = std::array::from_fn::<_, 10, _>(|index| {
        scene
            .sprite_uv_slot(NodeId(index as u32))
            .expect("benchmark primitive is a sprite")
    });
    let mut uv_frame = 0u32;
    let (patched_time, patched_alloc) = measure(|| {
        let patches = std::array::from_fn::<_, 10, _>(|index| SpriteUvPatch {
            slot: uv_slots[index],
            offset: [uv_frame as f32 * 0.001, index as f32 * -0.01],
        });
        uv_frame = uv_frame.wrapping_add(1);
        let mut frame = scene
            .emit_with_uv_patches(&mut compiled_scratch, &patches)
            .expect("benchmark slots remain valid");
        black_box(frame.objects.as_slice());
        black_box(frame.sprite_instances.as_slice());
        compiled_scratch.recycle_render_list(&mut frame);
    });
    let mut direct = scene.compile_draw_frame();
    let mut direct_uv_frame = 0u32;
    let (direct_time, direct_alloc) = measure(|| {
        let patches = std::array::from_fn::<_, 10, _>(|index| SpriteUvPatch {
            slot: uv_slots[index],
            offset: [direct_uv_frame as f32 * 0.001, index as f32 * -0.01],
        });
        direct_uv_frame = direct_uv_frame.wrapping_add(1);
        direct
            .apply_uv_patches(&patches)
            .expect("benchmark slots remain valid");
        black_box(direct.frame().ops.as_slice());
        black_box(direct.frame().sprite_instances.as_slice());
    });

    let legacy_ns = legacy_time.as_nanos() as f64 / ITERS as f64;
    let compiled_ns = compiled_time.as_nanos() as f64 / ITERS as f64;
    let patched_ns = patched_time.as_nanos() as f64 / ITERS as f64;
    let direct_ns = direct_time.as_nanos() as f64 / ITERS as f64;
    println!("compiled presentation benchmark: sprites={SPRITES} iterations={ITERS}");
    println!(
        "legacy compose:  {:>10.0} ns/frame allocs={} reallocs={} bytes={}",
        legacy_ns, legacy_alloc.allocs, legacy_alloc.reallocs, legacy_alloc.bytes,
    );
    println!(
        "compiled emit:   {:>10.0} ns/frame allocs={} reallocs={} bytes={}",
        compiled_ns, compiled_alloc.allocs, compiled_alloc.reallocs, compiled_alloc.bytes,
    );
    println!(
        "compiled +10 UV: {:>10.0} ns/frame allocs={} reallocs={} bytes={}",
        patched_ns, patched_alloc.allocs, patched_alloc.reallocs, patched_alloc.bytes,
    );
    println!(
        "retained +10 UV: {:>10.0} ns/frame allocs={} reallocs={} bytes={}",
        direct_ns, direct_alloc.allocs, direct_alloc.reallocs, direct_alloc.bytes,
    );
    println!(
        "compiled/legacy: {:>9.2}% ({:.2}x faster)",
        compiled_ns / legacy_ns * 100.0,
        legacy_ns / compiled_ns,
    );
}
