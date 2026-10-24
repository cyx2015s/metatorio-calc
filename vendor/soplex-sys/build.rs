use std::env;
use cmake::Config;

fn target_has_feature(feature: &str) -> bool {
    env::var("CARGO_CFG_TARGET_FEATURE")
        .unwrap_or_default()
        .split(',')
        .any(|enabled| enabled == feature)
}

fn build_and_link_soplex() {
    // Build SoPlex in Release even for Rust dev profiles: the bundled C++
    // libraries must use the same release CRT / iterator-debug settings as
    // highs-sys, otherwise MSVC reports LNK2038 mismatches.
    let build_type = "Release";
    let target = env::var("TARGET").unwrap();
    let msvc = target.contains("msvc");
    let crt_static = target_has_feature("crt-static");

    let mut config = Config::new("soplex");
    config
        .profile(build_type)
        .define("GMP", "off")
        .define("BOOST", "off")
        .define("QUADMATH", "off")
        .define("PAPILO", "off")
        .define("BUILD_SHARED_LIBS", "OFF");

    if msvc {
        // SoPlex 7.1's CMakeLists pins C++14, but its bundled fmt requires
        // C++17. /EHsc is also required by the C++ standard library.
        config
            .cxxflag("/std:c++17")
            .cxxflag("/EHsc")
            .cxxflag("/D_SECURE_SCL=0")
            .define(
                "CMAKE_MSVC_RUNTIME_LIBRARY",
                if crt_static {
                    "MultiThreaded"
                } else {
                    "MultiThreadedDLL"
                },
            );
    } else {
        config.cxxflag("-std=c++17");
    }

    let dst = config.build();

    // SoPlex's CMake target is named "libsoplex", so the installed MSVC
    // archive is "libsoplex.lib". Rust's "-l static=soplex" looks for
    // "soplex.lib", so provide an alias in the install directory.
    if msvc {
        let library = dst.join("lib").join("libsoplex.lib");
        if library.is_file() {
            let alias = dst.join("lib").join("soplex.lib");
            if !alias.exists() {
                std::fs::copy(&library, &alias).expect("copy libsoplex.lib to soplex.lib");
            }
        } else {
            panic!("SoPlex build did not produce {}", library.display());
        }
    }

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-search=native={}/lib64", dst.display());
    println!("cargo:rustc-link-lib=static=soplex");
    let apple = target.contains("apple");
    let linux = target.contains("linux");
    let mingw = target.contains("pc-windows-gnu");
    if apple {
        println!("cargo:rustc-link-lib=dylib=c++");
    } else if linux || mingw {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }
    println!("cargo:rerun-if-changed=soplex/src/soplex_interface.h");
}

fn main() {
    if std::env::var("DOCS_RS").is_err() {
        build_and_link_soplex();
    }

    let bindings = bindgen::Builder::default()
        .header("soplex/src/soplex_interface.h")
        .allowlist_function("SoPlex_.*")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
