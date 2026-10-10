# Model alpha cutoff

actor-input.json and native.json use compiled ITGmania Actor and Model
sources through actor-conformance, independently of the Lua actor adapter.
control.lua declares matching DeadSync Models. Eleven cases cover zero,
values immediately below and above 0.001, the exact threshold, glow-only
output and mixed diffuse/glow alpha. The model_alpha_cutoff test compares
the production composer's mesh pass counts to the native observations.

Model.cpp:345-350 skips drawing only when both diffuse and glow alpha are
strictly below 0.001. The shared DeadSync Model builder checks the final
colors after inheritance and effects. Tween values remain observable.
