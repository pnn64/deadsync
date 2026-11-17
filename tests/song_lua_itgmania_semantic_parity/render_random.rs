//! Raw native rendering randomness, bound to independently owned actor slots.
//! No expected position, camera, magnitude, scale, or vertex is a replay input.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
struct Input {
    scene: u64,
    update: u64,
    owner: String,
    path: Option<String>,
    source_actor: Option<String>,
    source_path: Option<String>,
    wrapper: i64,
    values: [f32; 3],
}

#[derive(Default)]
pub(super) struct Replay {
    inputs: Vec<Input>,
    used: BTreeSet<usize>,
}

pub(super) fn validate(trace: &Value) -> Result<(), String> {
    if trace["capabilities"]["native_render_random_inputs"] != true {
        if !trace["input"]["render_random"].is_null()
            || trace["render_random_inputs"].as_array().is_some_and(|v| !v.is_empty())
            || trace["drawable_samples"].as_array().is_some_and(|samples| samples.iter()
                .any(|sample| sample.get("render_random_sequences").is_some()))
        {
            return Err("render random inputs have no native capability".into());
        }
        return Ok(());
    }
    Replay::parse(trace).map(|_| ())
}

impl Replay {
    pub(super) fn parse(trace: &Value) -> Result<Self, String> {
        let provenance = &trace["input"]["render_random"];
        if trace["capabilities"]["native_render_random_inputs"] != true
            || trace["capabilities"]["native_drawable_vertices"] != true
            || provenance["protocol"] != 1
            || provenance["observation"] != "native-Actor-PreDraw-vibrate-randomf"
            || provenance["generator"] != "std::mt19937"
            || provenance["distribution"] != "uniform_real_distribution<double>(-1,1)-to-f32"
            || provenance["seed_source"] != "native-default-time"
            || provenance["association"] != "active-Actor-Draw-PreDraw-scopes"
            || trace["dropped_render_random_inputs"] != 0
        {
            return Err("invalid native render random provenance or dropped inputs".into());
        }
        for key in ["actor_sha256", "random_numbers_sha256", "random_numbers_header_sha256"] {
            if !trace["input"]["native_source"][key].as_str().is_some_and(|hash|
                hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
            {
                return Err("native render random source hashes are missing or invalid".into());
            }
        }
        let records = trace["render_random_inputs"].as_array()
            .ok_or("missing native render random input records")?;
        let frames = trace["update_frames"].as_array().ok_or("missing render random frame anchors")?;
        let actors = trace["runtime_actors"].as_array().ok_or("missing native random actor identities")?;
        let creations = trace["actor_creation_inputs"].as_array().ok_or("missing native random factory identities")?;
        let mut actor_by_id = BTreeMap::new();
        for actor in actors {
            let id = actor["id"].as_str().filter(|id| !id.is_empty())
                .ok_or("native random actor identity is missing")?;
            if actor_by_id.insert(id, actor).is_some() {
                return Err("duplicate native random actor identity".into());
            }
        }
        let identity_matches = |record: &Value, id_key: &str, definition_key: &str, path_key: &str| {
            record[id_key].as_str().and_then(|id| actor_by_id.get(id)).is_some_and(|actor| {
                record[definition_key] == actor["definition_id"]
                    && record[path_key] == actor["path"]
                    && record[path_key].as_str().is_some_and(|path| !path.is_empty())
            })
        };
        let mut inputs = Vec::with_capacity(records.len());
        let mut identities = BTreeSet::new();
        let mut native_scopes = BTreeSet::new();
        let mut draw_roles = BTreeSet::new();
        let mut draw_owners = BTreeMap::new();
        let mut draw_occurrences = BTreeMap::new();
        let mut previous = 0;
        for (index, record) in records.iter().enumerate() {
            let update = record["update_sequence"].as_u64().unwrap_or(0);
            let scene = record["scene"].as_u64().unwrap_or(0);
            let owner = record["actor"].as_str().filter(|id| !id.is_empty())
                .ok_or("render random input has no actor identity")?;
            let wrapper = record["wrapper_index"].as_i64().filter(|&v| v >= -1)
                .ok_or("invalid render random wrapper role")?;
            let actor = actor_by_id.get(owner)
                .ok_or("render random owner has no native runtime identity")?;
            if !identity_matches(record, "actor", "definition_id", "path")
                || !actor["definition_id"].is_null() && !creations.iter().any(|creation|
                    creation["actor"] == record["actor"] && creation["scene"] == record["scene"]
                        && creation["definition_id"] == actor["definition_id"])
                || wrapper == 0 && record["source_actor"] != record["actor"]
            {
                return Err("render random actor provenance does not match native identity".into());
            }
            if record["source_actor"].is_null() {
                if wrapper <= 0 || !record["source_definition_id"].is_null() || !record["source_path"].is_null() {
                    return Err("render random source identity is incomplete".into());
                }
            } else if !identity_matches(record, "source_actor", "source_definition_id", "source_path") {
                return Err("render random source provenance does not match native identity".into());
            }
            let draw = record["native_draw_sequence"].as_u64().filter(|&v| v > 0)
                .ok_or("missing native render draw occurrence")?;
            let native_update = record["native_update_sequence"].as_u64().filter(|&v| v > 0)
                .ok_or("missing native render update occurrence")?;
            let native_scope = record["native_scope_sequence"].as_u64().filter(|&v| v > 0)
                .ok_or("missing native PreDraw scope identity")?;
            let draw_ordinal = record["draw_ordinal"].as_u64().filter(|&v| v > 0)
                .ok_or("missing native owner draw ordinal")?;
            let predraw = record["predraw_ordinal"].as_u64().filter(|&v| v > 0)
                .ok_or("missing native PreDraw occurrence")?;
            if record["sequence"].as_u64() != Some(index as u64 + 1)
                || update == 0 || update < previous || update > frames.len() as u64
                || scene == 0 || frames[update as usize - 1]["scene"].as_u64() != Some(scene)
                || !identities.insert((draw, predraw)) || !native_scopes.insert(native_scope)
                || !draw_roles.insert((draw, wrapper))
            {
                return Err("invalid, reordered, or duplicate render random occurrence".into());
            }
            let draw_owner = (owner, native_update, draw_ordinal);
            if draw_owners.insert(draw, draw_owner).is_some_and(|previous| previous != draw_owner)
                || draw_occurrences.insert(draw_owner, draw).is_some_and(|previous| previous != draw)
            {
                return Err("native render draw occurrence has conflicting owner or ordinal".into());
            }
            let values = record["values"].as_array().filter(|v| v.len() == 3)
                .ok_or("render random input must contain XYZ")?;
            let mut xyz = [0.0; 3];
            for (out, value) in xyz.iter_mut().zip(values) {
                let value = value.as_f64().filter(|v| v.is_finite() && (-1.0..=1.0).contains(v))
                    .ok_or("invalid native random sample")?;
                *out = value as f32;
                if f64::from(*out) != value { return Err("native random sample is not an exact f32".into()); }
            }
            inputs.push(Input {
                scene, update, owner: owner.into(), wrapper, values: xyz,
                path: record["path"].as_str().map(str::to_owned),
                source_actor: record["source_actor"].as_str().map(str::to_owned),
                source_path: record["source_path"].as_str().map(str::to_owned),
            });
            previous = update;
        }
        let samples = trace["drawable_samples"].as_array().ok_or("missing random draw associations")?;
        let mut referenced = BTreeSet::new();
        let mut first_use = 0;
        for sample in samples {
            let refs = sample["render_random_sequences"].as_array().ok_or("missing draw random scope list")?;
            let mut unique = BTreeSet::new();
            let mut previous_id = 0;
            for id in refs {
                let id = id.as_u64().and_then(|id| usize::try_from(id).ok())
                    .filter(|&id| id > 0 && id <= inputs.len()).ok_or("invalid draw random reference")?;
                let input = &inputs[id - 1];
                if id <= previous_id || !unique.insert(id) || sample["scene"].as_u64() != Some(input.scene)
                    || sample["update_sequence"].as_u64() != Some(input.update)
                {
                    return Err("draw random scope has a duplicate or mismatched frame".into());
                }
                previous_id = id;
                if referenced.insert(id) {
                    first_use += 1;
                    if id != first_use { return Err("render random inputs are not in first-use order".into()); }
                }
            }
        }
        if referenced.len() != inputs.len() { return Err("unreferenced native render random inputs".into()); }
        let boundaries = trace["drawable_frames"].as_array().filter(|v| !v.is_empty())
            .ok_or("missing render random draw boundaries")?;
        let mut consumed = 0;
        let mut previous_update = 0;
        for boundary in boundaries {
            let update = boundary["update_sequence"].as_u64().unwrap_or(0);
            let count = boundary["render_random_inputs"].as_u64()
                .and_then(|n| usize::try_from(n).ok()).ok_or("missing random draw boundary count")?;
            if update == 0 || update < previous_update || update > frames.len() as u64
                || count < consumed || count > inputs.len()
                || boundary["scene"] != frames[update as usize - 1]["scene"]
                || inputs[consumed..count].iter().any(|input|
                    input.update != update || boundary["scene"].as_u64() != Some(input.scene))
            { return Err("invalid render random draw boundary".into()); }
            consumed = count;
            previous_update = update;
        }
        if consumed != inputs.len() { return Err("incomplete render random boundary tail".into()); }
        Ok(Self { inputs, used: BTreeSet::new() })
    }

    pub(super) fn used(&self) -> usize { self.used.len() }
    pub(super) fn total(&self) -> usize { self.inputs.len() }

    pub(super) fn apply(
        &mut self, sample: &Value, bindings: &BTreeMap<String, SongLuaLiveTarget>,
        paths: &BTreeMap<String, String>, targets: &BTreeMap<String, SongLuaLiveTarget>,
        expected: &[(i32, usize)], group: i32, local: &mut [SongLuaOverlayState],
        screen: Option<(usize, &mut SongLuaOverlayState)>,
    ) -> Result<Vec<usize>, String> {
        let refs = sample["render_random_sequences"].as_array().ok_or("missing draw random scopes")?;
        let mut actual = Vec::with_capacity(refs.len());
        let mut ids = Vec::with_capacity(refs.len());
        let mut screen = screen;
        for value in refs {
            let id = value.as_u64().and_then(|v| usize::try_from(v).ok())
                .filter(|&id| id > 0 && id <= self.inputs.len()).ok_or("invalid render random sequence")?;
            let input = &self.inputs[id - 1];
            if sample["scene"].as_u64() != Some(input.scene)
                || sample["update_sequence"].as_u64() != Some(input.update)
            { return Err("render random input belongs to another frame".into()); }
            let owner_path = paths.get(&input.owner).map(String::as_str).or(input.path.as_deref());
            let owner = bindings.get(&input.owner).copied()
                .or_else(|| owner_path.and_then(|path| targets.get(path).copied()))
                .ok_or_else(|| format!("render random owner {} has no independent visual slot", input.owner))?;
            let source = input.source_actor.as_ref().and_then(|id| bindings.get(id)).copied();
            let source_path_target = input.source_path.as_ref().and_then(|path| targets.get(path)).copied();
            let source_owned_target = input.source_actor.as_ref().and_then(|id| paths.get(id))
                .and_then(|path| targets.get(path)).copied();
            let target = if input.wrapper > 0 {
                let path = owner_path.ok_or("render random wrapper owner has no independent path")?;
                targets.get(&format!("{path}/WrapperState{}", input.wrapper)).copied()
            } else if input.wrapper == -1 {
                // A native path cannot substitute for an unidentified source.
                // External sources with supported stable paths were already
                // bound by DrawChecks alongside factory-created song actors.
                Some(source.ok_or("render random fake parent has no independent source binding")?)
            } else { Some(owner) }.ok_or_else(|| format!("render random owner {} has no independent visual slot", input.owner))?;
            if [source, source_path_target, source_owned_target].into_iter().flatten()
                .any(|source| target_slot(source) != target_slot(target))
            {
                return Err("render random source identity contradicts its owned draw role".into());
            }
            let slot = target_slot(target);
            if actual.contains(&slot) { return Err("duplicate vibration scope for one owned actor".into()); }
            actual.push(slot);
            let mut applied = false;
            if slot.0 == group {
                let state = local.get_mut(slot.1).ok_or("render random visual slot out of bounds")?;
                if !state.vibrate { return Err("native vibration is inactive in the owned actor".into()); }
                state.vibrate_input = Some(input.values);
                applied = true;
            }
            if let Some((index, state)) = screen.as_mut() && slot == (0, *index) {
                if !state.vibrate { return Err("native screen vibration is inactive in the owned actor".into()); }
                state.vibrate_input = Some(input.values);
                applied = true;
            }
            if !applied { return Err("render random scope is outside the independently owned ancestry".into()); }
            ids.push(id);
        }
        if actual != expected {
            return Err(format!("native vibration scopes differ from owned draw ancestry: {actual:?} vs {expected:?}"));
        }
        self.used.extend(ids.iter().copied());
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace() -> Value {
        json!({
            "capabilities": {"native_render_random_inputs": true,"native_drawable_vertices":true},
            "input": {"native_source":{"actor_sha256":"a".repeat(64),
                    "random_numbers_sha256":"b".repeat(64),"random_numbers_header_sha256":"c".repeat(64)},
                "render_random": {"protocol": 1,
                "observation": "native-Actor-PreDraw-vibrate-randomf",
                "generator":"std::mt19937","distribution":"uniform_real_distribution<double>(-1,1)-to-f32",
                "seed_source":"native-default-time",
                "association": "active-Actor-Draw-PreDraw-scopes"}},
            "dropped_render_random_inputs": 0,
            "update_frames": [{"scene": 1}],
            "drawable_frames": [{"scene":1,"update_sequence":1,"render_random_inputs":3}],
            "runtime_actors": [
                {"id":"screen", "path":"ScreenGameplay"},
                {"id":"parent", "definition_id":"parent-definition","path":"parent-definition"}
            ],
            "actor_creation_inputs": [{"actor":"parent", "definition_id":"parent-definition","scene":1}],
            "render_random_inputs": [
                {"sequence": 1, "scene": 1, "update_sequence": 1,
                    "actor": "screen", "source_actor":"screen", "path": "ScreenGameplay", "wrapper_index": 0,
                    "source_path":"ScreenGameplay","native_scope_sequence":21,"native_update_sequence":8,
                    "native_draw_sequence": 1, "draw_ordinal": 1, "predraw_ordinal": 1,
                    "values": [0.25, -0.5, 1.0]},
                {"sequence": 2, "scene": 1, "update_sequence": 1,
                    "actor": "parent", "definition_id":"parent-definition","path":"parent-definition","wrapper_index": 1,
                    "native_scope_sequence":22,"native_update_sequence":8,
                    "native_draw_sequence": 2, "draw_ordinal": 1, "predraw_ordinal": 1,
                    "values": [-0.25, 0.5, -1.0]},
                {"sequence": 3, "scene": 1, "update_sequence": 1,
                    "actor": "parent", "definition_id":"parent-definition","path":"parent-definition","wrapper_index": 1,
                    "native_scope_sequence":23,"native_update_sequence":8,
                    "native_draw_sequence": 3, "draw_ordinal": 2, "predraw_ordinal": 1,
                    "values": [0.5, 0.25, 0.0]}
            ],
            "drawable_samples": [
                {"scene": 1, "update_sequence": 1, "render_random_sequences": [1,2]},
                {"scene": 1, "update_sequence": 1, "render_random_sequences": [1,2]},
                {"scene": 1, "update_sequence": 1, "render_random_sequences": [1,3]}
            ]
        })
    }

    #[test]
    fn native_render_random_inputs_reject_missing_reordered_and_invalid_records() {
        let good = trace();
        validate(&good).unwrap();
        for (path, value) in [
            ("/capabilities/native_render_random_inputs", json!(false)),
            ("/capabilities/native_drawable_vertices", json!(false)),
            ("/dropped_render_random_inputs", json!(1)),
            ("/input/render_random/protocol", json!(2)),
            ("/input/render_random/generator", json!("Lua PRNG")),
            ("/input/render_random/distribution", json!("expected-geometry-derived")),
            ("/input/render_random/seed_source", json!("lua-random-seed")),
            ("/input/native_source/actor_sha256", Value::Null),
            ("/input/native_source/random_numbers_sha256", json!("z".repeat(64))),
            ("/input/native_source/random_numbers_header_sha256", json!("c".repeat(63))),
            ("/render_random_inputs/1/values", json!([0.0, 0.0])),
            ("/render_random_inputs/1/values/0", json!(1.01)),
            ("/render_random_inputs/1/values/0", json!(0.1)),
            ("/render_random_inputs/1/sequence", json!(1)),
            ("/render_random_inputs/1/scene", json!(2)),
            ("/render_random_inputs/0/actor", json!("unknown-screen")),
            ("/render_random_inputs/0/source_actor", json!("parent")),
            ("/render_random_inputs/0/source_path", json!("parent-definition")),
            ("/render_random_inputs/0/path", json!("ScreenGameplay/PlayerP1")),
            ("/render_random_inputs/1/path", Value::Null),
            ("/render_random_inputs/1/definition_id", Value::Null),
            ("/render_random_inputs/1/native_draw_sequence", json!(1)),
            ("/render_random_inputs/1/native_update_sequence", json!(0)),
            ("/render_random_inputs/1/native_scope_sequence", json!(21)),
            ("/render_random_inputs/2/draw_ordinal", json!(1)),
            ("/runtime_actors/1/id", json!("screen")),
            ("/actor_creation_inputs/0/definition_id", json!("other-definition")),
            ("/drawable_samples/0/render_random_sequences", json!([2,1])),
            ("/drawable_samples/0/render_random_sequences", json!([1,1])),
            ("/drawable_samples/0/render_random_sequences", json!([1,4])),
            ("/drawable_samples/0/scene", json!(2)),
            ("/drawable_samples/1/render_random_sequences", json!([2,1])),
            ("/drawable_samples/2/render_random_sequences", json!([1,2])),
            ("/drawable_frames/0/render_random_inputs", json!(2)),
            ("/drawable_frames/0/render_random_inputs", json!(4)),
            ("/drawable_frames/0/scene", json!(2)),
        ] {
            let mut bad = good.clone();
            *bad.pointer_mut(path).unwrap() = value;
            assert!(validate(&bad).is_err(), "accepted malformed input at {path}");
        }
        let mut conflicting_owner = good.clone();
        conflicting_owner["render_random_inputs"][1]["native_draw_sequence"] = json!(1);
        conflicting_owner["render_random_inputs"][1]["predraw_ordinal"] = json!(2);
        assert!(validate(&conflicting_owner).is_err(), "one native draw cannot have multiple owners");
        let mut duplicate_role = good.clone();
        duplicate_role["render_random_inputs"][2]["native_draw_sequence"] = json!(2);
        duplicate_role["render_random_inputs"][2]["draw_ordinal"] = json!(1);
        duplicate_role["render_random_inputs"][2]["predraw_ordinal"] = json!(2);
        assert!(validate(&duplicate_role).is_err(), "one native draw role cannot consume two triples");
        let mut later_native_update = good;
        later_native_update["render_random_inputs"][2]["native_update_sequence"] = json!(9);
        later_native_update["render_random_inputs"][2]["draw_ordinal"] = json!(1);
        validate(&later_native_update).unwrap();
    }

    #[test]
    fn native_render_random_capability_cannot_be_removed_from_its_observations() {
        validate(&json!({"drawable_samples":[{"scene":1,"update_sequence":1}]})).unwrap();
        validate(&json!({"capabilities":{"native_render_random_inputs":false},"render_random_inputs":[]})).unwrap();
        for indicator in [
            json!({"input":{"render_random":{"protocol":1}}}),
            json!({"drawable_samples":[{"render_random_sequences":[]}]}),
            json!({"render_random_inputs":[{"sequence":1}]}),
        ] {
            assert!(validate(&indicator).is_err(), "accepted orphan random channel: {indicator}");
        }
    }

    #[test]
    fn native_render_random_fake_parent_requires_matching_bound_source_identity() {
        let mut trace = trace();
        trace["runtime_actors"].as_array_mut().unwrap().push(json!({
            "id":"external-fake","path":"ScreenGameplay/Fake"}));
        for input in trace["render_random_inputs"].as_array_mut().unwrap().iter_mut().skip(1) {
            input["wrapper_index"] = json!(-1);
            input["source_actor"] = json!("external-fake");
            input["source_path"] = json!("ScreenGameplay/Fake");
        }
        validate(&trace).unwrap();
        for (field, value) in [
            ("source_actor", Value::Null),
            ("source_actor", json!("unidentified-fake")),
            ("source_path", json!("ScreenGameplay")),
            ("source_definition_id", json!("fabricated-definition")),
        ] {
            let mut invalid = trace.clone();
            invalid["render_random_inputs"][1][field] = value;
            assert!(validate(&invalid).is_err(), "accepted fake-parent {field}");
        }
        let mut bindings = BTreeMap::from([
            ("screen".into(), SongLuaLiveTarget::Root(0)),
            ("parent".into(), SongLuaLiveTarget::Foreground(0, 1)),
            ("external-fake".into(), SongLuaLiveTarget::Foreground(0, 0)),
        ]);
        let paths = BTreeMap::from([
            ("screen".into(), "ScreenGameplay".into()), ("parent".into(), "1".into()),
            ("external-fake".into(), "ScreenGameplay/Fake".into()),
        ]);
        let targets = BTreeMap::from([
            ("ScreenGameplay".into(), SongLuaLiveTarget::Root(0)),
            ("1".into(), SongLuaLiveTarget::Foreground(0, 1)),
            ("ScreenGameplay/Fake".into(), SongLuaLiveTarget::Foreground(0, 0)),
        ]);
        let mut replay = Replay::parse(&trace).unwrap();
        let mut screen = SongLuaOverlayState { vibrate:true, ..Default::default() };
        let mut local = [screen, SongLuaOverlayState::default()];
        replay.apply(&trace["drawable_samples"][0], &bindings, &paths, &targets,
            &[(0,0), (1,0)], 1, &mut local, Some((0, &mut screen))).unwrap();
        assert_eq!(local[0].vibrate_input, Some([-0.25,0.5,-1.0]));
        bindings.remove("external-fake");
        let mut replay = Replay::parse(&trace).unwrap();
        assert!(replay.apply(&trace["drawable_samples"][0], &bindings, &paths, &targets,
            &[(0,0), (1,0)], 1, &mut local, Some((0, &mut screen))).is_err(),
            "a known path cannot replace the missing source actor binding");
        assert_eq!(replay.used(), 0, "a rejected scope list must not count as replayed");
        bindings.insert("external-fake".into(), SongLuaLiveTarget::Root(0));
        assert!(replay.apply(&trace["drawable_samples"][0], &bindings, &paths, &targets,
            &[(0,0), (1,0)], 1, &mut local, Some((0, &mut screen))).is_err(),
            "a source actor binding cannot contradict its stable path");
    }

    #[test]
    fn native_render_random_wrapper_source_cannot_contradict_its_owner_slot() {
        let mut trace = trace();
        trace["render_random_inputs"][1]["source_actor"] = json!("screen");
        trace["render_random_inputs"][1]["source_path"] = json!("ScreenGameplay");
        validate(&trace).unwrap();
        let bindings = BTreeMap::from([
            ("screen".into(), SongLuaLiveTarget::Root(0)),
            ("parent".into(), SongLuaLiveTarget::Foreground(0, 1)),
        ]);
        let paths = BTreeMap::from([
            ("screen".into(), "ScreenGameplay".into()), ("parent".into(), "1".into()),
        ]);
        let targets = BTreeMap::from([
            ("ScreenGameplay".into(), SongLuaLiveTarget::Root(0)),
            ("1".into(), SongLuaLiveTarget::Foreground(0, 1)),
            ("1/WrapperState1".into(), SongLuaLiveTarget::Foreground(0, 0)),
        ]);
        let mut replay = Replay::parse(&trace).unwrap();
        let mut screen = SongLuaOverlayState { vibrate:true, ..Default::default() };
        let mut local = [screen, SongLuaOverlayState::default()];
        assert!(replay.apply(&trace["drawable_samples"][0], &bindings, &paths, &targets,
            &[(0,0), (1,0)], 1, &mut local, Some((0, &mut screen))).is_err());
        assert_eq!(replay.used(), 0);
    }

    #[test]
    fn native_render_random_scopes_share_parents_and_distinguish_repeated_draws() {
        let trace = trace();
        let mut replay = Replay::parse(&trace).unwrap();
        let bindings = BTreeMap::from([
            ("screen".into(), SongLuaLiveTarget::Root(0)),
            ("parent".into(), SongLuaLiveTarget::Foreground(0, 1)),
        ]);
        let paths = BTreeMap::from([
            ("screen".into(), "ScreenGameplay".into()), ("parent".into(), "1".into()),
        ]);
        let targets = BTreeMap::from([
            ("ScreenGameplay".into(), SongLuaLiveTarget::Root(0)),
            ("1".into(), SongLuaLiveTarget::Foreground(0, 1)),
            ("1/WrapperState1".into(), SongLuaLiveTarget::Foreground(0, 0)),
        ]);
        for (index, wrapper) in [[-0.25, 0.5, -1.0], [-0.25, 0.5, -1.0], [0.5, 0.25, 0.0]].into_iter().enumerate() {
            let mut screen = SongLuaOverlayState { vibrate: true, ..Default::default() };
            let mut local = [screen, SongLuaOverlayState::default()];
            replay.apply(&trace["drawable_samples"][index], &bindings, &paths, &targets,
                &[(0,0), (1,0)], 1, &mut local, Some((0, &mut screen))).unwrap();
            assert_eq!(screen.vibrate_input, Some([0.25,-0.5,1.0]));
            assert_eq!(local[0].vibrate_input, Some(wrapper));
        }
        assert_eq!(replay.used(), 3);
        let mut screen = SongLuaOverlayState { vibrate: true, ..Default::default() };
        let mut local = [screen, SongLuaOverlayState::default()];
        assert!(replay.apply(&trace["drawable_samples"][0], &bindings, &paths, &targets,
            &[(1,0), (0,0)], 1, &mut local, Some((0, &mut screen))).is_err(),
            "scope order must match independently composed parent order");
        local[0].vibrate = false;
        assert!(replay.apply(&trace["drawable_samples"][0], &bindings, &paths, &targets,
            &[(0,0), (1,0)], 1, &mut local, Some((0, &mut screen))).is_err(),
            "raw inputs cannot enable an effect absent from owned state");
    }
}
