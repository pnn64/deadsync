# Independent ICC profiles

Eight byte-exact sRGB/sGrey profiles from
[Compact-ICC-Profiles](https://github.com/saucecontrol/Compact-ICC-Profiles),
commit `bdd84663061bc4ae95ca70decff54f581e27f702`, downloaded 2026-10-26.
`manifest.json` records pinned source URLs, sizes and SHA-256 hashes.
The author releases these files under CC0; the original license is included.

The corpus covers ICC v2.1 and v4.2, RGB and grayscale data, shared curve data and
compact v2 description-tag storage. The v4.4 rules added in ICC.1:2022 must not
be applied indiscriminately to earlier profile layouts. Tests also reverse tag
table record order, which is unconstrained, without changing profile payloads.

Native validation checks container framing, tag bounds and the PNG-specific
data color-space match. It preserves tag payloads; it does not implement ICC
color transforms or certify the semantics of every profile tag.
