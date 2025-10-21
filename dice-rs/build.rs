use std::{env, path::PathBuf};

fn main() {
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

    let dst = cfg.build();

    let lib_dir = dst.join("lib");
    let alt_lib_dir = dst.join("build");
    if lib_dir.exists() {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
    } else {
        println!("cargo:rustc-link-search=native={}", alt_lib_dir.display());
    }

    println!("cargo:rustc-link-lib=dice");

    if cfg!(target_env = "gnu") {
        println!("cargo:rustc-link-lib=stdc++");
    }

    println!("cargo:rerun-if-changed={}", dice_src.display());

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
