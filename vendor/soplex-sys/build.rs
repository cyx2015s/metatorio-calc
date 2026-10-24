use cmake::Config;
use std::env;

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

    let manifest = std::path::PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let boost_root = env::var_os("SOPLEX_BOOST_ROOT")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let vendored = manifest.join("..").join("..").join("vendor").join("boost");
            vendored.is_dir().then_some(vendored)
        })
        .unwrap_or_else(|| {
            panic!(
                "SoPlex exact mode requires Boost headers; set SOPLEX_BOOST_ROOT                  or place them in vendor/boost"
            )
        });

    let mut config = Config::new("soplex");
    config
        .profile(build_type)
        .define("BOOST", "on")
        .define("BOOST_ROOT", &boost_root)
        .define("BOOST_INCLUDEDIR", &boost_root)
        .define("MPFR", "off")
        .define("GMP", "off")
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
            // Always overwrite: an older non-Boost build may have left a stale
            // soplex.lib here, and linking that silently disables rational support.
            std::fs::copy(&library, &alias).expect("copy libsoplex.lib to soplex.lib");
        } else {
            panic!("SoPlex build did not produce {}", library.display());
        }
    }

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-search=native={}/lib64", dst.display());
    println!("cargo:rustc-link-lib=static=soplex");
    // SoPlex's zstr uses zlib when CMake finds it (normal on Linux/macOS).
    // Windows has no z.lib in the build environment, and the bundled CMake
    // build there does not enable zlib, so only link it on non-Windows targets.
    if !target.contains("windows") {
        println!("cargo:rustc-link-lib=z");
    }
    let apple = target.contains("apple");
    let linux = target.contains("linux");
    let mingw = target.contains("pc-windows-gnu");
    if apple {
        println!("cargo:rustc-link-lib=dylib=c++");
    } else if linux || mingw {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }
    println!("cargo:rerun-if-changed=soplex/src/soplex_interface.h");
    println!("cargo:rerun-if-env-changed=SOPLEX_BOOST_ROOT");
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
