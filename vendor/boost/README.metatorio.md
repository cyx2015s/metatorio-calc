# Vendored Boost headers for SoPlex exact mode

SoPlex's rational solve uses Boost.Multiprecision's `cpp_rational` when GMP is
not available. This directory contains only the transitive header closure needed
by SoPlex 7.1.2, plus Boost.MPL's preprocessed headers that are selected by
preprocessor macros at compile time.

Source: Boost 1.87.0, `lib/native/include` from the NuGet package, copied from
the Boost 1.87.0 distribution. License: Boost Software License 1.0 (see
`LICENSE_1_0.txt`).

This is a build-time header dependency only. It does not change SoPlex's
license; SoPlex itself is Apache-2.0.
