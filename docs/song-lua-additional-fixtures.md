# Spectrum Sequence and Cursed Metamorph reference fixtures

These two locally requested captures are reference data for future parity
work. Native capture status is complete; DeadSync parity has **not** been
audited or fixed. They do not change the frozen project list or its passing
result ledger.

Both captures select dance-single Challenge, enable both players, use seed
1, advance at 60 Hz and retain render samples every 0.125 beat. They use
the local cel noteskin and Love judgment graphic. Capture continues through
the later of the native last note and LASTSECONDHINT. Both have zero Lua
errors and zero dropped events. Compressed bytes have an exact verified
round trip, with source, harness, executable and resource hashes recorded
in the selected manifest.

| Song | Chart hash | Update frames | Final beat | Final seconds |
| --- | --- | --- | --- | --- |
| Spectrum Sequence | `75df84a1b1e08f31` | 8,180 | 249.8933563232422 | 136.30299021055498 |
| Cursed Metamorph | `0866bcc47435a591` | 8,025 | 1093.1666259765625 | 133.71969604492188 |

Spectrum Sequence uses description `BR FS BT XO Rolls JA DS` and Cursed
Metamorph uses `2/1 BU+ FS BR DS`. Spectrum's Edit chart shares its note
hash; the fixture explicitly selects Challenge. Cursed's `saved/` backup
simfile is excluded from discovery. Song files and Lua remain unchanged.

The fixtures are registered in
`tests/fixtures/itgmania-song-lua-selected/_semantic_manifest.json`:

- `Spectrum Sequence/spectrumsequence.ssc.semantic.json.zst`
- `Cursed Metamorph - [Zaia]/Cursed Metamorph.ssc.semantic.json.zst`

Their raw capture SHA-256 values are:

- Spectrum: `62ec3686bbfcc303bf97ab3ef782d2f74f7000e5d5a2f02676f602af9bb743b5`
- Cursed: `e8a747b1b14edb961b5bdaeea0b20150bfa453f3ed6214348da1801be069e206`

The existing legacy Spectrum fixture remains available to its consumers.
Run the new ignored tests when parity work is requested:

```powershell
cargo test --offline -j1 --test song_lua_itgmania_semantic_parity corpora::lua_songs::spectrum_sequence -- --ignored --exact --nocapture
cargo test --offline -j1 --test song_lua_itgmania_semantic_parity corpora::lua_songs::cursed_metamorph -- --ignored --exact --nocapture
```

Only reference fixtures, their manifest, test registrations and this
documentation are added. The fixture-only commit keeps version 0.5.1806;
no new DeadSync passing result is claimed. No downloads or visible
ITGmania process were used.

Validation: the offline integration-test build succeeds and libtest lists
both new tests. The native capture and compression checks above pass.
The existing corpus-wide coverage test fails on missing
`mawaru2/mawaru2.sm`; Mawaru 3 and 4 are absent locally too. Their existing
registrations and fixtures are unchanged. This does not mark the new
captures incomplete, and no parity comparison has been run for them.
