# Native actor color inheritance

ITGmania applies color effects to each actor's local draw state in
`Actor::PreDraw`, then multiplies diffuse by the inherited tint and combines
glow with inherited glow using `own + parent - own * parent`. ActorFrame
passes these resulting colors to its children.

DeadSync previously inherited the parent's original tint, ignored inherited
glow, and applied the leaf's effect after inheritance. Parent rainbow and
diffuse effects were invisible on their children; a leaf rainbow could also
discard ancestor tint. DiffuseShift and DiffuseRamp multiplied the original
RGB instead of replacing it.

The draw-state composition now applies local color effects first, replaces
diffuse gradients when the native effect replaces all four colors, preserves
the original first-corner alpha, and applies ancestor tint and glow afterward.
The compiled local/getter states remain unchanged. A shared color-effect path
replaces the old diffuse multiplication and duplicated color branches.

The existing warmed composition cache now tracks the six diffuse/glow modes
and rainbow in initial states, command blocks, eases, and runtime tracks.
Affected descendants recompose as the clock advances, including seeks, with
no new frame-time allocation or cache pruning. The playback regression checks
clock-only updates, local-state preservation, unaffected actors, and stable
buffer capacities.

`color-inheritance.lua` exercises all seven parent color effects beneath a
tinted/glowing root and above another tinted/glowing frame. Each branch has
a plain child, rainbow child, diffuse-ramp child with an authored gradient,
and glow-shift child. There are 28 sprites, sampled from zero through two
seconds at 30 Hz with a 60 BPM beat clock. The rendered diffuse and glow
passes are compared against actual linked ITGmania Sprite draws: 54,656 RGBA
channel comparisons across 3,416 draw passes and all four native corners.
The comparison permits one byte quantization step because native draw colors
are bytes. Existing comparison tolerances were not widened.

The previous MAIN implementation fails the new fixture: the gradient child
does not produce the two expected native sprite passes. The fixture also
checks the ancestor tint/glow and all effect colors, rather than relying on
raw Lua getter colors. The older semantic host samples raw alpha for projected
visibility and does not independently validate the rendered color effect.

Capture provenance: local ITGmania revision
`5b205125ad53b9867bb4a494ff858f8d38ad4406`, 854 by 480, seed 1.
The linked actor-oracle executable SHA-256 is
`5bba522be095f4ef40272a92dbab07ebfa3e8361fa08a5a3d18565f901a9bbac`.
The raw actor capture SHA-256 is
`6fd50a2091ce28eba728ad4682ec5278679d22a365a594d50b137dcf4ee4d636`.
The compressed fixture was verified by exact round trip.
The checked-in input SHA-256 is `668a28354e90be10fbf28dd01dd6337cdd0d9bb7f4846f568878f625d9b8c38e`.
Reproduce with the local harness's `actor-conformance` command using
`tests/fixtures/itgmania-song-lua-micro/color-inheritance-input.json`.

Final validation passes 781 song-Lua unit tests, 172 playback integration
tests, two AMV tests, and 105 regular semantic tests in both rework and MAIN.
The complete Mawaru8 audit against the retained older trace is
**98,584/98,602**: five speculative input-message probes, twelve fade-boundary
visibility comparisons, and one additional raw-alpha comparison remain.
The additional comparison is def-0431 at beat 520.148: the old semantic
host records raw alpha 1 while the now-applied diffuse effect produces
draw alpha approximately 0.128. Its `actor_zpb` in `lua/twin/default.lua`
selects DiffuseShift with effect alphas 0.1 and 0.3; native rendered alpha
cannot be the recorded raw 1. This reference-color limitation remains
visible; the older trace and its provenance were not rewritten. Whole-song
parity is unfinished. This pass does not change song resources or frozen
project hashes. All resources and reference captures are local.
