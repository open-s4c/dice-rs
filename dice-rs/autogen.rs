use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    env,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

use bindgen::{
    callbacks::{AttributeInfo, IntKind, ItemInfo, ItemKind, ParseCallbacks, TypeKind},
    Formatter,
};

use crate::get_manifest_dir;

fn parse_raw_event_name(raw_name: &str) -> Option<String> {
    let base = raw_name.strip_suffix("_event").unwrap_or(raw_name);
    let name_body = base.strip_prefix("EVENT_")?;

    let text_part = name_body.trim_start_matches('_');
    if text_part.is_empty() { return None; }

    let underscore_count = name_body.len() - text_part.len();
    
    let mut struct_name = String::with_capacity(name_body.len() + 5);
    struct_name.push_str(&name_body[..underscore_count]); 
    struct_name.push_str(&to_camel_case(text_part));
    struct_name.push_str("Event");

    Some(struct_name)
}

fn to_camel_case(s: &str) -> String {
    s.to_ascii_lowercase()
        .split('_')
        .filter(|p| !p.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

fn extract_dice_event_macro(line: &str) -> Option<&str> {
    if !line.starts_with("#[dice_event(raw::") { return None; }
    // TODO: this could be made more sophisticated
    let start = line.find("raw::")? + 5;
    let end = line[start..].find(')')?;
    Some(&line[start..start + end])
}

fn parse_event_const(line: &str) -> Option<(&str, &str)> {
    if !line.starts_with("pub const EVENT_") { return None; }
    let after_const = &line[10..]; 
    let (name, rest) = after_const.split_once(':')?;
    let (_, val_part) = rest.split_once('=')?;
    Some((name, val_part.trim().trim_end_matches(';').trim()))
}

#[derive(Debug, Default)]
struct GeneratorCallbacks {
    event_macros: RefCell<HashMap<String, String>>,
}

impl ParseCallbacks for GeneratorCallbacks {
    fn int_macro(&self, name: &str, _value: i64) -> Option<IntKind> {
        if name.starts_with("EVENT_") { Some(IntKind::UInt) } else { None }
    }

    fn item_name(&self, item: ItemInfo<'_>) -> Option<String> {
        if !matches!(item.kind, ItemKind::Type) { return None; }

        let rust_name = parse_raw_event_name(item.name)?;
        
        let base_name = item.name.strip_suffix("_event").unwrap_or(item.name);
        self.event_macros.borrow_mut().insert(rust_name.clone(), base_name.to_string());

        Some(rust_name)
    }

    fn add_attributes(&self, info: &AttributeInfo<'_>) -> Vec<String> {
        if !matches!(info.kind, TypeKind::Struct) { return Vec::new(); }

        if let Some(macro_name) = self.event_macros.borrow().get(info.name) {
            vec![format!(r#"#[dice_event(raw::{})]"#, macro_name)]
        } else {
            Vec::new()
        }
    }
}

fn transform_bindings(src: &str) -> String {
    let mut main_body = String::with_capacity(src.len());
    let mut raw_body = String::new();
    let mut tests_body = String::new();

    let mut event_consts = Vec::new();
    let mut events_with_struct = HashSet::new();
    let mut in_test_block = false;

    const LAYOUT_START: &str = "#[allow(clippy::unnecessary_operation, clippy::identity_op)]";

    for line in src.lines() {
        let trimmed = line.trim_start();

        // Handle layout test continuation or end
        if in_test_block {
            writeln!(tests_body, "{}", line).unwrap();
            if line.contains("};") {
                in_test_block = false;
            }
            continue;
        }

        // Handle layout test start
        if trimmed.starts_with(LAYOUT_START) {
            in_test_block = true;
            writeln!(tests_body, "{}", line).unwrap();
            continue;
        }

        // Handle Event Structs
        if let Some(event_name) = extract_dice_event_macro(trimmed) {
            if let Some(struct_name) = parse_raw_event_name(event_name) {
                 events_with_struct.insert(struct_name);
            }
            writeln!(main_body, "{}", line).unwrap();
            continue;
        }

        // Handle Constants
        if let Some((name, value)) = parse_event_const(trimmed) {
            event_consts.push(name.to_string());
            writeln!(raw_body, "    pub const {}: TypeId = {};", name, value).unwrap();
            continue;
        } 
        
        writeln!(main_body, "{}", line).unwrap();
    }

    // Generate synthetic structs for bare constants
    if !event_consts.is_empty() {
        main_body.push_str("\n// --- synthetic event structs ---\n");
        for raw_name in &event_consts {
            if let Some(struct_name) = parse_raw_event_name(raw_name) {
                if events_with_struct.contains(&struct_name) { continue; }

                writeln!(main_body, 
"#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::{raw_name})]
pub struct {struct_name};").unwrap();
            }
        }
    }

    // Final Assembly
    let main_body = main_body.replace("::std::option::Option", "Option");

    let mut out = String::with_capacity(src.len() + 500);
    out.push_str("// --- custom additions ---\n");
    out.push_str("use crate::{DiceEvent, TypeId};\n");
    out.push_str("use dice_derive::dice_event;\n");
    
    out.push_str("// --- bindgen output ---\n\n");
    out.push_str("pub mod raw {\n");
    out.push_str("    use crate::TypeId;\n");
    out.push_str(&raw_body);
    out.push_str("}\n\n");
    
    out.push_str(&main_body);

    if !tests_body.is_empty() {
        out.push_str("\n// --- layout tests ---\n");
        out.push_str(&tests_body);
    }

    out
}

pub fn create_single_header<P: AsRef<Path>>(dir: P, out: P) {
    let mut wrapper_content = String::from("// Auto-generated wrapper\n");
    let entries = fs::read_dir(&dir).expect("Failed to read events directory");
    let mut paths: Vec<_> = entries.map(|res| res.map(|e| e.path())).collect::<Result<_, _>>().unwrap();
    paths.sort();

    for path in paths {
        if path.extension().map_or(false, |s| s == "h") {
            if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                writeln!(wrapper_content, "#include <dice/events/{}>", filename).unwrap();
            }
        }
    }
    fs::write(&out, wrapper_content).expect("Failed to write wrapper.h");
}

pub fn generate() {
    let manifest_dir = get_manifest_dir();
    let dice_include = manifest_dir.join("..").join("dice").join("include");
    let events_dir = dice_include.join("dice").join("events");

    if !events_dir.exists() { panic!("Could not find events directory"); }

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let wrapper_path = out_path.join("wrapper.h");

    create_single_header(&events_dir, &wrapper_path);

    let bindings = bindgen::Builder::default()
        .header(wrapper_path.to_string_lossy())
        .clang_arg(format!("-I{}", dice_include.display()))
        .parse_callbacks(Box::new(GeneratorCallbacks::default()))
        .allowlist_type(".*_event")
        .allowlist_var("EVENT_.*")
        .formatter(Formatter::Rustfmt)
        .ctypes_prefix("libc")
        .layout_tests(true)
        .generate()
        .expect("Unable to generate bindings");

    let output = bindings.to_string();
    let output = transform_bindings(&output);

    let bindings_file = out_path.join("bindings.rs");
    fs::write(&bindings_file, output).expect("Couldn't write bindings!");
    
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", events_dir.display());
}