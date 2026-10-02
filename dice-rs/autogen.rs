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
    callbacks::{
        AttributeInfo,
        IntKind,
        ItemInfo,
        ItemKind,
        ParseCallbacks,
        TypeKind,
    },
};
use regex::{Captures, Regex};

use crate::get_manifest_dir;

/// Transforms snake_case strings to CamelCase using Regex replacement.
fn to_camel_case(s: &str) -> String {
    let re =
        Regex::new(r"(?:^|_)(?<char>[a-z0-9])")
            .expect("create valid regex");

    re.replace_all(
        &s.to_ascii_lowercase(),
        |caps: &Captures| {
            caps["char"].to_uppercase()
        },
    )
        .to_string()
}

#[derive(Debug, Default)]
struct GeneratorCallbacks {
    event_map: RefCell<HashMap<String, String>>,
    known_constants: RefCell<HashSet<String>>,
}

impl ParseCallbacks for GeneratorCallbacks {
    /// Constants starting with EVENT_ should be treated as unsigned integers.
    fn int_macro(
        &self,
        name: &str,
        _value: i64,
    ) -> Option<IntKind> {
        if name.starts_with("EVENT_") {
            self.known_constants
                .borrow_mut()
                .insert(name.to_string());

            Some(IntKind::UInt)
        } else {
            None
        }
    }

    /// Renames generated structs
    ///
    ///     malloc_event -> MallocEvent
    ///
    /// and maps them to constants:
    ///
    ///     malloc_event -> EVENT_MALLOC
    fn item_name(
        &self,
        item: ItemInfo<'_>,
    ) -> Option<String> {
        if !matches!(item.kind, ItemKind::Type) {
            return None;
        }

        let re =
            Regex::new(r"^(?<base>\w+)_event$")
                .expect("Invalid Regex");

        let caps = re.captures(item.name)?;
        let base_name = &caps["base"];

        let const_name =
            format!("EVENT_{}", base_name.to_uppercase());

        if self
            .known_constants
            .borrow()
            .contains(&const_name)
        {
            let rust_name =
                to_camel_case(base_name) + "Event";

            self.event_map
                .borrow_mut()
                .insert(
                    rust_name.clone(),
                    const_name,
                );

            Some(rust_name)
        } else {
            None
        }
    }

    /// Add #[dice_event(raw::EVENT_...)] onto generated event structs.
    fn add_attributes(
        &self,
        info: &AttributeInfo<'_>,
    ) -> Vec<String> {
        if !matches!(info.kind, TypeKind::Struct) {
            return Vec::new();
        }

        if let Some(const_name) =
            self.event_map.borrow().get(info.name)
        {
            vec![
                format!(
                    r#"#[dice_event(raw::{})]"#,
                    const_name,
                ),
            ]
        } else {
            Vec::new()
        }
    }
}

fn special_event_payload(
    event: &str,
) -> Option<(&'static str, &'static str)> {
    match event {
        "EVENT_STACKTRACE_ENTER" => {
            Some(("StacktraceEnterEvent", "stacktrace_event"))
        }
        "EVENT_STACKTRACE_EXIT" => {
            Some(("StacktraceExitEvent", "stacktrace_event"))
        }
        _ => None,
    }
}

/// Post-processes bindgen output:
///
/// - moves layout tests to the bottom
/// - moves EVENT_* constants into `raw`
/// - gives EVENT_* constants TypeId type
/// - renames *_event structs
/// - attaches DiceEvent implementations
/// - generates synthetic unit structs for events without payload structs
fn transform_bindings(src: &str) -> String {
    let mut raw_body = String::new();
    let mut tests_body = String::new();
    let mut found_constants = Vec::new();

    // Example:
    //
    // pub const EVENT_FOO: u32 = 1;
    let const_re = Regex::new(
        r"(?m)^\s*pub\s+const\s+(?<fullname>EVENT_(?<name>\w+))\s*:.*?=\s*(?<value>.*?)\s*;\s*$",
    )
        .expect("create valid regex");

    // Current bindgen layout tests look approximately like:
    //
    // #[allow(clippy::unnecessary_operation, clippy::identity_op)]
    // const _: () = {
    //     ...
    // };
    let layout_re = Regex::new(
        r"(?ms)^\s*#\[\s*allow\s*\(\s*clippy\s*::\s*unnecessary_operation\s*,\s*clippy\s*::\s*identity_op\s*\)\s*\].*?^\s*\}\s*;\s*$",
    )
        .expect("create valid regex");

    let existing_event_re =
        Regex::new(
            r"#\[dice_event\(raw::(?<event>EVENT_\w+)\)\]",
        )
            .expect("create valid regex");

    let newline_re =
        Regex::new(r"\n{3,}")
            .expect("create valid regex");

    // Extract layout tests.
    let src_no_tests =
        layout_re.replace_all(
            src,
            |caps: &Captures| {
                tests_body.push_str(&caps[0]);
                tests_body.push('\n');
                ""
            },
        );

    // Extract EVENT_* constants.
    let main_body =
        const_re.replace_all(
            &src_no_tests,
            |caps: &Captures| {
                let full_name =
                    &caps["fullname"];

                let base_name =
                    &caps["name"];

                let value =
                    &caps["value"];

                found_constants.push((
                    full_name.to_string(),
                    base_name.to_string(),
                ));

                writeln!(
                    raw_body,
                    "    pub const {}: TypeId = {};",
                    full_name,
                    value,
                )
                    .expect("Writing to String works");

                ""
            },
        );

    let cleaned_body =
        newline_re.replace_all(
            &main_body,
            "\n\n",
        );

    // Find events for which bindgen already produced a struct.
    let implemented_events: HashSet<String> =
        existing_event_re
            .captures_iter(&cleaned_body)
            .map(|cap| {
                cap["event"].to_string()
            })
            .collect();

    // EVENT_* constants without a corresponding *_event struct are treated
    // as marker events.
    let mut synthetic_structs =
        String::new();

    if !found_constants.is_empty() {
        synthetic_structs.push_str(
            "\n// --- synthetic event structs ---\n",
        );

        for (const_name, base_name) in found_constants {
            if implemented_events.contains(&const_name) {
                continue;
            }

            if let Some((rust_name, payload_type)) =
                special_event_payload(&const_name)
            {
                writeln!(
                    synthetic_structs,
                    concat!(
                    "#[repr(transparent)]\n",
                    "#[derive(Copy, Clone, Debug)]\n",
                    "#[dice_event(raw::{})]\n",
                    "pub struct {}(pub {});\n",
                    "\n",
                    "impl ::core::ops::Deref for {} {{\n",
                    "    type Target = {};\n",
                    "\n",
                    "    fn deref(&self) -> &Self::Target {{\n",
                    "        &self.0\n",
                    "    }}\n",
                    "}}\n",
                    "\n",
                    "impl ::core::ops::DerefMut for {} {{\n",
                    "    fn deref_mut(&mut self) -> &mut Self::Target {{\n",
                    "        &mut self.0\n",
                    "    }}\n",
                    "}}\n",
                    ),
                    const_name,
                    rust_name,
                    payload_type,
                    rust_name,
                    payload_type,
                    rust_name,
                )
                    .expect("Writing to String works");

                continue;
            }

            // No C payload type was found. Treat this as a marker event.
            let struct_name =
                to_camel_case(&base_name);

            writeln!(
                synthetic_structs,
                concat!(
                "#[repr(C)]\n",
                "#[derive(Copy, Clone, Debug)]\n",
                "#[dice_event(raw::{})]\n",
                "pub struct {}Event;"
                ),
                const_name,
                struct_name,
            )
                .expect("Writing to String works");
        }
    }

    // Bindgen sometimes emits this verbose spelling.
    let final_main =
        cleaned_body.replace(
            "::std::option::Option",
            "Option",
        );

    format!(
        r#"
// --- Autogenerated by build.rs ---
// --- Manually Added ---
use crate::{{DiceEvent, TypeId}};
use dice_derive::dice_event;

// --- bindgen output ---

/// Raw event constants from Dice and Dice extensions.
pub mod raw {{
    use crate::TypeId;
{}}}

{}

{}

// --- layout tests ---

{}
"#,
        raw_body,
        final_main,
        synthetic_structs,
        tests_body,
    )
}

/// Parse DICE_EXTENSION_DIRS.
///
/// Dice defines this as a CMake list, so multiple directories are separated
/// using ';':
///
///     DICE_EXTENSION_DIRS=/path/to/ext1;/path/to/ext2
///
/// We deliberately use CMake-list semantics here rather than
/// `std::env::split_paths`, because the exact same value is also passed to
/// Dice's CMake configuration.
fn extension_dirs() -> Vec<PathBuf> {
    let Some(value) =
        env::var_os("DICE_EXTENSION_DIRS")
    else {
        return Vec::new();
    };

    value
        .to_string_lossy()
        .split(';')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Aggregates all .h files from all event directories into a single
/// temporary wrapper header.
///
/// We include headers by absolute path here. This avoids ambiguity when
/// Dice and one or more extensions all use the same public hierarchy:
///
///     include/dice/events/...
///
/// The headers themselves can still use normal includes such as:
///
///     #include <dice/events/foo.h>
///
/// because all corresponding include roots are supplied to clang separately.
pub fn create_single_header(
    event_dirs: &[PathBuf],
    out: &Path,
) {
    let mut paths = Vec::new();

    for dir in event_dirs {
        let entries =
            fs::read_dir(dir)
                .unwrap_or_else(|error| {
                    panic!(
                        "Failed to read events directory {}: {}",
                        dir.display(),
                        error,
                    )
                });

        for entry in entries {
            let path =
                entry
                    .expect(
                        "Failed to read event directory entry",
                    )
                    .path();

            if path
                .extension()
                .is_some_and(|ext| ext == "h")
            {
                paths.push(path);
            }
        }
    }

    paths.sort();
    paths.dedup();

    let mut wrapper_content =
        String::from(
            "// Auto-generated wrapper\n",
        );

    for path in paths {
        writeln!(
            wrapper_content,
            "#include \"{}\"",
            path.display(),
        )
            .expect("Writing to String works");
    }

    fs::write(
        out,
        wrapper_content,
    )
        .expect(
            "Failed to write wrapper.h",
        );
}

pub fn generate() {
    let manifest_dir =
        get_manifest_dir();

    // Built-in Dice headers.
    let dice_include =
        manifest_dir
            .join("..")
            .join("dice")
            .join("include");

    let dice_events =
        dice_include
            .join("dice")
            .join("events");

    if !dice_events.exists() {
        panic!(
            "Could not find Dice events directory: {}",
            dice_events.display(),
        );
    }

    let mut event_dirs =
        vec![dice_events];

    let mut include_dirs =
        vec![dice_include];

    // Add externally owned Dice extensions.
    //
    // Each extension is expected to have the same public-header layout used
    // by Craps:
    //
    //     <extension>/
    //       CMakeLists.txt
    //       include/
    //         dice/
    //           events/
    //             random.h
    //             epoll.h
    //             ...
    //
    for extension_dir in extension_dirs() {
        if !extension_dir.exists() {
            panic!(
                "DICE_EXTENSION_DIRS contains a directory that does not exist: {}",
                extension_dir.display(),
            );
        }

        let include_dir =
            extension_dir.join("include");

        let events_dir =
            include_dir
                .join("dice")
                .join("events");

        // Not every possible Dice extension necessarily needs to define
        // events. Such extensions should still be usable by CMake, so an
        // absent events directory is not an error here.
        if !events_dir.exists() {
            continue;
        }

        if !event_dirs.contains(&events_dir) {
            event_dirs.push(events_dir);
        }

        if !include_dirs.contains(&include_dir) {
            include_dirs.push(include_dir);
        }
    }

    let out_path =
        PathBuf::from(
            env::var("OUT_DIR")
                .expect("OUT_DIR exists"),
        );

    let wrapper_path =
        out_path.join("wrapper.h");

    create_single_header(
        &event_dirs,
        &wrapper_path,
    );

    let mut builder =
        bindgen::Builder::default()
            .header(
                wrapper_path
                    .to_string_lossy(),
            )
            .parse_callbacks(
                Box::new(
                    GeneratorCallbacks::default(),
                ),
            )
            .allowlist_type(".*_event")
            .allowlist_var("EVENT_.*")
            .formatter(
                Formatter::Rustfmt,
            )
            .ctypes_prefix("libc")
            .layout_tests(true)
            .derive_debug(true);

    // Make both built-in Dice headers and extension headers visible to clang.
    for include_dir in &include_dirs {
        builder =
            builder.clang_arg(
                format!(
                    "-I{}",
                    include_dir.display(),
                ),
            );
    }

    let bindings =
        builder
            .generate()
            .expect(
                "Unable to generate bindings",
            );

    let output =
        transform_bindings(
            &bindings.to_string(),
        );

    fs::write(
        out_path.join("bindings.rs"),
        output,
    )
        .expect(
            "Couldn't write bindings!",
        );

    // build.rs/autogen.rs itself.
    println!(
        "cargo:rerun-if-changed=build.rs"
    );

    // Re-run whenever the extension configuration changes.
    println!(
        "cargo:rerun-if-env-changed=DICE_EXTENSION_DIRS"
    );

    // Re-run whenever event headers change.
    for event_dir in &event_dirs {
        println!(
            "cargo:rerun-if-changed={}",
            event_dir.display(),
        );
    }
}
