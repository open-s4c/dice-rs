use ar::GnuBuilder;
use std::{
    env,
    error::Error,
    fmt,
    fs::{self, File},
    path::Path,
    path::PathBuf,
    process::Command,
};
use walkdir::{DirEntry, WalkDir};

mod autogen;

fn main() -> Result<(), Box<dyn Error>> {
    build_dice()?;
    build_dice_plugin("shim")?;
    Ok(())
}

fn build_dice() -> Result<(), Box<dyn Error>> {
    let manifest_dir = get_manifest_dir();
    let dice_src = manifest_dir.join("..").join("dice");

    let mut cfg = config_cmake(&dice_src);

    let build_path = env::var("OUT_DIR").expect("OUT_DIR must be set by Cargo");
    let lib_path = Path::new(&build_path).join("lib");
    let dice_path = lib_path.join("libdice.a");

    autogen::generate();

    fs::create_dir_all(&lib_path)?;

    let object_features = vec![
        "dice",
        "dice-box",
        "dice-cxa",
        "dice-dispatch",
        "dice-malloc",
        "dice-memcpy",
        "dice-mman",
        "dice-poll",
        "dice-pthread_cond",
        "dice-pthread_create",
        "dice-pthread_mutex",
        "dice-pthread_rwlock",
        "dice-self",
        "dice-sem",
        "dice-tsan",
        "dice-random",
    ];

    // filter enabled features
    let object_targets = object_features
        .into_iter()
        .filter(|feature| {
            env::var_os("CARGO_FEATURE_".to_owned() + &*feature.to_uppercase().replace('-', "_"))
                .is_some()
        })
        .map(|feature| feature.to_owned() + ".o")
        .collect::<Vec<String>>();

    // clean cmake build
    cfg.build_target("clean").build();

    // build all cmake targets and get the cmake directory if any built
    let mut maybe_cmake_out_dir = None;
    for target in object_targets {
        let cmake_out_dir = cfg.build_target(&target).build();
        assert!(
            maybe_cmake_out_dir
                .into_iter()
                .all(|old_cmake_out_dir| old_cmake_out_dir == cmake_out_dir)
        );
        maybe_cmake_out_dir = Some(cfg.build_target(&target).build());
    }

    let dependency_modules = ["tmplr"];

    // find all object file paths in cmake build directory
    let object_paths: Vec<PathBuf> = maybe_cmake_out_dir
        .map(|dst| dst.join("build"))
        .map(WalkDir::new)
        .into_iter()
        .flatten()
        .map(|maybe_entry| maybe_entry.map(DirEntry::into_path))
        .filter(|maybe_path| {
            maybe_path.iter().all(|path| {
                path.extension().into_iter().any(|ext| ext == "o")
                    & path.file_name().into_iter().any(|filename| {
                        filename.to_str().into_iter().any(|filename_str| {
                            dependency_modules.iter().all(|dependency_module| {
                                !filename_str.starts_with(dependency_module)
                            })
                        })
                    })
            })
        })
        .collect::<Result<_, _>>()?;

    // get object file names
    let object_file_names = object_paths
        .iter()
        .map(|path| {
            path.file_name()
                .expect(
                    "object paths are filtered by extension which requires filename to be present",
                )
                .as_encoded_bytes()
                .to_vec()
        })
        .collect();

    // pack object files into static library
    let dice_file = File::create(&dice_path)?;

    let mut builder = GnuBuilder::new(dice_file, object_file_names);

    object_paths
        .into_iter()
        .map(|object| builder.append_path(object))
        .reduce(Result::and)
        .unwrap_or(Ok(()))?;

    // add symbol index
    Command::new("ranlib").arg(&dice_path).status()?;

    let output_dir = Path::new(&build_path)
        .join("..")
        .join("..")
        .join("..")
        .join("libtsano.so");

    // build libtsano.o and copy it to the root output directory
    let _ = WalkDir::new(cfg.build_target("tsano").build())
        .into_iter()
        .find(|maybe_entry| {
            maybe_entry.iter().all(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .into_iter()
                    .any(|name| name == "libtsano.so" || name == "libtsano.dylib")
            })
        })
        .ok_or_else(|| FileNotFoundError {
            filename: "libtsano.so".to_string(),
        })?
        .map(DirEntry::into_path)
        .map(|path| fs::copy(path, &output_dir))?;

    println!("cargo:rustc-link-search={}", lib_path.display());
    println!("cargo:rerun-if-changed={}", dice_src.display());

    Ok(())
}

fn config_cmake(path: &Path) -> cmake::Config {
    let mut cfg = cmake::Config::new(path);
    let profile = env::var("PROFILE").unwrap_or("debug".into());
    let build_type = if profile == "release" {
        "Release"
    } else {
        "Debug"
    };
    cfg.profile(build_type);

    cfg.define("DICE_ALLOC_STRICT_ALIGN8", "ON");

    cfg.define("CMAKE_POSITION_INDEPENDENT_CODE", "ON");

    if cfg!(feature = "lto") {
        cfg.define("DICE_LTO", "ON");
    } else {
        cfg.define("DICE_LTO", "OFF");
    }

    if cfg!(feature = "interpose-memcpy") {
        cfg.define("DICE_INTERPOSE_MEMCPY", "ON");
    } else {
        cfg.define("DICE_INTERPOSE_MEMCPY", "OFF");
    }

    if cfg!(feature = "log-debug") {
        cfg.define("DICE_LOG_LEVEL", "DEBUG");
    } else if cfg!(feature = "log-info") {
        cfg.define("DICE_LOG_LEVEL", "INFO");
    } else if cfg!(feature = "log-fatal") {
        cfg.define("DICE_LOG_LEVEL", "FATAL");
    }

    if cfg!(feature = "san-thread") {
        cfg.define("DICE_SANITIZER", "thread");
    } else if cfg!(feature = "san-address") {
        cfg.define("DICE_SANITIZER", "address");
    } else if cfg!(feature = "san-undefined") {
        cfg.define("DICE_SANITIZER", "undefined");
    } else {
        cfg.define("DICE_SANITIZER", "");
    }

    let cmake_env_vars = vec![
        ("DICE_C_COMPILER", "CMAKE_C_COMPILER"),
        ("DICE_CXX_COMPILER", "CMAKE_CXX_COMPILER"),
        ("DICE_MEMPOOL_SIZE", "DICE_MEMPOOL_SIZE"),
        ("DICE_MEMSET", "DICE_MEMSET"),
    ];

    cmake_env_vars
        .into_iter()
        .flat_map(|(env_var, cmake_var)| {
            env::var(env_var).map(|env_var_val| (env_var_val, cmake_var))
        })
        .for_each(|(env_var_val, cmake_var)| {
            cfg.define(cmake_var, env_var_val);
        });

    cfg
}

fn build_dice_plugin(plugin_name: &str) -> Result<(), Box<dyn Error>> {
    let manifest_dir = get_manifest_dir();
    let plugin_dir = manifest_dir.join(plugin_name);
    let mut cfg = config_cmake(&plugin_dir);

    cfg.define("LOG_PREFIX", "\"dice-rs: \"");

    let plugin_out_dir = cfg.build_target(plugin_name).build();

    println!(
        "cargo:rustc-link-search={}",
        plugin_out_dir.join("build").display()
    );

    println!("cargo:rerun-if-changed={}", plugin_dir.display());
    let dice_src = manifest_dir.join("..").join("dice");
    println!(
        "cargo:rerun-if-changed={}",
        dice_src.join("include").display()
    );

    Ok(())
}

pub fn get_manifest_dir() -> PathBuf {
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Cargo sets this environment variable"))
}

#[derive(Debug, Clone)]
struct FileNotFoundError {
    filename: String,
}

impl fmt::Display for FileNotFoundError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "file not found: {}", self.filename)
    }
}

impl Error for FileNotFoundError {}
