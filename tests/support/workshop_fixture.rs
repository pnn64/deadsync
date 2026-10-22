fn copy_tree(source: &std::path::Path, target: &std::path::Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
}

pub fn build_workshop(root: &std::path::Path, workspace: &std::path::Path) {
    let mut skins = Vec::new();
    for family in ["Cel", "Metal"] {
        let source = workspace
            .join("assets/noteskins/dance")
            .join(family.to_lowercase());
        copy_tree(&source, &root.join(format!("{family} - Workshop")));
        for (category, label, file) in [
            (
                "Arrows",
                "DDR Vivid",
                "textures/Tap Note parts (mipmaps).png",
            ),
            ("Mines", "Recolored", "_mine tex.png"),
        ] {
            let target = root
                .join("Customizations")
                .join(category)
                .join(format!("{family} - {label}"));
            std::fs::create_dir_all(&target).unwrap();
            std::fs::copy(
                source.join(file),
                target.join(std::path::Path::new(file).file_name().unwrap()),
            )
            .unwrap();
        }
        let model = std::fs::read_to_string(source.join("_mine model.txt")).unwrap();
        for (label, scale) in [("Normal", 1.0_f32), ("Smaller", 0.5)] {
            let target = root
                .join("Customizations/Mine Size")
                .join(format!("{family} - {label}"));
            std::fs::create_dir_all(&target).unwrap();
            let mut scaled = String::new();
            for line in model.lines() {
                let mut fields = line
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                // MilkShape vertex: flags, x/y/z, u/v, bone index. Retain
                // topology, UVs, materials and animation from the bundled model.
                if fields.len() == 7 && fields[0].parse::<u32>().is_ok() {
                    for field in &mut fields[1..4] {
                        *field = (field.parse::<f32>().unwrap() * scale).to_string();
                    }
                    scaled.push_str(&fields.join(" "));
                } else {
                    scaled.push_str(line);
                }
                scaled.push('\n');
            }
            std::fs::write(target.join("_mine model.txt"), scaled).unwrap();
        }
        let preview = format!("{}-preview.png", family.to_lowercase());
        image::RgbaImage::new(2048, 2048)
            .save(root.join(&preview))
            .unwrap();
        let replacement =
            |slot: &str, id: &str, category: &str, label: &str, target: &str, cell: u32| {
                serde_json::json!({
                    "slot": slot, "id": id, "label": label, "cell": cell,
                    "files": [{"target": target, "source": format!(
                        "Customizations/{category}/{family} - {label}/{}",
                        std::path::Path::new(target).file_name().unwrap().to_str().unwrap())}]
                })
            };
        skins.push(serde_json::json!({
            "id": format!("{}-workshop", family.to_lowercase()),
            "base": format!("{family} - Workshop"), "preview": preview,
            "options": [
                replacement("arrows", "ddr-vivid", "Arrows", "DDR Vivid", "textures/Tap Note parts (mipmaps).png", 0),
                replacement("mines", "recolored", "Mines", "Recolored", "_mine tex.png", 1),
                replacement("mine_size", "normal", "Mine Size", "Normal", "_mine model.txt", 2),
                replacement("mine_size", "smaller", "Mine Size", "Smaller", "_mine model.txt", 3),
            ]
        }));
    }
    std::fs::write(root.join("pack.json"), serde_json::to_vec(&serde_json::json!({
        "schema": 1, "id": "generated-workshop", "version": "1", "source": "fixture", "skins": skins,
    })).unwrap()).unwrap();
}

pub fn build_preview_workshop(root: &std::path::Path, workspace: &std::path::Path) {
    build_workshop(root, workspace);
    let path = root.join("pack.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for skin in manifest["skins"].as_array_mut().unwrap() {
        let base = skin["base"].as_str().unwrap().to_owned();
        // Bundled Cel/Metal lifts are static. Give this animation fixture an
        // authored Model effect so both preview sizes must advance its pose.
        let lift = root.join(&base).join("Down Tap Lift.lua");
        let script = std::fs::read_to_string(&lift).unwrap().replace(
            "Def.Model {",
            "Def.Model { InitCommand=cmd(spin;effectmagnitude,0,0,90;effectclock,'beat');",
        );
        std::fs::write(lift, script).unwrap();
        let options = skin["options"].as_array_mut().unwrap();
        for (slot, id, target) in [
            ("arrows", "rgb", "textures/Tap Note parts (mipmaps).png"),
            (
                "arrows",
                "bright-td-vibrant",
                "textures/Tap Note parts (mipmaps).png",
            ),
            ("hold_active", "rgb", "Down Hold Body Active.png"),
            ("hold_inactive", "rgb", "Down Hold Body Inactive.png"),
            ("roll_active", "rgb", "_Down Roll Body active 4x1.png"),
            ("roll_inactive", "rgb", "Down Roll Body Inactive.png"),
            ("hold_explosions", "glow-default", "down hold explosion.png"),
            ("lifts", "spectrum", "textures/Tap Lift parts (mipmaps).png"),
            ("mines", "d-a-n-k", "_mine tex.png"),
        ] {
            let source = format!(
                "Customizations/Preview/{base}/{slot}/{id}/{}",
                std::path::Path::new(target)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
            );
            let destination = root.join(&source);
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(root.join(&base).join(target), destination).unwrap();
            if id == "bright-td-vibrant" {
                // Make lost frame resolution observable with a generated large
                // page, independently of the installed Workshop's artwork.
                let destination = root.join(&source);
                image::open(&destination)
                    .unwrap()
                    .resize_exact(2048, 2048, image::imageops::FilterType::Nearest)
                    .save(destination)
                    .unwrap();
            }
            options.push(
                serde_json::json!({"slot":slot,"id":id,"label":id,"cell":options.len(),
                "files":[{"target":target,"source":source}]}),
            );
        }
        for slot in [
            "arrows",
            "receptors",
            "hold_active",
            "hold_inactive",
            "roll_active",
            "roll_inactive",
            "tap_explosions",
            "hold_explosions",
            "mines",
            "mine_size",
            "lifts",
        ] {
            options.push(
                serde_json::json!({"slot":slot,"id":"base","label":"Original",
                "cell":options.len(),"files":[]}),
            );
        }
    }
    std::fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
}
