# Frame-Statistics Overlay

DeadSync ships a live, on-screen timing HUD modeled on
[osu!framework's FrameStatistics display](https://github.com/ppy/osu-framework).
It surfaces the per-frame timing and audio/display sync telemetry the engine
already captures, so you can *watch* a stutter happen instead of digging through
logs after the fact.

This document explains **what the overlay displays** and — the part that trips
people up — **how to read the colored graph**.

---

## Hotkeys

| Shortcut | Action |
| --- | --- |
| `Ctrl`+`F3` | Toggle the overlay on/off. It opens in the top-right corner, or wherever you last moved it. |
| `Ctrl`+`Shift`+`F3` | Move the overlay to the next corner. Remembered across toggles and restarts. |
| `Ctrl`+`Alt`+`F3` | Switch between the **detailed** and **minimal** presentations (see [Presentation styles](#presentation-styles)). Remembered across restarts. |

The overlay costs nothing when it's off: the per-frame samples are only captured
while it's enabled, and there's no heap allocation on the hot path. It never
touches note timing, scoring, or music sync.

---

## Anatomy of the panel

```
+-------------------------------------------------------+
|  ||| | ||  |        per-frame column graph            |   (1)
|  ----------------       target / 2x ref lines         |   (2)
+-------------------------------------------------------+
|  .:||||:.               jitter histogram              |   (3)
+--------+---------+---------+---------+-----------------+
| FRAME  | LOAD    | STUTTER | DISPLAY | AUDIO           |   (4)
| STATS  | cpu/gpu | hitches | CLOCK   |                 |
+--------+---------+---------+---------+-----------------+
```

1. **Column graph** — one column per frame, newest on the right.
2. **Reference lines** — fixed horizontal yardsticks over the graph.
3. **Jitter histogram** — distribution of recent frame times (detailed style only).
4. **Readout cells** — five text panels: FRAME STATS, LOAD, STUTTER, DISPLAY CLOCK, AUDIO.

The **graph** is the main event. Every vertical column is one rendered frame,
newest on the right, scrolling left as time passes. The **height** of a column is
how long that frame took — taller is slower. The column is split into colored
**segments** showing *where* that time went.

---

## The graph colors

Each column is a stack of phases, drawn from the **bottom up** in the order the
engine executes them. The total height is the whole frame interval.

| Color | Segment | What it measures |
| --- | --- | --- |
| ⬛ **Dark gray** | `idle` | Headroom — the part of the frame spent waiting for vsync / not doing work. **Big gray = healthy** (you have spare time). |
| 🟦 **Light blue** | `input` | Draining and handling debounced input events. |
| 🟩 **Green** | `update` | Game-logic update: screen transitions, gameplay stepping, animation. |
| 🟨 **Yellow** | `compose` | Building the actor/draw list for the frame (composing the scene). |
| 🟧 **Orange** | `upload` | Streaming textures, generated assets, and video frames to the GPU. |
| 🟥 **Red** | `draw` | Recording and submitting the render commands. |
| 🟪 **Purple** | `gpu-wait` | CPU blocked waiting on the GPU / swapchain to be ready. |

Reading it at a glance:

- **Mostly dark gray columns** → frames finishing early with lots of idle
  headroom. This is what you want.
- **A column suddenly grows a tall colored band** → that phase spiked. The color
  tells you the culprit: a tall **orange** band is a texture-upload hitch, a tall
  **purple** band is the GPU/swapchain making the CPU wait, a tall **green** band
  is game logic, etc.
- **A wall of color with little gray** → frames are using most of their budget;
  you're close to dropping frames.

### Marker lines (vertical, full-height)

Some columns are overdrawn with a solid full-height vertical bar to flag an event
on that frame:

| Color | Marker | Meaning |
| --- | --- | --- |
| 🟡 **Amber bar** | catch-up | The display clock was actively *catching up* on this frame (resyncing to audio after falling behind). osu!'s GC-marker analog. |
| 🔴 **Red bar** | spike | The frame blew well past the graph scale (≥ ⅔ of full height) — a dropped/long frame. |

### Reference lines (horizontal)

Two faint horizontal lines give the eye a **fixed yardstick** so you read jitter
against a stable baseline instead of an auto-scaled one:

| Color | Line | Meaning |
| --- | --- | --- |
| 🟢 **Green line** | target | The monitor's frame budget (refresh period — e.g. ~16.7 ms at 60 Hz, ~6.9 ms at 144 Hz). Columns reaching this line took a full refresh interval. |
| 🟠 **Orange line** | 2× target | The "stutter" threshold — two refresh intervals. A column crossing this almost certainly dropped a frame. |

A line is hidden when it would fall off the top of the current graph scale.

---

## The jitter histogram (detailed style)

Below the graph, the light-blue **histogram** shows the *distribution* of recent
frame times: the x-axis is frame-time buckets (fast on the left, slow on the
right) and bar height is how often frames landed in that bucket.

- A **single tall spike** = consistent frame pacing (most frames take the same
  time). This is healthy.
- A **wide smear or a second hump on the right** = inconsistent pacing / jitter.

This panel is the osu! philosophy of "the graph *is* the jitter display," so the
**minimal** presentation drops it (see below).

---

## The five readout cells

| Cell | Field | Meaning |
| --- | --- | --- |
| **FRAME STATS** | `### FPS` | Smoothed frames per second. |
| | `avg X ±Y ms` | EWMA-smoothed mean frame time ± jitter (standard deviation). |
| | `p99 Z ms` | 99th-percentile frame time from a decaying histogram — a *stable* tail number (detailed only). |
| | `max W ms` | Worst recent frame, slow-decay held (spike marker analog). |
| | `tgt T ms` | Target frame time (the green reference line). |
| **LOAD** | `cpu X ms` | Smoothed CPU work per frame (input → draw, excluding GPU wait). |
| | `gpu X ms` | Smoothed GPU / swapchain wait per frame. |
| | `idle N%` | Headroom — share of the frame spent idle (the dark-gray band). High = lots of spare time. |
| | `lim CPU/GPU/none` | What's limiting the frame: the largest slice. `none` = idle dominates, so you're frame-capped (vsync), not bound by CPU or GPU. |
| **STUTTER** | `over-budget N` | Frames in the recent window that crossed the 2× stutter threshold (the orange line) — a count of visible hitches. |
| | `catch-ups N` | Distinct display-clock resync events in the window (a multi-frame catch-up counts once). |
| | `worst X ms` | Worst recent frame, slow-decay held (same as FRAME STATS `max`). |
| **DISPLAY CLOCK** | `err ±X ms` | Live display-clock error: how far the visual clock is from audio. `n/a (menu)` outside gameplay. |
| | `p99 Z ms` | Stable 99th-percentile of the error magnitude (detailed only). |
| | `jit Y ms` | Smoothed jitter (std dev) of the error. |
| | `catch-up yes/no` | Whether the clock is currently resyncing. |
| **AUDIO** | `gap X ms` | EWMA-smoothed audio callback interval (≈ the device period; steady = healthy). |
| | `underruns N` | Count of audio output underruns (any increase = an audible dropout). |
| | `out X ms` | Estimated audio output delay (device period + stream latency + queued frames). |
| | `q N` | Frames currently queued to the audio device. |

> **Why are the numbers smoothed?** Raw per-frame values jitter every frame and
> are impossible to read. The averages use an EWMA, the percentiles use a
> decaying histogram refreshed a few times a second, and `max` is slow-decay
> held — so the text stays steady while the graph shows the raw per-frame detail.

---

## Presentation styles

`Ctrl`+`Alt`+`F3` flips between two styles (the underlying stats keep running in
both, so switching is instant and the percentiles stay warm):

| | **Detailed** (default) | **Minimal** |
| --- | --- | --- |
| Per-phase graph + reference lines | ✓ | ✓ |
| Jitter histogram | ✓ | dropped (the graph is the jitter display) |
| `p99` readouts (frame + display) | ✓ | dropped |
| `avg ± jitter`, `max`, `tgt` | ✓ | ✓ |
| Panel height | full | shorter |

**Detailed** keeps DeadSync's richer, stabilized telemetry (a trustworthy p99
plus the histogram). **Minimal** mirrors osu!framework more closely: no
percentiles, no histogram — you read jitter straight off the graph against the
reference lines.

---

## A worked example

> Columns are mostly dark gray and short, hugging well below the green line —
> then one column shoots up past the orange line with a tall **purple** band and
> a **red** spike bar, the histogram grows a small second hump on the right, and
> `max` jumps to 33 ms.

Read: frames are normally healthy (lots of idle headroom), but one frame stalled
waiting on the GPU (`gpu-wait`), long enough to drop a frame (red spike, past 2×
target). The histogram's second hump and the held `max` confirm it was a real
outlier, not noise. If `underruns` also ticked up, the stall was long enough to
starve audio.

---

## Where this comes from (for contributors)

- Rendering + layout + the streaming statistics math:
  `src/screens/components/shared/frame_stats_overlay.rs` (pure functions, unit
  tested).
- Per-frame sample capture, smoothing, and the toggle plumbing:
  `src/app/mod.rs` (`record_frame_stats_sample`, `push_frame_stats_overlay`,
  the `Ctrl`+`F3` family of handlers).
- Data sources reused as-is: display-clock health
  (`src/game/gameplay.rs`) and audio output timing
  (`crates/deadsync-audio*`).

The colors, segment order, marker thresholds, and reference lines are all defined
as constants at the top of `frame_stats_overlay.rs`; this document tracks those.
