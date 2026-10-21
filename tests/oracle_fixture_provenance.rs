use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

fn repository_file(relative: &str) -> PathBuf {
    let path = Path::new(relative);
    assert!(
        path.components()
            .all(|part| matches!(part, Component::Normal(_))),
        "capture dependency must be repository-relative: {relative}"
    );
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn json_file(relative: &str) -> Value {
    let path = repository_file(relative);
    let mut bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{relative}: {error}"));
    if path.extension().is_some_and(|extension| extension == "zst") {
        let mut decoded = Vec::new();
        zstd::stream::read::Decoder::new(bytes.as_slice())
            .unwrap()
            .read_to_end(&mut decoded)
            .unwrap();
        bytes = decoded;
    }
    serde_json::from_slice(&bytes).unwrap_or_else(|error| panic!("{relative}: {error}"))
}

fn assert_hash(relative: &str, expected: &Value) {
    let path = repository_file(relative);
    let mut bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{relative}: {error}"));
    // Matches scripts/fixture_capture_hash.py. Git can check text out as CRLF;
    // compressed captures and binary resources retain their exact-byte hashes.
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            [
                "json", "lua", "sm", "ssc", "ini", "txt", "xml", "vert", "frag", "glsl", "patch",
                "rs", "py",
            ]
            .contains(&extension.to_ascii_lowercase().as_str())
        })
    {
        let mut normalized = Vec::with_capacity(bytes.len());
        for (index, &byte) in bytes.iter().enumerate() {
            if byte != b'\r' || bytes.get(index + 1) != Some(&b'\n') {
                normalized.push(byte);
            }
        }
        bytes = normalized;
    }
    let actual = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        actual,
        expected.as_str().expect("recorded capture hash"),
        "{relative} changed: regenerate its capture with the pinned harness"
    );
}

fn assert_song_resources_are_recorded(value: &Value, song_root: &Path, dependencies: &[Value]) {
    match value {
        Value::String(value) => {
            if let Some(relative) = value.strip_prefix("song:/") {
                let path = song_root.join(relative);
                if path.is_file() {
                    let path = path.canonicalize().unwrap();
                    assert!(
                        dependencies.iter().any(|file| {
                            repository_file(file["path"].as_str().unwrap())
                                .canonicalize()
                                .unwrap()
                                == path
                        }),
                        "capture resource is missing its input hash: {}",
                        path.display()
                    );
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                assert_song_resources_are_recorded(value, song_root, dependencies);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                assert_song_resources_are_recorded(value, song_root, dependencies);
            }
        }
        _ => {}
    }
}

#[test]
fn primary_gameplay_observers_pin_native_source_and_archived_inputs() {
    let contract = json_file("tests/fixtures/oracle-contract.json");
    let group = &contract["native_gameplay_controls"];
    let requests = json_file(group["requests"].as_str().unwrap());
    let captures = json_file(group["captures"].as_str().unwrap());
    assert_eq!(captures["review_status"], "accepted");
    for key in ["harness_version", "harness_revision"] {
        assert_eq!(requests[key], contract[key]);
        assert_eq!(captures[key], contract[key]);
    }
    assert_eq!(
        requests["native_revision"],
        contract["primary_itgmania_revision"]
    );
    assert_eq!(captures["native_revision"], requests["native_revision"]);
    assert_eq!(
        captures["native_executable_sha256"],
        requests["native_executable_sha256"]
    );
    assert_hash(
        group["requests"].as_str().unwrap(),
        &captures["request_manifest_sha256"],
    );
    assert_hash(
        requests["archive"].as_str().unwrap(),
        &requests["archive_sha256"],
    );
    let recipes = requests["fixtures"].as_array().unwrap();
    let entries = captures["fixtures"].as_array().unwrap();
    assert_eq!((recipes.len(), entries.len()), (2, 2));
    for (recipe, entry) in recipes.iter().zip(entries) {
        for (key, value) in recipe.as_object().unwrap() {
            assert_eq!(&entry[key], value);
        }
        assert_hash(
            recipe["observer"].as_str().unwrap(),
            &entry["observer_sha256"],
        );
        assert_hash(recipe["output"].as_str().unwrap(), &entry["output_sha256"]);
        let capture = json_file(recipe["output"].as_str().unwrap());
        assert_eq!(capture["native_source"], entry["native_source"]);
        assert_eq!(
            capture["native_source"]["revision"],
            requests["native_revision"]
        );
        assert_eq!(capture["native_source"]["modified_src"], false);
        for key in ["note_display_sha256", "cubic_spline_sha256"] {
            assert_eq!(
                capture["native_source"][key],
                requests["native_source_sha256"][key]
            );
        }
        assert_eq!(capture["harness_version"], contract["harness_version"]);
        assert_eq!(
            capture["native_exe_sha256"],
            requests["native_executable_sha256"]
        );
        assert_eq!(capture["simfile_sha256"], requests["simfile_sha256"]);
        assert_eq!(capture["lua_sha256"], requests["lua_sha256"]);
        assert_eq!(capture["observer_lf_sha256"], entry["observer_sha256"]);
        assert_eq!(capture["raw_capture_sha256"], entry["raw_capture_sha256"]);
        assert_eq!(capture["inputs"], requests["inputs"]);
        assert_eq!(capture["observed_inputs"]["headless_graphics"], true);
        assert_eq!(capture["observed_inputs"]["window_visible"], false);
        for (observed, requested, tolerance) in [
            (
                "constant_update_delta_seconds",
                "fixed_update_delta_seconds",
                1e-9,
            ),
            ("global_offset_seconds", "global_offset", 1e-7),
        ] {
            assert!(
                (capture["observed_inputs"][observed].as_f64().unwrap()
                    - requests["inputs"][requested].as_f64().unwrap())
                .abs()
                    < tolerance
            );
        }
        assert!(
            capture["frames"].as_array().unwrap().len()
                >= recipe["minimum_frames"].as_u64().unwrap() as usize
        );
        if recipe.get("cap_windows").is_some() {
            let caps = capture["caps"].as_array().unwrap();
            assert_eq!(caps.len(), 2);
            for (cap, window) in caps.iter().zip(recipe["cap_windows"].as_array().unwrap()) {
                let beat = cap["beat"].as_f64().unwrap();
                assert!(
                    (window[0].as_f64().unwrap()..=window[1].as_f64().unwrap()).contains(&beat)
                );
                assert_eq!(cap["top"].as_array().unwrap().len(), 6);
                assert_eq!(cap["bottom"].as_array().unwrap().len(), 9);
            }
        }
    }
    let group = &contract["native_notefield_controls"];
    let requests = json_file(group["requests"].as_str().unwrap());
    let captures = json_file(group["captures"].as_str().unwrap());
    assert_eq!(captures["review_status"], "captured-long-replay-deferred");
    assert_eq!(captures["harness_version"], contract["harness_version"]);
    assert_eq!(captures["harness_revision"], contract["harness_revision"]);
    assert_eq!(
        captures["native_revision"],
        contract["primary_itgmania_revision"]
    );
    assert_hash(
        group["requests"].as_str().unwrap(),
        &captures["request_manifest_sha256"],
    );
    assert_hash(
        requests["archive"].as_str().unwrap(),
        &requests["archive_sha256"],
    );
    for entry in captures["fixtures"].as_array().unwrap() {
        assert_hash(entry["output"].as_str().unwrap(), &entry["output_sha256"]);
        let capture = json_file(entry["output"].as_str().unwrap());
        assert_eq!(capture["input"]["native_source"], entry["native_source"]);
        assert_eq!(
            capture["input"]["native_source"]["revision"],
            contract["primary_itgmania_revision"]
        );
        assert_eq!(capture["input"]["native_source"]["modified_src"], false);
        assert_eq!(
            capture["provenance"]["simfile_sha256"],
            requests["simfile_sha256"]
        );
        assert_eq!(capture["input"]["window_visible"], false);
        assert_eq!(capture["input"]["constant_update_delta_seconds"], 0);
        assert_eq!(capture["dropped_drawable_samples"], 0);
        assert_eq!(capture["dropped_model_geometry_samples"], 0);
        assert_eq!(
            capture["model_geometry_samples"].as_array().unwrap().len(),
            entry["models"].as_u64().unwrap() as usize
        );
        assert_eq!(
            capture["drawable_samples"].as_array().unwrap().len(),
            entry["strips"].as_u64().unwrap() as usize
        );
    }
    for historical in contract["historical_captures"].as_array().unwrap() {
        assert_hash(historical["path"].as_str().unwrap(), &historical["sha256"]);
    }
}

#[test]
fn primary_bitmap_inputs_and_adapter_are_pinned() {
    let contract = json_file("tests/fixtures/oracle-contract.json");
    assert_hash(
        "tests/fixtures/itgmania-primary-bitmaps/capture.json",
        &contract["primary_bitmap_capture_sha256"],
    );
    let capture = json_file("tests/fixtures/itgmania-primary-bitmaps/capture.json");
    assert_eq!(capture["schema_version"], 1);
    assert_eq!(capture["harness_revision"], contract["harness_revision"]);
    assert_eq!(
        capture["primary_codec_revision"],
        contract["primary_itgmania_revision"]
    );
    assert_eq!(
        capture["native_support_revision"],
        contract["itgmania_revision"]
    );
    assert_eq!(
        capture["harness_version"],
        format!(
            "{}+primary-codecs",
            contract["harness_version"].as_str().unwrap()
        )
    );
    assert_eq!(capture["support_proofs"].as_array().unwrap().len(), 17);
    assert_hash(
        "tests/fixtures/itgmania-primary-bitmaps/build-adapter.patch",
        &capture["adapter_patch_sha256"],
    );
    let cases = capture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 35);
    for case in cases
        .iter()
        .chain(capture["header_only"].as_array().unwrap())
    {
        assert_hash(case["source"].as_str().unwrap(), &case["source_sha256"]);
    }
    assert_eq!(capture["header_only"].as_array().unwrap().len(), 2);
}

#[test]
fn refreshed_captures_match_pinned_harness_and_committed_inputs() {
    let contract = json_file("tests/fixtures/oracle-contract.json");
    assert_eq!(contract["schema_version"], 1);
    let pixel_exports =
        json_file("tests/fixtures/itgmania-song-lua-micro/native-pixel-exports.json");
    assert_eq!(pixel_exports["schema_version"], 1);
    for key in ["harness_version", "harness_revision"] {
        assert_eq!(pixel_exports[key], contract[key]);
    }
    let pixel_exports = pixel_exports["exports"].as_array().unwrap();
    let mut exported_paths = BTreeSet::new();
    let mut exported_bytes = 0;
    let mut outputs = BTreeSet::new();
    for group in contract["groups"].as_array().unwrap() {
        let version = group
            .get("harness_version")
            .unwrap_or(&contract["harness_version"]);
        let request_path = group["requests"].as_str().unwrap();
        let requests = json_file(request_path);
        let captures = json_file(group["captures"].as_str().unwrap());
        assert_eq!(captures["schema_version"], 1);
        assert_eq!(captures["hash_encoding"], "sha256-lf-text-v1");
        let executable_hash = captures["harness_binary_sha256"].as_str().unwrap();
        assert_eq!(executable_hash.len(), 64);
        assert!(executable_hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(
            &requests["harness_version"], version,
            "request {request_path}"
        );
        assert_eq!(
            &captures["harness_version"], version,
            "capture {request_path}"
        );
        assert_eq!(requests["harness_revision"], contract["harness_revision"]);
        assert_eq!(captures["harness_revision"], contract["harness_revision"]);
        if group.get("driver").is_some() {
            assert_eq!(
                requests["native_support_revision"],
                contract["itgmania_revision"]
            );
            assert_eq!(
                captures["native_support_revision"],
                contract["itgmania_revision"]
            );
            assert_eq!(
                requests["primary_codec_revision"],
                contract["primary_itgmania_revision"]
            );
            assert_eq!(
                captures["primary_codec_revision"],
                contract["primary_itgmania_revision"]
            );
            assert_eq!(captures["failures"], serde_json::json!([]));
            assert_eq!(captures["review_status"], "accepted");
            for (path_key, hash_key) in [
                ("driver", "driver_sha256"),
                ("cli_patch", "cli_patch_sha256"),
                ("native_setup_patch", "native_setup_patch_sha256"),
            ] {
                assert_hash(group[path_key].as_str().unwrap(), &captures[hash_key]);
            }
        } else {
            assert_eq!(requests["itgmania_revision"], contract["itgmania_revision"]);
        }
        assert_hash(request_path, &captures["request_manifest_sha256"]);
        let entries = captures["fixtures"].as_array().unwrap();
        let recipes = requests["fixtures"].as_array().unwrap();
        assert!(!recipes.is_empty());
        assert_eq!(
            entries.len(),
            recipes.len(),
            "all requested controls must be captured"
        );
        for (recipe, entry) in recipes.iter().zip(entries) {
            for (key, value) in recipe.as_object().unwrap() {
                if key != "dependencies" {
                    assert_eq!(&entry[key], value, "capture recipe changed: {key}");
                }
            }
            let output = entry["output"].as_str().unwrap();
            assert!(
                outputs.insert(output.to_owned()),
                "duplicate capture {output}"
            );
            assert_hash(output, &entry["output_sha256"]);
            if let Some(hash) = entry.get("input_sha256") {
                assert_hash(entry["input"].as_str().unwrap(), hash);
            }
            let dependencies = entry["dependencies"].as_array().unwrap();
            for dependency in dependencies {
                assert_hash(dependency["path"].as_str().unwrap(), &dependency["sha256"]);
            }
            if let Some(required) = recipe["dependencies"].as_array() {
                for path in required {
                    assert!(dependencies.iter().any(|file| file["path"] == *path));
                }
            }
            if let Some(missing) = recipe["absent_dependencies"].as_array() {
                for path in missing {
                    assert!(!repository_file(path.as_str().unwrap()).exists());
                }
            }
            let native = &entry["native_provenance"];
            assert_eq!(
                native["git_revision"], contract["itgmania_revision"],
                "{output}"
            );
            assert_eq!(native["git_dirty"], "false", "{output}");
            let capture = json_file(output);
            if group.get("driver").is_some() {
                assert_song_resources_are_recorded(
                    &capture,
                    &repository_file(recipe["song_root"].as_str().unwrap()),
                    dependencies,
                );
            }
            for export in pixel_exports
                .iter()
                .filter(|export| export["capture"] == output)
            {
                let index = export["case_index"].as_u64().unwrap() as usize;
                assert_eq!(capture["cases"][index]["name"], export["case_name"]);
                let pointer = export["pointer"].as_str().unwrap();
                assert!(pointer.starts_with(&format!("/cases/{index}/")));
                let pixels = capture.pointer(pointer).unwrap().as_array().unwrap();
                let mut expected = Vec::with_capacity(pixels.len() * 4);
                for pixel in pixels {
                    let channels = pixel.as_array().expect("native RGBA pixel");
                    assert_eq!(channels.len(), 4);
                    expected.extend(channels.iter().map(|value| {
                        u8::try_from(value.as_u64().unwrap()).expect("native channel byte")
                    }));
                }
                let path = export["output"].as_str().unwrap();
                assert!(
                    exported_paths.insert(path.to_owned()),
                    "duplicate pixel export {path}"
                );
                let actual =
                    std::fs::read(repository_file(path)).expect("committed native RGBA export");
                assert_eq!(
                    actual, expected,
                    "{path}: regenerate from the current native capture"
                );
                exported_bytes += actual.len();
            }
            if recipe["capture_kind"] == "rejections" {
                assert_eq!(recipe["oracle"], "itgmania_native_actor_rejection");
                let request = json_file(recipe["input"].as_str().unwrap());
                let cases = capture.as_array().expect("native rejection cases");
                let specs = request["texture_files"].as_array().unwrap();
                assert!(!cases.is_empty());
                assert_eq!(cases.len(), specs.len());
                for (case, spec) in cases.iter().zip(specs) {
                    for (key, value) in spec.as_object().unwrap() {
                        assert_eq!(&case[key], value);
                    }
                    assert_eq!(case["exit_code"], 1);
                    assert_eq!(case["error"], request["expected_error"]);
                }
                assert_eq!(native["execution"], "embedded_native_sources");
            } else if let Some(oracle) = recipe.get("oracle") {
                assert_eq!(&capture["oracle"], oracle, "{output}");
                assert_eq!(
                    capture["provenance"]["git_revision"],
                    native["git_revision"]
                );
                assert_eq!(native["execution"], "embedded_native_sources");
            } else {
                assert_eq!(&capture["harness_version"], version, "{output}");
                assert_eq!(
                    capture["oracle"],
                    "itgmania_song_lua_headless_semantic_trace"
                );
                assert_eq!(capture["runtime_errors"], serde_json::json!([]), "{output}");
                assert_eq!(capture["dropped_events"], 0, "{output}");
                assert_eq!(
                    capture["capabilities"]["runtime_complete"], true,
                    "{output}"
                );
                assert!(
                    dependencies
                        .iter()
                        .any(|file| file["path"] == recipe["input"])
                );
                if recipe["kind"] == "context" {
                    let request = json_file(recipe["input"].as_str().unwrap());
                    if let Some(simfile) = request["simfile"].as_str() {
                        assert_eq!(capture["arrow_timing"], "native", "{output}");
                        assert!(dependencies.iter().any(|file| file["path"] == simfile));
                        assert_eq!(capture["input_adapter"]["simfile_is_symbolic"], false);
                    } else {
                        assert_eq!(capture["arrow_timing"], "linear", "{output}");
                        assert_eq!(capture["input_adapter"]["simfile_is_symbolic"], true);
                    }
                }
            }
        }
    }
    assert_eq!(
        exported_paths.len(),
        pixel_exports.len(),
        "all exports must reference current captures"
    );
    assert_eq!((exported_paths.len(), exported_bytes), (67, 147_648));
}
