# Flip69 base rotation result

The existing complete Flip69 comparison passes **390,119 / 390,119**:
157,716 modifier actor writes, 213,984 multitap zoom checks, 14,465 geometry
checks, 3,758 vibration checks, 190 final render checks and 6 compile/layer
checks. The reference and frozen chart hash `0ea0735edbc2405f` are unchanged.

DeadSync previously aliased base rotation setters to ordinary rotation.
A later `rotationz(0)` erased `baserotationz(90)`. ITGmania's Actor keeps
`m_baseRotation` separate and adds it to the tween/effect rotation in
`Actor::BeginDraw` (local `itgmania/src/Actor.cpp`, lines 684–686).

The replaced aliases are deleted. Base rotation is captured independently
for all three axes, takes effect immediately, and is added once during
draw composition. Lua rotation getters retain the ordinary rotation.
Frame sampling preserves changes in base rotation without interpolating
between immediate writes.

The regression checks getters, dynamic base rotation, the draw pose for
all axes and single application during composition. Existing multitap
boundary and noteskin command tests also pass. All 2,638 formerly failing
rotation writes remain in the complete comparison and now pass.

MAIN is promoted to `0.5.1785` with both Cargo files incremented once.
No downloads or ITGmania windows were used. This retained headless reference
does not establish full gameplay pixels or every native model draw.
