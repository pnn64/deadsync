# Simply Love layout tests and PNG exports

These commands target `deadsync-theme-simply-love`, not the main DeadSync binary
or the shell crate. Cargo still builds the theme's dependencies (including the
renderer). Run commands from the repository root.

## Export gameplay screenshots

```powershell
cargo run -p deadsync-theme-simply-love --example export_gameplay
```

The exporter generates PNGs of the actual Simply Love gameplay presentation,
using an isolated synthetic chart and sample gameplay statistics. It does not
read or modify your saved player settings. No song pack or audio device is
required. A desktop session and a Vulkan-capable GPU are required; the renderer
uses a hidden window.

By default, every resolution preset is exported for P1, P2, versus, and doubles:

| Preset | Pixels | Aspect |
| --- | --- | --- |
| `vga` | 640 x 480 | 4:3 |
| `svga` | 800 x 600 | 4:3 |
| `xga` | 1024 x 768 | 4:3 |
| `720p` | 1280 x 720 | 16:9 |
| `1080p` | 1920 x 1080 | 16:9 |
| `16-10` | 1920 x 1200 | 16:10 |
| `1440p` | 2560 x 1440 | 16:9 |
| `ultrawide` | 2560 x 1080 | 64:27 |
| `uwqhd` | 3440 x 1440 | 43:18 |
| `portrait` | 1080 x 1920 | 9:16 |

List presets without initializing graphics:

```powershell
cargo run -p deadsync-theme-simply-love --example export_gameplay -- --list-presets
```

Choose a subset, or supply custom pixel dimensions:

```powershell
cargo run -p deadsync-theme-simply-love --example export_gameplay -- --preset 720p --preset ultrawide
cargo run -p deadsync-theme-simply-love --example export_gameplay -- --size 1366x768 --size 1600x900
```

Custom dimensions normally determine the display aspect. To inspect a different
logical aspect at the same pixel resolution, append `@A:B`. For example,
`640x480@16:9` renders a widescreen layout into a 640 x 480 PNG:

```powershell
cargo run -p deadsync-theme-simply-love --example export_gameplay -- --size 640x480@16:9
```

Select layouts, reverse scrolling, and an output directory:

```powershell
cargo run -p deadsync-theme-simply-love --example export_gameplay -- --preset 1080p --layout p1 --layout versus --reverse --output target\sl-preview
```

`--preset`, `--size`, and `--layout` may be repeated. Specifying resolutions
replaces the default resolution list; specifying layouts replaces the default
layout list. Valid layouts are `p1`, `p2`, `versus`, and `double`.

### Output

The default destination is `<workspace-root>\target\gameplay-exports`. Relative
`--output` paths are also resolved from the workspace root, regardless of
whether you start the exporter with `cargo run` or `cargo test`:

```text
target\gameplay-exports\
  index.html
  1280x720-16x9\
    p1\gameplay.png
    p2\gameplay.png
    versus\gameplay.png
    double\gameplay.png
```

Open `index.html` for a gallery, or inspect individual PNGs at full resolution.
Reverse exports use `gameplay-reverse.png`. Rerunning overwrites matching PNGs
and regenerates the gallery for that run; unrelated or older exports are not
deleted.

### Enabled HUD

The preview enables all step-statistics widgets, including the density graph,
peak NPS, banners, song information, duration, step counts, and judgment counts.
It also enables FA+/EX presentation, timing/error displays, live timing stats,
life percentage, scorebox, cues, countdowns, run timer, and a subtractive mini
indicator. Sample judgment, combo, life, offset, and heart-rate data are supplied.
The preview also creates a synthetic pack banner and registers it in an isolated
song catalog, so the Pack Info image is included above the pack title.

Normal gameplay visibility rules still apply. For example, step statistics are
not forced onto unsupported aspect ratios, and widgets requiring external score
or hardware data do not acquire that data. Mutually exclusive display modes are
represented by one selection, not superimposed. This is a layout inspection
tool, not a pixel-golden assertion suite or a replay of a real score.

## Export-only snapshot tests

Snapshot exports are opt-in: ordinary `cargo test` runs do not run the exporter.
Run it explicitly with the command below (or with `cargo run --example` as shown
above). Selecting all targets with `cargo test --all-targets` also explicitly
includes examples and can run the exporter.

The snapshot command only exports PNG files and the gallery for manual visual
inspection. It does not compare actual images against expected images, golden
files, or left/right values:

```powershell
cargo test -p deadsync-theme-simply-love --example export_gameplay
```

This runs the same exporter as `cargo run`, including graphics initialization.
It fails only if exporting fails (for example, invalid arguments, unavailable
graphics, or a file-write error). Pass exporter options after `--`:

```powershell
cargo test -p deadsync-theme-simply-love --example export_gameplay -- --preset 720p --layout p1
```

## Separate automated layout checks

These are ordinary assertion-based tests, separate from the export-only
snapshots:

Run the focused step-statistics layout tests:

```powershell
cargo test -p deadsync-theme-simply-love --test step_stats_layout
```

The layout tests cover side-pane anchors and notefield separation at 4:3 and
16:9, plus portrait aspect gating. For all theme tests:

```powershell
cargo test -p deadsync-theme-simply-love
```

Exporter source: [export_gameplay.rs](../examples/export_gameplay.rs).
Layout tests: [step_stats_layout.rs](step_stats_layout.rs).
