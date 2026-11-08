//! Model::LoadPieces reads each declared file independently; errors must not
//! substitute the mesh file's embedded materials or an unrelated texture.
//! These load/reject cases were checked with ITGmania's native actor harness
//! (Model.cpp: LoadPieces and LoadMaterialsFromMilkshapeAscii).
use deadsync_noteskin::{
    ItgModelSlotPlan, actor, itg, itg_load_model_slots, itg_resolve_model_decl,
    model::MODEL_WHITE_TEXTURE,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

const MESH: &str = "Meshes: 1\n\"mine\" 0 0\n3\n\
    0 -1 -1 0 0 0 0\n0 1 -1 0 1 0 0\n0 0 1 0 0 1 0\n\
    1\n0 0 1\n1\n0 0 1 2 0 0 0 1\n";
const BONES: &str =
    "Bones: 1\n\"root\"\n\"\"\n0 0 0 0 0 0 0\n1\n0 0 0 0\n2\n0 0 0 0\n30 0 0 1.570796327\n";

fn materials(color: &str, texture: &str, alpha: &str) -> String {
    format!(
        "Materials: 1\n\"mine nomove\"\n0 0 0 1\n{color}\n0 0 0 1\n0 0 0 1\n0\n1\n\"{texture}\"\n\"{alpha}\"\n"
    )
}

struct Fixture {
    root: PathBuf,
    data: itg::NoteskinData,
}
type Slot = (PathBuf, Option<ItgModelSlotPlan>);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "deadsync-model-source-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("materials")).unwrap();
        fs::write(root.join("mesh.png"), []).unwrap();
        fs::write(root.join("materials/skin.png"), []).unwrap();
        fs::write(
            root.join("_mine meshes.txt"),
            format!("{MESH}{}{BONES}", materials("1 0 0 1", "mesh.png", "")),
        )
        .unwrap();
        fs::write(
            root.join("materials/_mine materials.txt"),
            materials("0 1 0 1", "skin.png", ""),
        )
        .unwrap();
        fs::write(root.join("_mine bones.txt"), BONES.replace("30 0", "60 0")).unwrap();
        let data = itg::NoteskinData {
            name: "pieces".into(),
            metrics: itg::IniData::default(),
            search_dirs: vec![root.clone()],
            overrides: vec![],
        };
        Self { root, data }
    }

    fn declaration(&self) -> actor::ItgLuaModelDecl {
        actor::parse_actor_decl(
            r#"return Def.Model {
            Meshes="_mine meshes.txt";
            Materials="materials/_mine materials.txt";
            Bones="_mine bones.txt";
        }"#,
            &self.data.metrics,
        )
        .models
        .remove(0)
    }

    fn resolve(&self, declaration: actor::ItgLuaModelDecl) -> Vec<Slot> {
        itg_resolve_model_decl(
            &self.data,
            "Down",
            "Tap Mine",
            declaration,
            None,
            None,
            |path| Some((path.to_path_buf(), None)),
            |path, _| Some((path.to_path_buf(), None)),
            |slot, plan| slot.1 = Some(plan),
            |_, _| {},
        )
        .into_iter()
        .map(|slot| slot.slot)
        .collect()
    }

    fn load(&self) -> Result<Vec<Slot>, String> {
        itg_load_model_slots(
            &self.root.join("_mine meshes.txt"),
            &self.root.join("materials/_mine materials.txt"),
            &self.root.join("_mine bones.txt"),
            |path| Some((path.to_path_buf(), None)),
            |slot, plan| slot.1 = Some(plan),
        )
    }

    fn set_materials(&self, source: &str) {
        fs::write(self.root.join("materials/_mine materials.txt"), source).unwrap();
    }

    fn assert_rejected(&self) {
        assert!(
            self.resolve(self.declaration()).is_empty(),
            "noteskin must not substitute another material source"
        );
        assert!(
            self.load().is_err(),
            "shared model loader must reject invalid declared materials"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn declared_materials_and_bones_override_conflicting_mesh_sections() {
    let fixture = Fixture::new();
    for slots in [
        fixture.resolve(fixture.declaration()),
        fixture.load().unwrap(),
    ] {
        assert_eq!(slots.len(), 1);
        let (texture, plan) = &slots[0];
        assert_eq!(texture, &fixture.root.join("materials/skin.png"));
        let plan = plan.as_ref().unwrap();
        assert_eq!(
            plan.model.as_ref().unwrap().material.unwrap().diffuse,
            [0.0, 1.0, 0.0, 1.0]
        );
        assert_eq!(plan.model_auto_rot_total_frames, 60.0);
        assert!(!plan.note_color_translate);
    }
}

#[test]
fn missing_declared_pieces_never_fall_back_to_mesh_file() {
    let fixture = Fixture::new();
    for piece in ["Materials", "Bones"] {
        let mut declaration = fixture.declaration();
        match piece {
            "Materials" => declaration.materials_expr = Some("\"missing materials.txt\"".into()),
            _ => declaration.bones_expr = Some("\"missing bones.txt\"".into()),
        }
        assert!(
            fixture.resolve(declaration).is_empty(),
            "missing {piece} must not reuse mesh contents"
        );
    }
}

#[test]
fn malformed_materials_never_reopen_embedded_materials() {
    let fixture = Fixture::new();
    fixture.set_materials("Materials: 1\n\"broken\"\n\"skin.png\"\n");
    fixture.assert_rejected();
}

#[test]
fn mesh_material_index_must_exist_in_declared_material_file() {
    let fixture = Fixture::new();
    fixture.set_materials("Materials: 0\n// old texture: \"skin.png\"\n");
    fixture.assert_rejected();
}

#[test]
fn missing_diffuse_texture_does_not_use_alpha_texture_as_diffuse() {
    let fixture = Fixture::new();
    fixture.set_materials(&materials("0 1 0 1", "missing.png", "skin.png"));
    fixture.assert_rejected();
}

#[test]
fn missing_alpha_texture_rejects_declared_materials() {
    let fixture = Fixture::new();
    fixture.set_materials(&materials("0 1 0 1", "skin.png", "missing.png"));
    fixture.assert_rejected();
}

#[test]
fn material_textures_do_not_fall_back_to_noteskin_search_directories() {
    let fixture = Fixture::new();
    // mesh.png exists beside the mesh, but not beside the declared materials.
    fixture.set_materials(&materials("0 1 0 1", "mesh.png", ""));
    fixture.assert_rejected();
}

#[test]
fn unused_material_textures_are_still_validated() {
    let fixture = Fixture::new();
    let first = materials("0 1 0 1", "skin.png", "");
    let unused = materials("1 0 0 1", "missing.png", "");
    fixture.set_materials(&format!(
        "{}{}",
        first.replacen("Materials: 1", "Materials: 2", 1),
        unused.strip_prefix("Materials: 1\n").unwrap()
    ));
    fixture.assert_rejected();
}

#[test]
fn blank_diffuse_texture_keeps_material_and_alpha_texture() {
    let fixture = Fixture::new();
    fixture.set_materials(&materials("0 1 0 1", "", "skin.png"));
    for slots in [
        fixture.resolve(fixture.declaration()),
        fixture.load().unwrap(),
    ] {
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].0, PathBuf::from(MODEL_WHITE_TEXTURE));
        let plan = slots[0].1.as_ref().unwrap();
        assert_eq!(plan.animation_length, 1.0);
        assert_eq!(
            plan.additive.as_ref().unwrap().texture_path,
            fixture.root.join("materials/skin.png")
        );
        assert_eq!(
            plan.model.as_ref().unwrap().material.unwrap().diffuse,
            [0.0, 1.0, 0.0, 1.0]
        );
    }
}
