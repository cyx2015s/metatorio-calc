# Local SoPlex sys patch

This is a vendored copy of `soplex-sys 0.1.6` (Apache-2.0), based on
`mmghannam/soplex-sys` at commit `49cf3605257d3f567ed2774f168ce5c5fd640a10`. The workspace patches crates.io through the root
`[patch.crates-io]` entry.

Changes in `build.rs`:

- force C++17 and `/EHsc` on MSVC (SoPlex 7.1's CMakeLists pins C++14, but its
  bundled fmt requires C++17 and the C++ runtime needs exception handling);
- define `_SECURE_SCL=0` so the old bundled fmt does not use the removed
  `stdext::checked_array_iterator`;
- build the C++ library in Release even for Rust dev profiles, matching the
  release CRT / iterator-debug settings used by `highs-sys`;
- set the MSVC runtime to match Rust's `crt-static` setting;
- create `soplex.lib` as an alias of SoPlex's actual `libsoplex.lib` output,
  because this crate links `static=soplex`;
- drop the unconditional `z.lib` link (the bundled SoPlex is built without
  zlib; Windows has no `z.lib`);
- disable shared libraries and add `rerun-if-changed` for the C interface.

The bundled build still disables GMP, Boost and quadmath. This is a
floating-point SoPlex, not its rational exact mode.
