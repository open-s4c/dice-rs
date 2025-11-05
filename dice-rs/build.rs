use ar::GnuBuilder;
use std::{env, fs::{self, File}, path::Path, path::PathBuf, process::Command};
use walkdir::WalkDir;

fn main() {
    build_dice();
    build_shim();
}

fn build_dice() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let dice_src = manifest_dir.join("..").join("dice");

    let mut cfg = config_dice();

    let build_path = env::var("OUT_DIR").expect("OUT_DIR not set by Cargo");
    let lib_path = Path::new(&build_path).join("lib");
    let dice_path = lib_path.join("libdice.a");

    if !lib_path.exists() {
        fs::create_dir_all(&lib_path).expect("Failed to create 'lib' directory");
    }

    let object_targets  = vec![
        #[cfg(feature = "dice")]
        "dice.o",
        #[cfg(feature = "dice-box")]
        "dice-box.o",
        #[cfg(feature = "dice-cxa")]
        "dice-cxa.o",
        #[cfg(feature = "dice-dispatch")]
        "dice-dispatch.o",
        #[cfg(feature = "dice-malloc")]
        "dice-malloc.o",
        #[cfg(feature = "dice-memcpy")]
        "dice-memcpy.o",
        #[cfg(feature = "dice-mman")]
        "dice-mman.o",
        #[cfg(feature = "dice-pthread_cond")]
        "dice-pthread_cond.o",
        #[cfg(feature = "dice-pthread_create")]
        "dice-pthread_create.o",
        #[cfg(feature = "dice-pthread_mutex")]
        "dice-pthread_mutex.o",
        #[cfg(feature = "dice-pthread_rwlock")]
        "dice-pthread_rwlock.o",
        #[cfg(feature = "dice-self")]
        "dice-self.o",
        #[cfg(feature = "dice-sem")]
        "dice-sem.o",
        #[cfg(feature = "dice-tsan")]
        "dice-tsan.o"];

    let object_paths : Vec<PathBuf> = object_targets
        .iter()
        .map(|objlib| cfg.build_target(objlib).build())
        .collect::<Vec<PathBuf>>()
        .iter()
        .next()
        .iter()
        .map(|dst| dst.join("build"))
        .flat_map(|lib_dir| WalkDir::new(lib_dir).into_iter())
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().to_owned())
        .filter(|path| path.extension().map(|ext| ext == "o").unwrap_or(false))
        .collect();

    let object_file_names : Vec<Vec<u8>> = object_paths.iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_string())
        .map(|file_name| file_name.as_bytes().to_vec())
        .collect();

    let mut builder = GnuBuilder::new(File::create(&dice_path).unwrap(), object_file_names);

    object_paths
        .iter()
        .for_each(|object| builder.append_path(object).expect("could not add object to archive"));

    Command::new("ranlib")
        .arg(&dice_path)
        .status()
        .expect("Failed to run ranlib");

    let output_dir = Path::new(&build_path).join("..").join("..").join("..").join("libtsano.so");

    WalkDir::new(cfg.build_target("tsano").build())
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_str().map(|filename| filename == "libtsano.so").unwrap_or(false))
        .map(|entry| entry.into_path())
        .for_each(|path| { fs::copy(path, &output_dir).expect("could not copy libtsano.so"); });

    println!("cargo:rustc-link-search={}", lib_path.display());

    if cfg!(target_env = "gnu") {
        println!("cargo:rustc-link-lib=stdc++");
    }

    println!("cargo:rerun-if-changed={}", dice_src.display());
}

fn config_dice() -> cmake::Config {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let dice_src = manifest_dir.join("..").join("dice");
    let mut cfg = cmake::Config::new(&dice_src);

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());
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

    env::var("DICE_C_COMPILER").iter()
        .for_each(|cc| { cfg.define("CMAKE_C_COMPILER", cc); });
    env::var("DICE_CXX_COMPILER").iter()
        .for_each(|cc| { cfg.define("CMAKE_CXX_COMPILER", cc); });

    cfg
}

fn build_shim() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let dice_src = manifest_dir.join("..").join("dice");

    let shim_dir = manifest_dir.join("glue");

    let mut cc_build = cc::Build::new();
    cc_build
        .file(shim_dir.join("log_shim.c"))
        .include(dice_src.join("include"))
        .include(&shim_dir)
        .flag_if_supported("-fPIC");

    if cfg!(feature = "log-debug") {
        cc_build.define("DICE_LOG_LEVEL", Some("DEBUG"));
    } else if cfg!(feature = "log-info") {
        cc_build.define("DICE_LOG_LEVEL", Some("INFO"));
    } else if cfg!(feature = "log-fatal") {
        cc_build.define("DICE_LOG_LEVEL", Some("FATAL"));
    }

    cc_build.define("LOG_PREFIX", Some("\"dice-rs: \""));

    cc_build.compile("dice_log_shim");

    println!(
        "cargo:rerun-if-changed={}",
        shim_dir.join("log_shim.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        shim_dir.join("log_shim.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        dice_src.join("include").display()
    )
}
