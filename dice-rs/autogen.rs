use std::{
    collections::{HashMap, HashSet},
    env,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use bindgen::{
    Formatter,
    callbacks::{AttributeInfo, IntKind, ItemInfo, ItemKind, ParseCallbacks, TypeKind},
};

use crate::get_manifest_dir;

#[derive(Debug, Default)]
struct GeneratorCallbacks {
    event_macros: Mutex<HashMap<String, String>>,
}

impl ParseCallbacks for GeneratorCallbacks {
    fn int_macro(&self, name: &str, _value: i64) -> Option<IntKind> {
        if name.starts_with("EVENT_") {
            Some(IntKind::UInt)
        } else {
            None
        }
    }

    fn item_name(&self, item: ItemInfo<'_>) -> Option<String> {
        if !matches!(item.kind, ItemKind::Type) {
            return None;
        }

        let Some(base) = item.name.strip_suffix("_event") else {
            return None;
        };

        let mut chars = base.chars().peekable();
        let mut rust_name = String::new();

        while let Some('_') = chars.peek() {
            rust_name.push('_');
            chars.next();
        }

        let rest: String = chars.collect();
        if rest.is_empty() {
            return None;
        }

        rust_name.push_str(
            &rest
                .split('_')
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let mut c = s.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect::<String>(),
        );

        rust_name.push_str("Event");

        let macro_name = format!("EVENT_{}", base.to_ascii_uppercase());

        self.event_macros
            .lock()
            .unwrap()
            .insert(rust_name.clone(), macro_name);

        Some(rust_name)
    }

    fn add_attributes(&self, info: &AttributeInfo<'_>) -> Vec<String> {
        if !matches!(info.kind, TypeKind::Struct) {
            return Vec::new();
        }

        let rust_name = info.name;

        if let Some(macro_name) = self.event_macros.lock().unwrap().get(rust_name) {
            vec![format!(r#"#[dice_event(raw::{})]"#, macro_name)]
        } else {
            Vec::new()
        }
    }
}

pub fn create_single_header<P: AsRef<Path>>(dir: P, out: P) {
    let mut wrapper_content = String::from("// Auto-generated wrapper\n");

    let entries = fs::read_dir(&dir).expect("Failed to read events directory");

    for entry in entries {
        let entry = entry.expect("Failed to read entry");
        let path = entry.path();

        if let Some(ext) = path.extension() {
            if ext == "h" {
                if let Some(_) = path.file_stem() {
                    let filename = path.file_name().unwrap().to_string_lossy();
                    wrapper_content.push_str(&format!("#include <dice/events/{}>\n", filename));
                }
            }
        }
    }

    fs::write(&out, &wrapper_content).expect("Failed to write wrapper.h");
}

fn event_const_to_struct_name(event_const: &str) -> Option<String> {
    let base = event_const.strip_prefix("EVENT_")?;

    let mut chars = base.chars().peekable();
    let mut rust = String::new();

    while let Some('_') = chars.peek() {
        rust.push('_');
        chars.next();
    }

    let rest: String = chars.collect();
    if rest.is_empty() {
        return None;
    }

    rust.push_str(
        &rest
            .to_ascii_lowercase()
            .split('_')
            .filter(|p| !p.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<String>(),
    );

    if rust.trim_matches('_').is_empty() {
        return None;
    }

    rust.push_str("Event");
    Some(rust)
}

fn transform_bindings(src: &str) -> String {
    let mut body = String::new();

    // All events
    let mut event_consts: Vec<String> = Vec::new();
    // All events with a struct
    let mut events_with_struct: HashSet<String> = HashSet::new();
    // constants to merge later
    let mut raw_consts = String::new();

    for line in src.lines() {
        let trimmed = line.trim_start();

        if let Some(attr_pos) = trimmed.find("dice_event(") {
            let after = &trimmed[attr_pos + "dice_event(".len()..];
            if let Some(close) = after.find(')') {
                let inner = &after[..close];
                let ident = inner.trim();
                let last_segment = ident.rsplit("::").next().unwrap_or(ident).trim();
                if last_segment.starts_with("EVENT_") {
                    events_with_struct.insert(last_segment.to_string());
                }
            }
        }

        if trimmed.starts_with("pub const EVENT_") {
            let rest = trimmed
                .strip_prefix("pub const ")
                .expect("starts_with(pub const) but strip_prefix failed");

            let (name_and_type, value_part) = match rest.split_once('=') {
                Some(v) => v,
                None => {
                    body.push_str(line);
                    body.push('\n');
                    continue;
                }
            };

            let (name, _ty) = match name_and_type.split_once(':') {
                Some(v) => v,
                None => {
                    body.push_str(line);
                    body.push('\n');
                    continue;
                }
            };

            let name = name.trim().to_string();
            event_consts.push(name.clone());

            let value = value_part.trim().trim_end_matches(';').trim();

            let new_line = format!("    pub const {name}: TypeId = {value};\n");
            raw_consts.push_str(&new_line);

        } else {
            body.push_str(line);
            body.push('\n');
        }
    }

    // Add synthetic unit structs for events without a struct
    let mut first_synthetic = true;
    for const_name in &event_consts {
        if events_with_struct.contains(const_name) {
            continue;
        }

        if let Some(struct_name) = event_const_to_struct_name(const_name) {
            if first_synthetic {
                body.push('\n');
                body.push_str("// --- synthetic event structs for bare event constants ---\n");
                first_synthetic = false;
            }

            writeln!(
                &mut body,
                "\
#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::{const_name})]
pub struct {struct_name};\n"
            )
            .unwrap();
        }
    }

    let body = body.replace("::std::option::Option", "Option");

    // Prepend mod raw
    let mut full_body = String::new();

    full_body.push_str("pub mod raw {\n");
    full_body.push_str("    use crate::TypeId;\n");
    full_body.push_str(&raw_consts);
    full_body.push_str("}\n\n");

    full_body.push_str(&body);

    let mut out = String::new();
    out.push_str(
        "// --- custom additions ---\n\
         use crate::{DiceEvent, TypeId};\n\
         use dice_derive::dice_event;\n\
         // --- bindgen output (post-processed) ---\n\n",
    );
    out.push_str(&full_body);
    out
}

fn move_layout_tests_to_end(src: &str) -> String {
    let mut main_body = String::new();
    let mut tests = Vec::<String>::new();

    let mut in_test_block = false;
    let mut current_block = String::new();

    for line in src.lines() {
        if !in_test_block
            && line
                .trim_start()
                .starts_with("#[allow(clippy::unnecessary_operation, clippy::identity_op)]")
        {
            in_test_block = true;
            current_block.clear();
            current_block.push_str(line);
            current_block.push('\n');
            continue;
        }

        if in_test_block {
            current_block.push_str(line);
            current_block.push('\n');

            if line.contains("};") {
                in_test_block = false;
                tests.push(current_block.clone());
            }

            continue;
        }

        main_body.push_str(line);
        main_body.push('\n');
    }

    if !tests.is_empty() {
        if !main_body.ends_with('\n') {
            main_body.push('\n');
        }
        main_body.push('\n');
        main_body.push_str("// --- layout tests (moved to end by build.rs) ---\n\n");
        for block in tests {
            main_body.push_str(&block);
        }
    }

    main_body
}

pub fn generate() {
    let manifest_dir = get_manifest_dir();
    let dice_include = manifest_dir.join("..").join("dice").join("include");
    let events_dir = dice_include.join("dice").join("events");

    if !events_dir.exists() {
        panic!(
            "Could not find events directory at: {}",
            events_dir.display()
        );
    }

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
        .generate()
        .expect("Unable to generate bindings");

    let output = bindings.to_string();
    let output = transform_bindings(&output);
    let output = move_layout_tests_to_end(&output);

    let bindings_file = out_path.join("bindings.rs");
    fs::write(&bindings_file, output).expect("Couldn't write bindings!");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", events_dir.display());
}
