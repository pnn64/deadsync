smx-pad-lights/ -- full-pad background GIFs for StepManiaX pads
================================================================

This folder holds full-pad background animations: GIFs that light up the
entire pad (all 9 panels at once) based on whatever screen is currently
active (menus, song select, gameplay, results, etc).

Full details, including GIF encoding format, marker rows, and BPM-variant
tagging: docs/stepmaniax.md, section 11 ("Pad light GIF animations").
This file is a quick-reference summary of pack layout and fallback rules.


DIRECTORY LAYOUT
----------------

    smx-pad-lights/
      common/
        common/           <- shipped default pack (DeadSync always ships this)
      dance/
        none/             <- shipped "off switch" pack (gifpack.ini only)
        <your-pack>/      <- your own pack goes here
          gifpack.ini      <- optional metadata (see below)
          default_25.gif
          gameplay_25.gif
          ...

The shipped dance/none pack is empty apart from a gifpack.ini declaring
Fallback = "none", so every role resolves to nothing. Select it in Options
to turn background gifs off entirely without touching the Panel Lights
toggle (judgement gifs keep working if their own pack is set normally).

Only add new packs under dance/. Don't edit common/common/ directly --
future updates to DeadSync may overwrite it. A dance/ pack that reuses a
shipped pack's name replaces that pack: any file it supplies wins outright
over the shipped one of the same name and size.

Per-song and per-pack overrides are also supported: DeadSync checks for a
smx-pad-lights/ folder inside a song's own folder, and inside that song's
parent song-pack folder, before falling back to the pack selected in
Options. See docs/stepmaniax.md section 11b for details.


FILENAME CONVENTION
--------------------

    {role}_{size}[@{beats}b{bpm}].gif           <- background with optional BPM tag
    {role}_{size}[@{difficulty}][@{grade}].gif  <- results background, difficulty and/or grade

  {role}  -- one of the role names listed below.
  {size}  -- "25" (25-LED pads) or "16" (16-LED pads). DeadSync tries the
             requested size first, then falls back to the other size if only
             one exists. In practice this means you usually only need to
             author the _25 version -- the pad's firmware only lights the
             LEDs it actually has, so a 25-LED GIF works fine on a 16-LED pad.
  @{beats}b{bpm}   -- BPM variant tag. {beats} is the loop length in beats,
             {bpm} is the reference tempo. Lets you author several
             tempo-locked variants of the same role; DeadSync picks the one
             with the smallest reference BPM at or above the song's tempo
             (the densest gif that still fits the pad's 30fps cap), NOT the
             nearest one (see "WHERE BPM TAGGING ACTUALLY MATTERS" below --
             it's only meaningful for a couple of roles).
  @{difficulty}    -- results-only: the chart's actual difficulty, lowercase:
             beginner, easy, medium, hard, challenge, or edit. This is the
             chart's real file-level difficulty, not a display name -- e.g. a
             Challenge chart shown on-screen as "Novice" or "Expert" (a
             per-song display convention) still tags its gif "challenge".
  @{grade}   -- grade suffix for results backgrounds (e.g. @S+, @B-, @*****).
  A results file can combine both, difficulty first: @{difficulty}@{grade}.

Examples:
  default_25.gif
  gameplay_25@4b120.gif
  results_25.gif
  results_25@S+.gif
  results_25@*****.gif
  results_25@hard.gif
  results_25@hard@S+.gif
  results_25@edit@*****.gif


ROLE NAMES (what you can author)
---------------------------------

  default          All screens not covered by a more specific role; also the
                   ultimate fallback role if nothing else resolves.
  gameplay         During a song (Gameplay screen).
  song_select      Song/course selection screen.
  results          Evaluation/results screen (no grade-specific GIF found).
  results@*****    Results screen, Quint (5-star) grade.
  results@****     Results screen, Quad (4-star) grade.
  results@***      Results screen, Triple (3-star) grade.
  results@**       Results screen, Double (2-star) grade.
  results@*        Results screen, Single (1-star) grade.
  results@S+ / S / S-
  results@A+ / A / A-
  results@B+ / B / B-
  results@C+ / C / C-
  results@D
  results@F

Grade-specific roles fall back gracefully so you only have to author what you
want to customize, e.g.:

  results@S+  -->  results@S  -->  results  -->  default
  results@A-  -->  results@A  -->  results  -->  default
  results@*****  -->  results  -->  default   (no base-letter fallback)

You don't need to author every role. Anything you don't supply falls
through to the chain described below.

DIFFICULTY TAGGING (results@<difficulty>@<grade>)
----------------------------------------------------

Any results@<grade> role can also be qualified by the difficulty of the
chart that earned the grade, e.g. results_25@hard@S+.gif. Difficulty slots
into the existing grade chain as an extra tier tried before the
difficulty-agnostic role at each grade level, so packs that don't care about
difficulty keep working exactly as before:

  results@hard@S+  -->  results@S+  -->  results@hard@S  -->  results@S
    -->  results@hard  -->  results  -->  default

That is: exact difficulty+grade, then the plain grade file (unchanged from
before difficulty tagging existed), then the same two steps for the grade's
base letter (e.g. S for S+/S-), then a difficulty-only file with no grade,
then plain "results", then "default". You only need to author the specific
combinations you want to differentiate -- e.g. just results_25@hard@F.gif to
give Hard-chart fails a distinct look; everything else keeps resolving
however it already did.

If you'd rather not hand-paint a separately-colored file per difficulty at
all, see MatchColorToDifficulty under "gifpack.ini" below -- it recolors one
grayscale gif automatically instead.


WHERE BPM TAGGING ACTUALLY MATTERS
-------------------------------------

The @{beats}b{bpm} tag is only meaningful for two roles:

  gameplay        DeadSync knows the current song's BPM throughout the song.
  song_select     DeadSync knows the highlighted song's BPM.

The two roles use the tag differently. On song select the animation is
BEAT-LOCKED: DeadSync ignores the GIF's own frame delays and maps the music
preview's live beat straight to a frame (one loop pass spans the tagged beat
count), so it follows the music exactly. During gameplay the tag only picks
WHICH variant plays; the gif then runs in real time at its encoded frame
delays, so author each gameplay variant's delays to match its reference tempo
if you want it to sit near the beat.

Variant selection picks the smallest reference BPM at or above the song's
tempo, not the nearest: a 130-BPM song with 129- and 225-BPM variants plays
the 225 one (the 129 variant would have to run faster than its reference to
keep up, overshooting the pad's 30fps cap). A song faster than every variant
plays the highest-reference one at half speed.

Every other role (default, options, results, and every results@... grade
and/or difficulty variant) always resolves with no song BPM available --
there's no "current song" concept on those screens. If you author several
@{beats}b{bpm} variants of one of those roles, DeadSync doesn't error, but it
deterministically always uses the lowest-reference-BPM variant, every time
-- the others just sit unused. So:

  - gameplay_25@4b120.gif, gameplay_25@4b240.gif, etc: useful, DeadSync
    picks between them by the actual song's tempo.
  - results_25@4b120.gif plus results_25@4b240.gif: pointless, only the
    120-BPM one would ever be picked.

If you want several tempo variants of a results/default/options role for
some other reason (e.g. hand-picking one yourself outside DeadSync's normal
flow), an untagged file works the same as a single tagged one -- just don't
expect DeadSync to pick among multiple.

BPM VARIANTS DON'T POOL ACROSS PACKS BY DEFAULT. If your pack supplies ANY
song_select (or gameplay) gif at all, common's variants for that role are
ignored entirely, even ones that would fit a song's tempo better -- each
pack's variant list for a role is considered on its own, all-or-nothing. See
MergeCommonBPMVariants / MergeFallbackBPMVariants under gifpack.ini below if
you want your pack's variants pooled with common's (and/or your Fallback
pack's) instead.


FALLBACK ORDER
---------------

For any given role, DeadSync resolves through this chain:

  1. Per-song smx-pad-lights/ folder (if the current song ships one)
  2. Per-song-pack smx-pad-lights/ folder (if the song's pack ships one)
  3. Your selected pack's own GIF for this role
  4. If your pack declares a Fallback pack (gifpack.ini): that pack's GIF
  5. The built-in common/common pack's GIF for this role  <-- automatic
  6. Your selected pack's "default" GIF
  7. If your pack declares a Fallback pack: that pack's "default" GIF
  8. common/common's "default" GIF

Step 5 (and 8) happen automatically for every pack -- you only need to
author the roles you actually want to customize; everything else quietly
uses common/common's version. See gifpack.ini below for how to opt out of
this if you want a role to show nothing (solid black) instead.


gifpack.ini (optional pack metadata)
--------------------------------------

Place an optional gifpack.ini file inside your pack folder
(dance/<your-pack>/gifpack.ini) to adjust the fallback behavior above. It's a
plain "Key = "value"" file, one setting per line. Keys use CamelCase,
matching deadsync.ini's convention. Unknown keys and comment lines
(starting with #) are ignored.

  Key          Values                          Effect
  -----------  ------------------------------  --------------------------------
  Fallback     any pack name, or "none"        Try the named pack (both sizes)
                                                before falling back to common.
                                                "none" opts the WHOLE pack out
                                                of the automatic common
                                                fallback -- every role this
                                                pack doesn't supply shows solid
                                                black instead of pulling from
                                                common. Omitting the key is the
                                                default: no extra pack to try,
                                                but the automatic common
                                                fallback still applies.

  CanBeEmpty   comma-separated role names      These specific roles never fall
               e.g. "results, song_select"     back to anything (not Fallback,
                                                not common) when your pack
                                                doesn't supply them -- they
                                                show solid black for that role
                                                specifically, while every other
                                                role still falls back normally.

  MatchColorToDifficulty                       Whatever gif actually resolves
               comma-separated base role       for that role gets recolored to
               names, e.g. "results"           match the played chart's
                                                difficulty color. See
                                                "DIFFICULTY COLOR MATCHING"
                                                below.

  MergeCommonBPMVariants                       Pool this pack's BPM-tagged
               comma-separated base role       variants for that role with
               names, e.g. "song_select"       common's, instead of using
                                                only this pack's own. See
                                                "MERGING BPM VARIANTS ACROSS
                                                PACKS" below.

  MergeFallbackBPMVariants                     Same as MergeCommonBPMVariants,
               comma-separated base role       but pools with the declared
               names                           Fallback pack's variants
                                                instead. No-op if no Fallback
                                                is declared.

Example:

    # gifpack.ini
    Fallback = "cool-pack"
    CanBeEmpty = "results, song_select"

This says: if a role isn't found in my pack, try "cool-pack" first, then
common -- except "results" and "song_select", which should just show black
if I haven't provided them (never borrow cool-pack's or common's version).

The common pack never needs a gifpack.ini -- it's the terminal fallback and
has nothing further to fall back to.

Fallback, CanBeEmpty, MatchColorToDifficulty, MergeCommonBPMVariants, and
MergeFallbackBPMVariants declared here only affect smx-pad-lights/ (full-pad
backgrounds). A pack of the same name in smx-judge-lights/ (see that
folder's readme.txt) is entirely independent and needs its own gifpack.ini
if you want the same behavior there (though MatchColorToDifficulty and the
two Merge*BPMVariants keys only have any effect here, in smx-pad-lights/ --
judgement gifs don't support any of them).

Chains declared via Fallback are at most two deep (your pack, then one
Fallback pack) before the automatic common fallback kicks in. If you want to
mix GIFs from more than one non-common pack, copy the files you want directly
into your own pack folder instead.


DIFFICULTY COLOR MATCHING (MatchColorToDifficulty)
-------------------------------------------------------

Rather than hand-authoring a separately-colored gif per difficulty (see
DIFFICULTY TAGGING above), you can author ONE grayscale gif -- every pixel
R=G=B, varying only in brightness -- and have DeadSync recolor it
automatically to match whichever difficulty the player is looking at results
for.

    # gifpack.ini
    MatchColorToDifficulty = "results"

Whatever gif actually wins the normal results resolution chain (plain
results, a grade-specific file, a difficulty-specific file, whatever you've
authored) gets recolored before it's sent to the pad: each pixel's R, G, B
values are multiplied by the target difficulty color's R, G, B (each scaled
to 0.0-1.0). White pixels become the target color exactly, black stays
black, grays become dimmed versions of the target color.

The target color is the SAME theme-relative color the rest of the UI already
uses for difficulty: Challenge = your current theme color; Hard, Medium,
Easy, Beginner step backward around the same color wheel one step at a time;
Edit is a fixed grey. So a Hard-difficulty result tints differently
depending on what theme color you've picked, but always consistent with how
difficulty is colored everywhere else in the game.

The target color is also adapted for the pad LEDs before the multiply: the
theme palette is sRGB (made for screens) and the LEDs are linear, so raw
palette bytes would look washed-out near-white on the pad. DeadSync
gamma-expands the color so it comes out vivid; author your grayscale art
normally and don't compensate for this yourself.

THIS ONLY WORKS CORRECTLY ON GRAYSCALE SOURCE ART. If your gif has actual
color in it, the same per-channel multiply still runs, but the result
generally will NOT look like a clean recolor -- a red pixel tinted toward
blue doesn't become a different shade of blue, it becomes mostly black (red's
green and blue channels multiplied by blue's target color both land near
zero). There's no way for DeadSync to tell which color in your gif is the
"primary" one to shift, so this feature assumes lightness-only source art.

Recoloring is computed once per (pack, role, difficulty) combination and
cached -- not per frame -- and only recomputed when the player's theme color
changes, so it costs nothing during normal gameplay.


MERGING BPM VARIANTS ACROSS PACKS (MergeCommonBPMVariants / MergeFallbackBPMVariants)
------------------------------------------------------------------------------------------

By default, BPM variants don't pool across packs (see "WHERE BPM TAGGING
ACTUALLY MATTERS" above) -- if your pack supplies any song_select (or
gameplay) gif at all, common's variants for that role are ignored entirely,
even ones that would fit a song's tempo better. These two keys opt a role
into pooling instead:

    # gifpack.ini
    Fallback = "cool-pack"
    MergeCommonBPMVariants = "song_select"
    MergeFallbackBPMVariants = "song_select"

  - MergeCommonBPMVariants pools this pack's variants for the listed role(s)
    with common's.
  - MergeFallbackBPMVariants pools with the declared Fallback pack's variants
    instead (a no-op if no Fallback is declared).
  - Both, as above: all three sources (your pack, the Fallback pack, and
    common) are pooled into one set, and DeadSync picks whichever variant
    best fits the song's actual BPM from the combined set.
  - Neither (the default): only your pack's own variants are considered.

Each key is independent and only affects the role names it lists -- a role
not listed in either key keeps the default (own-pack-only) behavior even if
the pack sets one of these keys for a DIFFERENT role.

EXACT BPM-TAG COLLISIONS STILL RESPECT THE NORMAL PRECEDENCE. If two sources
happen to author the same reference BPM for a role (say your pack and common
both have a @2b129 variant), pooling doesn't create a duplicate or pick
arbitrarily -- your pack's own variant always wins, then the Fallback pack's,
then common's, exactly the same precedence order as the regular (non-merged)
resolution chain.


GIF FORMAT (brief)
--------------------

Full-pad canvases: 23x24 pixels for 25-LED pads, or 14x15 for 16-LED pads.
Each of the 9 panels is a block in a 3x3 grid with 1px gaps; an extra bottom
row carries loop/outro markers. See docs/stepmaniax.md section 11a for the
full pixel-level layout, and the stepmaniax-gif-maker tool for an editor that
previews directly on a connected pad.
