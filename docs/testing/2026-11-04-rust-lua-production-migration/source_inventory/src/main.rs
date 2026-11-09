//! Read-only Rust item ranges for preserving bodies during oracle isolation.
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};
use syn::{Item, UseTree, spanned::Spanned, visit::Visit};

#[derive(Default)]
struct References {
    paths: BTreeSet<String>,
    macro_identifiers: BTreeSet<String>,
    locals: BTreeSet<String>,
}
fn identifiers(tokens: TokenStream, out: &mut BTreeSet<String>) {
    for token in tokens {
        match token {
            TokenTree::Ident(name) => { out.insert(name.to_string()); }
            TokenTree::Group(group) => identifiers(group.stream(), out),
            _ => {}
        }
    }
}
impl<'ast> Visit<'ast> for References {
    fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
        self.locals.insert(pattern.ident.to_string());
        syn::visit::visit_pat_ident(self, pattern);
    }
    fn visit_type_param(&mut self, parameter: &'ast syn::TypeParam) {
        self.locals.insert(parameter.ident.to_string());
        syn::visit::visit_type_param(self, parameter);
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.paths.insert(path.segments.iter().map(|s| s.ident.to_string()).collect::<Vec<_>>().join("::"));
        syn::visit::visit_path(self, path);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        identifiers(mac.tokens.clone(), &mut self.macro_identifiers);
        syn::visit::visit_macro(self, mac);
    }
}
fn attributes(item: &Item) -> &[syn::Attribute] {
    match item {
        Item::Fn(x) => &x.attrs, Item::Struct(x) => &x.attrs,
        Item::Enum(x) => &x.attrs, Item::Type(x) => &x.attrs,
        Item::Const(x) => &x.attrs, Item::Static(x) => &x.attrs,
        Item::Mod(x) => &x.attrs, Item::Trait(x) => &x.attrs,
        Item::Impl(x) => &x.attrs, Item::Use(x) => &x.attrs,
        Item::Macro(x) => &x.attrs, Item::ForeignMod(x) => &x.attrs,
        _ => &[],
    }
}
fn imports(tree: &UseTree, prefix: Vec<String>, out: &mut Vec<Value>) {
    match tree {
        UseTree::Path(path) => {
            let mut next = prefix;
            next.push(path.ident.to_string());
            imports(&path.tree, next, out);
        }
        UseTree::Name(name) => {
            let mut path = prefix;
            path.push(name.ident.to_string());
            out.push(json!({"path":path,"alias":name.ident.to_string()}));
        }
        UseTree::Rename(name) => {
            let mut path = prefix;
            path.push(name.ident.to_string());
            out.push(json!({"path":path,"alias":name.rename.to_string()}));
        }
        UseTree::Glob(_) => out.push(json!({"path":prefix,"alias":"*"})),
        UseTree::Group(group) => for item in &group.items { imports(item, prefix.clone(), out); },
    }
}
fn describe(item: &Item) -> Value {
    let (kind, name, vis) = match item {
        Item::Fn(x) => ("fn", x.sig.ident.to_string(), x.vis.to_token_stream().to_string()),
        Item::Struct(x) => ("struct", x.ident.to_string(), x.vis.to_token_stream().to_string()),
        Item::Enum(x) => ("enum", x.ident.to_string(), x.vis.to_token_stream().to_string()),
        Item::Type(x) => ("type", x.ident.to_string(), x.vis.to_token_stream().to_string()),
        Item::Const(x) => ("const", x.ident.to_string(), x.vis.to_token_stream().to_string()),
        Item::Static(x) => ("static", x.ident.to_string(), x.vis.to_token_stream().to_string()),
        Item::Mod(x) => ("mod", x.ident.to_string(), x.vis.to_token_stream().to_string()),
        Item::Trait(x) => ("trait", x.ident.to_string(), x.vis.to_token_stream().to_string()),
        Item::Impl(x) => ("impl", x.self_ty.to_token_stream().to_string(), String::new()),
        Item::Use(x) => ("use", String::new(), x.vis.to_token_stream().to_string()),
        Item::Macro(x) => ("macro", x.ident.as_ref().map(ToString::to_string).unwrap_or_default(), String::new()),
        Item::ForeignMod(_) => ("extern", String::new(), String::new()),
        _ => ("other", String::new(), String::new()),
    };
    let span = item.span();
    let range = span.byte_range();
    let mut refs = References::default();
    if !matches!(item, Item::Mod(_)) { refs.visit_item(item); }
    let mut uses = Vec::new();
    if let Item::Use(x) = item { imports(&x.tree, vec![], &mut uses); }
    json!({"kind":kind,"name":name,"visibility":vis,
        "start":range.start,"end":range.end,
        "line":span.start().line,"end_line":span.end().line,
        "paths":refs.paths,"macro_identifiers":refs.macro_identifiers,"locals":refs.locals,
        "attributes":attributes(item).iter().map(|a| a.meta.to_token_stream().to_string()).collect::<Vec<_>>(),
        "imports":uses})
}
fn walk(root: &Path, dir: &Path, files: &mut Vec<Value>) {
    let mut entries = fs::read_dir(dir).unwrap().map(|x| x.unwrap().path()).collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if !["rust_engine", "playback"].iter().any(|name| path.file_name().unwrap() == *name) {
                walk(root, &path, files);
            }
        } else if path.extension().is_some_and(|x| x == "rs") {
            let source = fs::read_to_string(&path).unwrap();
            let parsed = syn::parse_file(&source).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            files.push(json!({"file":path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"),
                "bytes":source.len(),"items":parsed.items.iter().map(describe).collect::<Vec<_>>()}));
        }
    }
}
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 3, "source_inventory SOURCE_DIR OUTPUT_JSON");
    let root = Path::new(&args[1]);
    let mut files = Vec::new();
    walk(root, root, &mut files);
    let count = files.len();
    let items = files.iter().map(|file| file["items"].as_array().unwrap().len()).sum::<usize>();
    fs::write(&args[2], serde_json::to_vec_pretty(&files).unwrap()).unwrap();
    println!("Inventoried {items} items in {count} source files without modifying them.");
}
