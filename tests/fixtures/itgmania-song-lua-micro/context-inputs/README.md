# Short context capture inputs

These JSON requests make the former temporary capture contexts reproducible.
They name repository Lua roots, layer order, sampling limits, BPM segments and
the random seed. Thirteen original linear-clock controls use an explicitly
symbolic simfile. Controls that previously used native chart timing instead
name a real committed simfile; the refresh command rejects loss of that native
timing.

Thirty requests use existing SM/SSC files. The 29 small SSC files in `simfiles`
make formerly uncommitted timing inputs explicit: zero offset, the recorded BPM
map, and one four-column Challenge measure. The JSON supplies the Lua entry
roots independently of those charts. Stop and offset controls use their existing
authored simfiles. Ordinary tests need only these repository inputs and the
recorded captures, without a harness executable or a sibling checkout.

The input driver calls the current harness evaluator. Its isolated build also
uses the primary engine's image codecs and fixes one harness setup issue: chart
scoring and the noteskin manager must share the same native game identity.
Otherwise loading a real chart makes valid skin setters fail and modifier
equality queries disagree with the native options. Engine implementations are
unchanged. The explicit adapter version, binary hash, driver hash and patches
are retained in the capture receipt; it is not the official unmodified harness.

Rebuild in a new output directory, then stage captures for numerical review:

```powershell
python -B -X utf8 scripts/build_primary_oracle_adapter.py --harness-source ../itgmania-harness --itgmania-source ../itgmania --output target/oracle-refresh --codecs-only
python -B -X utf8 scripts/install_oracle_micro_adapter.py --adapter-root target/oracle-refresh --harness-source ../itgmania-harness --itgmania-source ../itgmania
python -B -X utf8 scripts/refresh_micro_context_fixtures.py --adapter target/oracle-refresh/micro-adapter.json --output-root target/oracle-captures
```

Generation stages an unreviewed receipt. Numerical parity tests must pass before
the outputs and an accepted receipt replace their repository references.
