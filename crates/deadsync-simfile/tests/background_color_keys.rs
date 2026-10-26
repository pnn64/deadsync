use deadsync_simfile::changes::extract_background_lua_changes;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "deadsync-background-color-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("effect.lua"), b"return Def.ActorFrame{}").unwrap();
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0.join("effect.lua"));
        let _ = fs::remove_dir(&self.0);
    }
}

#[test]
fn every_color_channel_keeps_distinct_owners_and_merges_equivalent_encodings() {
    let fixture = Fixture::new();
    for column in [9, 10] {
        for component in 0..4 {
            let mut records = Vec::with_capacity(512);
            let mut expected = Vec::with_capacity(256);
            for value in 0..=u8::MAX {
                let mut rgba = [17, 85, 170, 255];
                rgba[component] = value;
                expected.push(rgba.map(|byte| f32::from(byte) / 255.0));
                let [r, g, b, a] = rgba;
                let hex = if a == 255 {
                    format!("#{r:02x}{g:02x}{b:02x}")
                } else {
                    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
                };
                let decimal = expected
                    .last()
                    .unwrap()
                    .map(|value| value.to_string())
                    .join("^");
                for (duplicate, color) in [hex, decimal].into_iter().enumerate() {
                    let mut fields = [
                        (usize::from(value) * 2 + duplicate).to_string(),
                        "effect.lua".to_owned(),
                        "1".to_owned(),
                        "0".to_owned(),
                        "0".to_owned(),
                        "1".to_owned(),
                        "UpperLeft".to_owned(),
                        String::new(),
                        String::new(),
                        "#FFFFFF".to_owned(),
                        String::new(),
                    ];
                    fields[column] = color;
                    records.push(fields.join("="));
                }
            }
            let source = format!("#BGCHANGES:{};", records.join(","));
            let owners = extract_background_lua_changes(fixture.path(), source.as_bytes(), "");
            assert_eq!(owners.len(), 256, "column {column}, component {component}");
            for (index, owner) in owners.iter().enumerate() {
                assert_eq!(owner.activations.len(), 2);
                assert_eq!(owner.activations[0].start_beat, (index * 2) as f32);
                assert_eq!(owner.activations[1].start_beat, (index * 2 + 1) as f32);
                assert_eq!(owner.effect, "UpperLeft");
                let color = if column == 9 {
                    expected[index]
                } else {
                    [1.0; 4]
                };
                assert_eq!(
                    owner.color1.map(|rgba| rgba.map(f32::to_bits)),
                    Some(color.map(f32::to_bits))
                );
            }
        }
    }
}
