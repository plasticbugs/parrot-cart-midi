// Builds Munt's libmt32emu (vendor/munt/mt32emu) as a static library with the
// `cc` crate, so no CMake is needed. The C interface is bound by hand in
// src/mt32.rs.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const MT32EMU: &str = "vendor/munt/mt32emu/src";

// The library's own source list (mt32emu/CMakeLists.txt), with the
// internal resampler and the C interface.
const SOURCES: &[&str] = &[
    "Analog.cpp",
    "BReverbModel.cpp",
    "Display.cpp",
    "File.cpp",
    "FileStream.cpp",
    "LA32FloatWaveGenerator.cpp",
    "LA32Ramp.cpp",
    "LA32WaveGenerator.cpp",
    "MidiStreamParser.cpp",
    "Part.cpp",
    "Partial.cpp",
    "PartialManager.cpp",
    "Poly.cpp",
    "ROMInfo.cpp",
    "Synth.cpp",
    "Tables.cpp",
    "TVA.cpp",
    "TVF.cpp",
    "TVP.cpp",
    "sha1/sha1.cpp",
    "SampleRateConverter.cpp",
    "c_interface/c_interface.cpp",
    "srchelper/srctools/src/FIRResampler.cpp",
    "srchelper/srctools/src/SincResampler.cpp",
    "srchelper/srctools/src/IIR2xResampler.cpp",
    "srchelper/srctools/src/LinearResampler.cpp",
    "srchelper/srctools/src/ResamplerModel.cpp",
    "srchelper/InternalResampler.cpp",
];

fn main() {
    let src = Path::new(MT32EMU);
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let version = munt_version();

    // What CMake would generate from src/config.h.in for a static build.
    let include = out.join("mt32emu-config");
    fs::create_dir_all(&include).unwrap();
    fs::write(
        include.join("config.h"),
        format!(
            "#ifndef MT32EMU_CONFIG_H\n#define MT32EMU_CONFIG_H\n\
             #define MT32EMU_VERSION \"{}.{}.{}\"\n\
             #define MT32EMU_VERSION_MAJOR {}\n#define MT32EMU_VERSION_MINOR {}\n#define MT32EMU_VERSION_PATCH {}\n\
             #define MT32EMU_EXPORTS_TYPE 3\n#undef MT32EMU_SHARED\n\
             #define MT32EMU_WITH_VERSION_TAGGING 0\n#undef MT32EMU_RUNTIME_VERSION_CHECK\n\
             #endif\n",
            version.0, version.1, version.2, version.0, version.1, version.2
        ),
    )
    .unwrap();

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .include(&include)
        .include(src)
        .warnings(false)
        .define("MT32EMU_WITH_INTERNAL_RESAMPLER", None)
        .define("MT32EMU_WITH_SNPRINTF", None)
        .define("MT32EMU_WITH_EXP2F", None);
    if target_env == "msvc" {
        build.define("_CRT_SECURE_CPP_OVERLOAD_STANDARD_NAMES", "1");
    }
    for file in SOURCES {
        build.file(src.join(file));
    }

    // With GNU toolchains, link libstdc++ statically when it is available, so
    // the program runs on systems with an older libstdc++ (Linux) and needs no
    // extra DLLs (MinGW).
    let static_stdcxx = target_env == "gnu" && (target_os == "linux" || target_os == "windows");
    let stdcxx_dir = if static_stdcxx { static_libstdcxx_dir(&build) } else { None };
    if stdcxx_dir.is_some() {
        build.cpp_link_stdlib(None);
    }
    build.compile("mt32emu");
    if let Some(dir) = stdcxx_dir {
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=static=stdc++");
    }

    println!("cargo:rerun-if-changed={MT32EMU}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-env=MT32EMU_VERSION={}.{}.{}", version.0, version.1, version.2);
}

// The version from mt32emu/cmake/project_data.cmake.
fn munt_version() -> (u32, u32, u32) {
    let text = fs::read_to_string("vendor/munt/mt32emu/cmake/project_data.cmake").unwrap();
    let get = |key: &str| -> u32 {
        let line = text.lines().find(|l| l.contains(key)).unwrap_or_else(|| panic!("{key} not in project_data.cmake"));
        line.trim_end_matches(')').rsplit(' ').next().unwrap().trim().parse().unwrap()
    };
    (get("libmt32emu_VERSION_MAJOR "), get("libmt32emu_VERSION_MINOR "), get("libmt32emu_VERSION_PATCH "))
}

// Where the C++ compiler keeps libstdc++.a, if it has one.
fn static_libstdcxx_dir(build: &cc::Build) -> Option<PathBuf> {
    let compiler = build.get_compiler();
    let output = Command::new(compiler.path()).arg("-print-file-name=libstdc++.a").output().ok()?;
    let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    if path.is_absolute() && path.is_file() { path.parent().map(Path::to_path_buf) } else { None }
}
