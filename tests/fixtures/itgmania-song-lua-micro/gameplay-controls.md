These two direct observers run the unmodified archived 7th Gear Lua in the actual
primary ITGmania `704cdd` build. They observe spline points, evaluated positions,
zoom and submitted hold-cap vertices after native drawing. They do not derive
poses from authored setters or wrap authored functions.

`gameplay-refresh-inputs.json` pins the archive, native executable, source hashes,
clock profile, observer files and selection windows. The current captures retain
142 swap frames and 70 approach frames. The two cap pairs retain their complete
six-vertex top and nine-vertex bottom submissions. Native preferences serialize
the nominal 120 Hz fixed update as 0.008333 seconds; the captures record that
actual value and the -8 ms global offset. `gameplay-capture-manifest.json` retains
the output, raw observation and observer hashes.

Regenerate into a new staging directory:

```powershell
python -B -X utf8 scripts/refresh_native_gameplay_controls.py --harness ../itgmania-harness/target/debug/itgmania-harness-rs.exe --harness-source ../itgmania-harness --native-source ../itgmania-harness/target/native-engine-source --itgmania-bin ../itgmania-harness/target/native-engine-source/Program/ITGmania-harness-trace.exe --output-root target/native-gameplay-refresh
```

The official harness prepares a private offscreen install from the committed
archive. The script then runs the two repository observers there and checks the
actual source, window and preference inputs. Raw observations remain in the
staging directory for review. Only native arrays are selected for the normalized
fixtures; vertices and spline values are never edited.

After replacing the staged outputs, all 480 active notefield library tests pass,
including both recorded-clock spline evaluation and cap-row comparisons. All
three provenance checks pass as well. Ordinary tests need only repository data,
without the original song folder, a native executable or either source checkout.
