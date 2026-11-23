use ar::GnuBuilder;
use std::{
    env,
    error::Error,
    fs::{self, File},
    path::Path,
    path::PathBuf,
    process::Command,
};
use walkdir::{DirEntry, WalkDir};

mod autogen;

fn main() -> Result<(), Box<dyn Error>> {
    // C compilation and Rust bindgen must consume the same Roll header.
    let header = if env::var_os("CARGO_FEATURE_MANUAL_LINK").is_some() {
        PathBuf::from(env::var("DICE_ROLL_HEADER")?)
    } else {
        build_rolled_dice()?
    };
    if !header.is_file() {
        return Err(format!("Dice Roll header not found: {}", header.display()).into());
    }
    autogen::generate(Some(&header));
    build_dice_plugin("shim")?;
    Ok(())
}

/// Build the configured Dice core without archiving legacy dispatch objects.
fn build_rolled_dice() -> Result<PathBuf, Box<dyn Error>> {
    let manifest_dir = get_manifest_dir();
    let dice_src = manifest_dir.join("..").join("dice");
    let config = env::var_os("DICE_ROLL_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir.join("roll.dice"));
    if !config.exists() {
        return Err(format!("DICE_ROLL_CONFIG does not exist: {}", config.display()).into());
    }
    let config = config.canonicalize()?;
    for name in ["DICE_ROLL_CONFIG", "DICE_ROLL_IMPORT_PATHS", "DICE_MODULES"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    println!("cargo:rerun-if-changed={}", config.display());

    let mut modules = Vec::<String>::new();
    for module in [
        "cxa", "malloc", "memcpy", "mman", "pthread_cond",
        "pthread_create", "pthread_mutex", "pthread_rwlock", "self",
        "sem", "stacktrace", "tsan",
    ] {
        let feature = format!("CARGO_FEATURE_DICE_{}", module.to_uppercase());
        if env::var_os(feature).is_some() {
            modules.push(module.to_string());
        }
    }
    for module in env::var("DICE_MODULES").unwrap_or_default().split(';') {
        let module = module.trim().trim_start_matches("dice-");
        if !module.is_empty() && !modules.iter().any(|value| value == module) {
            modules.push(module.to_string());
        }
    }
    if modules.is_empty() {
        // Default Cargo feature has no modules; self yields a minimal core.
        modules.push("self".to_string());
    }
    let mut cfg = config_cmake(&dice_src);
    cfg.define("DICE_ROLL_CONFIG", config.to_string_lossy().as_ref());
    cfg.define("DICE_ROLL_MODULES", modules.join(";"));
    if let Ok(paths) = env::var("DICE_ROLL_IMPORT_PATHS") {
        cfg.define("DICE_ROLL_IMPORT_PATHS", paths);
    }
    let build_root = cfg.build_target("dice-roll-runtime").build().join("build");
    let header = build_root.join("dice-roll-runtime-dice").join("dice.h");
    if !header.is_file() {
        return Err(format!("Dice Roll header was not generated: {}", header.display()).into());
    }
    let mut paths: Vec<PathBuf> = WalkDir::new(&build_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().is_some_and(|ext| ext == "o")
                && entry.path().to_string_lossy().contains("CMakeFiles/dice-roll-runtime.dir/")
        })
        .map(DirEntry::into_path)
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err("Dice Roll produced no runtime object files".into());
    }
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let lib_dir = out_dir.join("lib");
    fs::create_dir_all(&lib_dir)?;
    let archive = File::create(lib_dir.join("libdice.a"))?;
    let names = paths.iter()
        .map(|p| p.file_name().expect("object filename").as_encoded_bytes().to_vec())
        .collect();
    let mut ar = GnuBuilder::new(archive, names);
    for path in paths {
        ar.append_path(path)?;
    }
    let status = Command::new("ranlib")
        .arg(lib_dir.join("libdice.a"))
        .status()?;
    if !status.success() {
        return Err("ranlib failed for the rolled Dice archive".into());
    }
    println!("cargo:rustc-link-search={}", lib_dir.display());

    let tsano_root = cfg.build_target("tsano").build();
    let tsano = WalkDir::new(tsano_root).into_iter()
        .filter_map(Result::ok)
        .find(|entry| {
            let name = entry.file_name().to_string_lossy();
            name == "libtsano.so" || name == "libtsano.dylib"
        })
        .ok_or("Could not locate the built libtsano shared library")?;
    fs::copy(
        tsano.path(),
        out_dir.join("..").join("..").join("..").join(tsano.file_name()),
    )?;
    println!("cargo:rerun-if-changed={}", dice_src.display());
    Ok(header)
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
        ("DICE_EXTENSION_DIRS", "DICE_EXTENSION_DIRS"),
    ];

    cmake_env_vars
        .into_iter()
        .inspect(|(env_var, _)| println!("cargo:rerun-if-env-changed={}", env_var))
        .flat_map(|(env_var, cmake_var)|
            env::var(env_var).map(|env_var_val| (env_var_val, cmake_var))
        )
        .for_each(|(env_var_val, cmake_var)| {
            cfg.define(cmake_var, env_var_val);
        });

    env::var("DICE_EXTENSION_DIRS")
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
        .for_each(|dir| println!("cargo:rerun-if-changed={}", dir));

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
