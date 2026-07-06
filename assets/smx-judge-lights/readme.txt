smx-judge-lights/ -- per-panel judgement GIFs for StepManiaX pads
==================================================================

This folder holds per-panel animations: GIFs that light up a single panel in
response to a gameplay event (a judged tap, a mine, a held freeze/roll,
etc), drawn on top of (or instead of) whatever full-pad background is
currently showing (see smx-pad-lights/readme.txt).

Full details, including GIF encoding format, marker rows, and the loop /
outro region for sustains: docs/stepmaniax.md, section 11 ("Pad light GIF
animations"). This file is a quick-reference summary of pack layout and
fallback rules.


DIRECTORY LAYOUT
----------------

    smx-judge-lights/
      common/
        common/           <- shipped default pack (DeadSync always ships this)
      dance/
        none/             <- shipped "off switch" pack (gifpack.ini only)
        <your-pack>/      <- your own pack goes here
          gifpack.ini      <- optional metadata (see below)
          miss_25.gif
          great_25.gif
          ...

The shipped dance/none pack is empty apart from a gifpack.ini declaring
Fallback = "none", so every name resolves to nothing. Select it in Options
to turn judgement gifs off entirely without touching the Panel Lights
toggle (background gifs keep working if their own pack is set normally).

Only add new packs under dance/. Don't edit common/common/ directly --
future updates to DeadSync may overwrite it. A dance/ pack that reuses a
shipped pack's name replaces that pack: any file it supplies wins outright
over the shipped one of the same name and size.


FILENAME CONVENTION
--------------------

    {name}_{size}.gif

  {name}  -- one of the judgement names listed below.
  {size}  -- "25" (25-LED pads) or "16" (16-LED pads). DeadSync tries the
             requested size first, then falls back to the other size if only
             one exists. In practice you usually only need to author the
             _25 version -- the pad's firmware only lights the LEDs it
             actually has, so a 25-LED GIF works fine on a 16-LED pad.

Examples:
  fantastic_blue_25.gif
  miss_25.gif
  press_25.gif

Note: unlike smx-pad-lights/ backgrounds, judgement GIFs do NOT support a
@{beats}b{bpm} BPM-variant tag, or a @{grade}/@{difficulty} tag -- there's
only ever one GIF per name per size per pack. A judgement fires once (or
loops) per event regardless of song tempo or grade, so there's nothing to
pick a tempo- or grade-specific variant by.


JUDGEMENT NAMES (what you can author)
----------------------------------------

  fantastic_blue    Fantastic judgment, FA+ (blue) timing window
  fantastic_white   Fantastic judgment, standard (white) timing window
  excellent         Excellent judgment
  great             Great judgment
  decent            Decent judgment
  way_off           Way Off judgment
  miss              Miss judgment
  mine              Mine explosion
  ok                Freeze/roll/lift held successfully (plays on release)
  bad               Freeze/roll/lift dropped
  freeze            Looping sustain while a freeze note is held
  roll              Looping sustain while a roll note is held
  press             Generic press feedback: any panel press with no note of
                    its own. Plays underneath judgement/sustain gifs during
                    gameplay (so a real hit always overrides it), and fires
                    on every raw panel press outside gameplay/practice for
                    tactile menu feedback.

You don't need to author every name. Anything you don't supply falls
through to the chain described below.


FALLBACK ORDER
---------------

For any given judgement name, DeadSync resolves through this chain:

  1. Your selected pack's own GIF for this name
  2. If your pack declares a Fallback pack (gifpack.ini): that pack's GIF
  3. The built-in common/common pack's GIF for this name  <-- automatic

Step 3 happens automatically for every pack -- you only need to author the
judgements you actually want to customize; everything else quietly uses
common/common's version. Unlike a missing background (which can reasonably
show nothing), a judgement with literally no gif and no fallback would give
NO pad feedback at all for that event, so this automatic step matters more
here than it does for smx-pad-lights/. See gifpack.ini below for how to opt
specific judgements (or the whole pack) out of this if you deliberately want
some events to show nothing.


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
                                                fallback -- every judgement
                                                this pack doesn't supply shows
                                                nothing instead of pulling
                                                from common. Omitting the key
                                                is the default: no extra pack
                                                to try, but the automatic
                                                common fallback still applies.

  CanBeEmpty   comma-separated judgement names These specific judgements never
               e.g. "miss, ok, bad"            fall back to anything (not
                                                Fallback, not common) when your
                                                pack doesn't supply them --
                                                they show nothing for that
                                                judgement specifically, while
                                                every other judgement still
                                                falls back normally.

Example:

    # gifpack.ini
    Fallback = "cool-pack"
    CanBeEmpty = "miss, ok, bad"

This says: if a judgement isn't found in my pack, try "cool-pack" first, then
common -- except "miss", "ok", and "bad", which should show nothing if I
haven't provided them (never borrow cool-pack's or common's version). Use
CanBeEmpty when you deliberately want certain events to stay dark rather than
showing a mismatched style borrowed from another pack.

The common pack never needs a gifpack.ini -- it's the terminal fallback and
has nothing further to fall back to.

Fallback and CanBeEmpty declared here only affect smx-judge-lights/
(judgements). A pack of the same name in smx-pad-lights/ (see that folder's
readme.txt) is entirely independent and needs its own gifpack.ini if you
want the same behavior there.

Note: there is no MatchColorToDifficulty, MergeCommonBPMVariants, or
MergeFallbackBPMVariants key here. MatchColorToDifficulty (recoloring a
grayscale gif to match the played chart's difficulty) and the two
Merge*BPMVariants keys (pooling BPM-tagged variants across packs) only make
sense for full-pad backgrounds -- judgement gifs don't carry a BPM tag at
all (see FILENAME CONVENTION above). All three are smx-pad-lights/-only --
see that folder's readme.txt. Declaring any of them in a smx-judge-lights/
gifpack.ini has no effect.

Chains declared via Fallback are at most two deep (your pack, then one
Fallback pack) before the automatic common fallback kicks in. If you want to
mix GIFs from more than one non-common pack, copy the files you want directly
into your own pack folder instead.


GIF FORMAT (brief)
--------------------

Per-panel canvases: 7x8 pixels for 25-LED pads, or 4x5 for 16-LED pads (both
with a trailing marker row), or bare 7x7 / 4x4 canvases which loop the whole
sequence with no separate outro. The 7x7 canvas is a staggered LED grid: an
LED sits only where x and y share parity. See docs/stepmaniax.md section 11a
for the full pixel-level layout, and the stepmaniax-gif-maker tool for an
editor that previews directly on a connected pad.
