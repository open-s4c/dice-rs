use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    env,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

use bindgen::{
    Formatter,
    callbacks::{AttributeInfo, IntKind, ItemInfo, ItemKind, ParseCallbacks, TypeKind},
};
use regex::{Captures, Regex};

use crate::get_manifest_dir;

/// Transforms snake_case strings to CamelCase using Regex replacement.
fn to_camel_case(s: &str) -> String {
    let re = Regex::new(r"(?:^|_)([a-z0-9])").unwrap();
    re.replace_all(&s.to_ascii_lowercase(), |caps: &Captures| {
        caps[1].to_uppercase()
    })
    .to_string()
}

#[derive(Debug, Default)]
struct GeneratorCallbacks {
    event_map: RefCell<HashMap<String, String>>,
}

impl ParseCallbacks for GeneratorCallbacks {
    /// Constants starting with EVENT_ should be treated as unsigned integers.
    fn int_macro(&self, name: &str, _value: i64) -> Option<IntKind> {
        if name.starts_with("EVENT_") {
            Some(IntKind::UInt)
        } else {
            None
        }
    }

    /// Renames generated structs (ex: `malloc_event` -> 'MallocEvent`) and maps them to constants.
    fn item_name(&self, item: ItemInfo<'_>) -> Option<String> {
        if !matches!(item.kind, ItemKind::Type) {
            return None;
        }

        // Regex: Anchored start (^), capture base name (\w+), literal suffix _event, anchored end ($)
        // Example: matches "aligned_alloc_event", captures "aligned_alloc" in group [1]
        let re = Regex::new(r"^(\w+)_event$").expect("Invalid Regex");
        let caps = re.captures(item.name)?;
        let base_name = &caps[1];

        // Generate Struct Name: "aligned_alloc" -> "AlignedAllocEvent" (note: _event was removed before)
        let rust_name = to_camel_case(base_name) + "Event";

        // Generate Const Name: "ALLIGNED_ALLOC" -> "EVENT_ALLIGNED_ALLOC"
        let const_name = format!("EVENT_{}", base_name.to_uppercase());

        self.event_map
            .borrow_mut()
            .insert(rust_name.clone(), const_name);

        Some(rust_name)
    }

    /// Add the custom #[dice_event(raw::...)] attribute onto generated structs.
    fn add_attributes(&self, info: &AttributeInfo<'_>) -> Vec<String> {
        if !matches!(info.kind, TypeKind::Struct) {
            return Vec::new();
        }

        if let Some(const_name) = self.event_map.borrow().get(info.name) {
            vec![format!(r#"#[dice_event(raw::{})]"#, const_name)]
        } else {
            Vec::new()
        }
    }
}

/// Post-processes the bindgen output string
/// - move layout tests to the bottom of the file
/// - move constants into raw
/// - change constants type
/// - cleanup some paths of types
/// - rename structs to use CamelCase
fn transform_bindings(src: &str) -> String {
    let mut raw_body = String::new();
    let mut tests_body = String::new();
    let mut found_constants = Vec::new();

    // Example match: "pub const EVENT_FOO: u32 = 1;\n"
    let const_re = Regex::new(r"(?m)^pub const (EVENT_(\w+)):.*?= (.*?);\r?\n?").unwrap();

    // Example match: "#[allow(clippy::unnecessary_operation... } ]; ... };" (spanning multiple lines)
    let layout_re = Regex::new(
        r"(?ms)^#\[allow\(clippy::unnecessary_operation, clippy::identity_op\)\].*?^\};",
    )
    .unwrap();

    // Example match: "#[dice_event(raw::EVENT_FOO)]"
    let existing_event_re = Regex::new(r"#\[dice_event\(raw::(EVENT_\w+)\)\]").unwrap();

    // Extract Layout Testes
    let src_no_tests = layout_re.replace_all(src, |caps: &Captures| {
        tests_body.push_str(&caps[0]);
        tests_body.push('\n');
        ""
    });

    // Extract Constants
    let main_body = const_re.replace_all(&src_no_tests, |caps: &Captures| {
        let full_name = &caps[1];
        let base_name = &caps[2];
        let value = caps[3].trim().trim_end_matches(';');

        found_constants.push((full_name.to_string(), base_name.to_string()));
        writeln!(raw_body, "    pub const {}: TypeId = {};", full_name, value).unwrap();
        ""
    });

    // Generate Missing Structs
    let implemented_events: HashSet<String> = existing_event_re
        .captures_iter(&main_body)
        .map(|cap| cap[1].to_string())
        .collect();

    let mut synthetic_structs = String::new();
    if !found_constants.is_empty() {
        synthetic_structs.push_str("\n// --- synthetic event structs ---\n");
        for (const_name, base_name) in found_constants {
            if !implemented_events.contains(&const_name) {
                let struct_name = to_camel_case(&base_name);
                writeln!(
                    synthetic_structs,
                    "#[repr(C)]\n#[derive(Copy, Clone, Debug)]\n#[dice_event(raw::{})]\npub struct {}Event;",
                    const_name, struct_name
                ).unwrap();
            }
        }
    }

    // Strip some paths
    let final_main = main_body.replace("::std::option::Option", "Option");

    // Assemble
    format!(
        r#"
// --- Autogenerated by build.rs ---
// --- Manually Added ---
use crate::{{DiceEvent, TypeId}};
use dice_derive::dice_event;

// --- bindgen output ---
/// raw constants from dice
pub mod raw {{
    use crate::TypeId;
{}}}
{}
{}
// --- layout tests ---
{}
"#,
        raw_body,
        final_main.trim(),
        synthetic_structs,
        tests_body
    )
}

/// Aggregates all .h files in the events directory into a single temporary wrapper.h file.
pub fn create_single_header<P: AsRef<Path>>(dir: P, out: P) {
    let mut wrapper_content = String::from("// Auto-generated wrapper\n");
    let entries = fs::read_dir(&dir).expect("Failed to read events directory");
    let mut paths: Vec<_> = entries
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<_, _>>()
        .unwrap();
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

    if !events_dir.exists() {
        panic!("Could not find events directory");
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
        .layout_tests(true)
        .generate()
        .expect("Unable to generate bindings");

    let output = transform_bindings(&bindings.to_string());

    fs::write(out_path.join("bindings.rs"), output).expect("Couldn't write bindings!");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", events_dir.display());
}
