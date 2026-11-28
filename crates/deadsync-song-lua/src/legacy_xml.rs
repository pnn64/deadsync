//! Legacy actor declarations enter the same materializer as Lua definitions.
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};
use std::{
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

#[derive(Default)]
struct Node {
    tag: String,
    attributes: Vec<(String, String)>,
    children: Vec<Self>,
}

impl Node {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }
}

pub(super) fn is_xml(path: &Path) -> bool {
    extension_is(path, "xml")
}

fn extension_is(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
}

pub(super) fn load(path: &Path) -> Result<String, String> {
    crate::bake_inputs::file(path);
    let input = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let root = read_tree(&input).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut output = String::from("return ");
    actor(&mut output, &root, path)?;
    Ok(output)
}

fn read_tree(input: &str) -> Result<Node, String> {
    let mut reader = Reader::from_str(input);
    let mut parents = vec![Node::default()];
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(start) => {
                if parents.len() > 128 {
                    return Err("legacy actor XML exceeds 128 nested elements".into());
                }
                parents.push(read_element(&start)?);
            }
            Event::Empty(start) => parents
                .last_mut()
                .unwrap()
                .children
                .push(read_element(&start)?),
            Event::End(_) if parents.len() > 1 => {
                let node = parents.pop().unwrap();
                parents.last_mut().unwrap().children.push(node);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut root = parents.pop().unwrap();
    if !parents.is_empty() || root.children.len() != 1 {
        return Err("legacy actor XML requires one complete root element".into());
    }
    Ok(root.children.pop().unwrap())
}

fn read_element(start: &BytesStart<'_>) -> Result<Node, String> {
    let name = start.name();
    let name = std::str::from_utf8(name.as_ref()).map_err(|error| error.to_string())?;
    let attributes = start
        .attributes()
        .map(|attribute| {
            let attribute = attribute.map_err(|error| error.to_string())?;
            let name =
                std::str::from_utf8(attribute.key.as_ref()).map_err(|error| error.to_string())?;
            // The legacy parser preserves line breaks in command values.
            let value = std::str::from_utf8(&attribute.value).map_err(|error| error.to_string())?;
            let value = quick_xml::escape::unescape(value).map_err(|error| error.to_string())?;
            Ok((name.to_string(), value.into_owned()))
        })
        .collect::<Result<_, String>>()?;
    Ok(Node {
        tag: name.to_string(),
        attributes,
        children: Vec::new(),
    })
}

fn actor(output: &mut String, node: &Node, source: &Path) -> Result<(), String> {
    let directory = source.parent().unwrap_or(Path::new("."));
    let class = node.attr("Class").or_else(|| node.attr("Type"));
    let file = class
        .is_none()
        .then(|| node.attr("File"))
        .flatten()
        .filter(|file| !file.is_empty());
    let resource = file.map(|file| resolve(directory, file)).transpose()?;
    let class = match resource.as_deref() {
        Some(path)
            if extension_is(path, "sprite")
                || crate::is_song_lua_image_path(path)
                || crate::is_song_lua_video_path(path) =>
        {
            "Sprite"
        }
        Some(path) if extension_is(path, "txt") => "Model",
        // The pinned MakeActor returns a dummy Actor for unknown file types,
        // including the OpenITG .model references in this legacy chart.
        Some(_) => "Actor",
        None => class.unwrap_or(&node.tag),
    };
    // The baked Def constructors bind the native actor methods before the
    // background effect wrapper adopts the tree.
    output.push_str("Def[");
    quote(output, class);
    output.push_str("]{\n");
    field(output, "_Source", &crate::file_path_string(source));
    field(
        output,
        "_Dir",
        &format!("{}/", crate::file_path_string(directory)),
    );
    match (class, resource.as_deref()) {
        ("Sprite", Some(path)) if extension_is(path, "sprite") => sprite(output, path)?,
        ("Sprite", Some(path)) => path_field(output, "Texture", path),
        ("Model", Some(path)) => {
            for key in ["Meshes", "Materials", "Bones"] {
                path_field(output, key, path);
            }
        }
        // ITGmania does not render OpenITG .model descriptors. The dummy
        // still runs the XML commands and contributes its tween lifetime.
        ("Actor", Some(path)) if extension_is(path, "model") => {}
        ("Actor", Some(path)) => log::warn!(
            "Unknown legacy actor file type '{}', using a dummy Actor",
            path.display()
        ),
        _ => {}
    }
    for (name, value) in &node.attributes {
        if matches!(name.as_str(), "Class" | "Type" | "File") {
            continue;
        }
        if name.ends_with("Command") {
            key(output, name);
            command(output, value)?;
            output.push_str(",\n");
        } else if let Some(expression) = value.strip_prefix('@') {
            key(output, name);
            writeln!(output, "({expression}),").unwrap();
        } else {
            field(output, name, value);
        }
    }
    if let Some(children) = node.children.iter().find(|child| child.tag == "children") {
        for child in &children.children {
            actor(output, child, source)?;
            output.push_str(",\n");
        }
    }
    output.push('}');
    Ok(())
}

fn resolve(directory: &Path, file: &str) -> Result<PathBuf, String> {
    // Native RageFile resolves spelling case-insensitively, including the
    // .model references with mixed-case names that become dummy Actors.
    let joined = crate::files::collapse_actor_path(&directory.join(file.replace('\\', "/")));
    if joined.exists() {
        return Ok(joined);
    }
    let name = joined
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let mut matches = joined
        .parent()
        .into_iter()
        .flat_map(|parent| fs::read_dir(parent).into_iter().flatten())
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
        });
    if let Some(path) = matches.next()
        && matches.next().is_none()
    {
        return Ok(path);
    }
    Err(format!(
        "legacy actor resource '{}' could not be resolved",
        joined.display()
    ))
}

fn sprite(output: &mut String, path: &Path) -> Result<(), String> {
    crate::bake_inputs::file(path);
    let input = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut in_sprite = false;
    for line in input.lines().map(str::trim) {
        if line.starts_with('[') {
            in_sprite = line.eq_ignore_ascii_case("[Sprite]");
        } else if in_sprite && let Some((name, value)) = line.split_once('=') {
            let name = name.trim();
            let value = value.trim();
            if name == "Texture" {
                path_field(
                    output,
                    name,
                    &resolve(path.parent().unwrap_or(Path::new(".")), value)?,
                );
            } else if name.starts_with("Frame") || name.starts_with("Delay") {
                field(output, name, value);
            }
        }
    }
    Ok(())
}

// LuaManager.cpp::ParseCommandList's legacy mode. The wrapped expression
// is already authored Lua when the command begins with `%`.
fn command(output: &mut String, value: &str) -> Result<(), String> {
    if let Some(lua) = value.strip_prefix('%') {
        write!(output, "({lua})").unwrap();
        return Ok(());
    }
    output.push_str("function(self)\nparent = self:GetParent()\n");
    for segment in value.split(';').filter(|segment| !segment.is_empty()) {
        let mut args = segment.split(',');
        let method = args.next().unwrap().trim().to_ascii_lowercase();
        if method.is_empty()
            || !method
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(format!("invalid legacy actor command '{method}'"));
        }
        write!(output, "self:{method}(").unwrap();
        for (index, argument) in args.enumerate() {
            if index != 0 {
                output.push(',');
            }
            let argument = argument.strip_prefix('+').unwrap_or(argument);
            if index == 0
                && matches!(
                    method.as_str(),
                    "horizalign"
                        | "vertalign"
                        | "effectclock"
                        | "blend"
                        | "ztestmode"
                        | "cullmode"
                        | "playcommand"
                        | "queuecommand"
                        | "queuemessage"
                        | "settext"
                )
            {
                quote(output, argument);
            } else if argument.starts_with('#') {
                let [r, g, b, a] =
                    crate::theme_colors::parse_color_text(argument).unwrap_or([1.0; 4]);
                write!(output, "{r},{g},{b},{a}").unwrap();
            } else {
                output.push_str(argument);
            }
        }
        output.push_str(")\n");
    }
    output.push_str("end");
    Ok(())
}

fn path_field(output: &mut String, name: &str, path: &Path) {
    field(output, name, &crate::file_path_string(path));
}

fn field(output: &mut String, name: &str, value: &str) {
    key(output, name);
    quote(output, value);
    output.push_str(",\n");
}

fn key(output: &mut String, name: &str) {
    output.push('[');
    quote(output, name);
    output.push_str("]=");
}

fn quote(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' | '\\' => {
                output.push('\\');
                output.push(character);
            }
            character if character.is_ascii_control() => {
                write!(output, "\\{:03}", character as u8).unwrap()
            }
            character => output.push(character),
        }
    }
    output.push('"');
}
