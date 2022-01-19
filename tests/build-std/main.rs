//! A test suite for `-Zbuild-std` which is much more expensive than the
//! standard test suite.
//!
//! This test suite attempts to perform a full integration test where we
//! actually compile the standard library from source (like the real one) and
//! the various tests associated with that.
//!
//! YOU SHOULD IDEALLY NOT WRITE TESTS HERE.
//!
//! If possible, use `tests/testsuite/standard_lib.rs` instead. That uses a
//! 'mock' sysroot which is much faster to compile. The tests here are
//! extremely intensive and are only intended to run on CI and are theoretically
//! not catching any regressions that `tests/testsuite/standard_lib.rs` isn't
//! already catching.
//!
//! All tests here should use `#[cargo_test(build_std)]` to indicate that
//! boilerplate should be generated to require the nightly toolchain and the
//! `CARGO_RUN_BUILD_STD_TESTS` env var to be set to actually run these tests.
//! Otherwise the tests are skipped.

use cargo_test_support::*;
use std::env;
use std::path::Path;

#[cargo_test(build_std)]
fn basic() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
            cargo-features = ["build-std"]
            [package]
            name = "foo"
            version = "0.0.1"
            edition = "2018"
            build-std = ["std"]
            "#,
        )
        .file(
            "src/main.rs",
            "
                fn main() {
                    foo::f();
                }

                #[test]
                fn smoke_bin_unit() {
                    foo::f();
                }
            ",
        )
        .file(
            "src/lib.rs",
            "
                extern crate alloc;
                extern crate proc_macro;

                /// ```
                /// foo::f();
                /// ```
                pub fn f() {
                }

                #[test]
                fn smoke_lib_unit() {
                    f();
                }
            ",
        )
        .file(
            "tests/smoke.rs",
            "
                #[test]
                fn smoke_integration() {
                    foo::f();
                }
            ",
        )
        .build();

    p.cargo("check").masquerade_as_nightly_cargo().run();
    p.cargo("build")
        .masquerade_as_nightly_cargo()
        // Importantly, this should not say [UPDATING]
        // There have been multiple bugs where every build triggers and update.
        .with_stderr(
            "[COMPILING] foo v0.0.1 [..]\n\
             [FINISHED] dev [..]",
        )
        .run();
    p.cargo("run").masquerade_as_nightly_cargo().run();
    p.cargo("test").masquerade_as_nightly_cargo().run();

    // Check for hack that removes dylibs.
    let deps_dir = Path::new("target").join("debug").join("deps");
    assert!(p.glob(deps_dir.join("*.rlib")).count() > 0);
    assert_eq!(p.glob(deps_dir.join("*.dylib")).count(), 0);
}

#[cargo_test(build_std)]
fn cross_custom() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                cargo-features = ["build-std"]
                [package]
                name = "foo"
                version = "0.1.0"
                edition = "2018"
                build-std = ["core"]

                [target.custom-target.dependencies]
                dep = { path = "dep" }
            "#,
        )
        .file(
            "src/lib.rs",
            "#![no_std] pub fn f() -> u32 { dep::answer() }",
        )
        .file("dep/Cargo.toml", &basic_manifest("dep", "0.1.0"))
        .file("dep/src/lib.rs", "#![no_std] pub fn answer() -> u32 { 42 }")
        .file(
            "custom-target.json",
            r#"
            {
                "llvm-target": "x86_64-unknown-none-gnu",
                "data-layout": "e-m:e-i64:64-f80:128-n8:16:32:64-S128",
                "arch": "x86_64",
                "target-endian": "little",
                "target-pointer-width": "64",
                "target-c-int-width": "32",
                "os": "none",
                "linker-flavor": "ld.lld"
            }
            "#,
        )
        .build();

    p.cargo("build --target custom-target.json -v")
        .masquerade_as_nightly_cargo()
        .run();
}

#[cargo_test(build_std)]
fn custom_test_framework() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                cargo-features = ["build-std"]
                [package]
                name = "foo"
                version = "0.1.0"
                edition = "2018"
                build-std = ["core"]
            "#,
        )
        .file(
            "src/lib.rs",
            r#"
            #![no_std]
            #![cfg_attr(test, no_main)]
            #![feature(custom_test_frameworks)]
            #![test_runner(crate::test_runner)]

            pub fn test_runner(_tests: &[&dyn Fn()]) {}

            #[panic_handler]
            fn panic(_info: &core::panic::PanicInfo) -> ! {
                loop {}
            }
            "#,
        )
        .file(
            "target.json",
            r#"
            {
                "llvm-target": "x86_64-unknown-none-gnu",
                "data-layout": "e-m:e-i64:64-f80:128-n8:16:32:64-S128",
                "arch": "x86_64",
                "target-endian": "little",
                "target-pointer-width": "64",
                "target-c-int-width": "32",
                "os": "none",
                "linker-flavor": "ld.lld",
                "linker": "rust-lld",
                "executables": true,
                "panic-strategy": "abort"
            }
            "#,
        )
        .build();

    // This is a bit of a hack to use the rust-lld that ships with most toolchains.
    let sysroot = paths::sysroot();
    let sysroot = Path::new(&sysroot);
    let sysroot_bin = sysroot
        .join("lib")
        .join("rustlib")
        .join(rustc_host())
        .join("bin");
    let path = env::var_os("PATH").unwrap_or_default();
    let mut paths = env::split_paths(&path).collect::<Vec<_>>();
    paths.insert(0, sysroot_bin);
    let new_path = env::join_paths(paths).unwrap();

    p.cargo("test --target target.json --no-run -v")
        .masquerade_as_nightly_cargo()
        .env("PATH", new_path)
        .run();
}

#[cargo_test(build_std)]
fn forced_custom_target() {
    // Checks how per-package-targets interct with build-std.

    let p = project()
        .file(
            "src/main.rs",
            "
        #![no_std]
        #![no_main]

        #[panic_handler]
        fn panic(_info: &core::panic::PanicInfo) -> ! {
            loop {}
        }

        pub fn foo() -> u8 { 42 }
        ",
        )
        .file(
            "Cargo.toml",
            r#"
                cargo-features = ["build-std", "per-package-target"]
                [package]
                name = "foo"
                version = "0.1.0"
                edition = "2018"
                build-std = ["core"]
                forced-target = "custom-target.json"
            "#,
        )
        .file(
            "custom-target.json",
            r#"
            {
                "llvm-target": "x86_64-unknown-none-gnu",
                "data-layout": "e-m:e-i64:64-f80:128-n8:16:32:64-S128",
                "arch": "x86_64",
                "target-endian": "little",
                "target-pointer-width": "64",
                "target-c-int-width": "32",
                "os": "none",
                "linker-flavor": "ld.lld",
                "linker": "rust-lld",
                "executables": true,
                "panic-strategy": "abort"
            }
            "#,
        )
        .build();

    p.cargo("build").masquerade_as_nightly_cargo().run();

    assert!(p.target_bin("custom-target", "foo").exists());
}
