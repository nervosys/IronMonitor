//! The dependencies of a cross-platform feature must exist on every platform.
//!
//! From the initial commit until 2.1.2, the CLI, TUI, GUI, remote-AI-backend, and
//! logging dependencies — along with the unconditional `chrono` and `async-trait` —
//! sat below `[target.'cfg(windows)'.dependencies]` with no `[dependencies]` header
//! to close it. TOML therefore read all of them as Windows-only.
//!
//! The consequence was total: on Linux and macOS the `cli`, `gui`, and
//! `remote-backends` features enabled code whose crates were not in the dependency
//! graph, so the build failed outright with "cannot find crate `crossterm`" and a
//! dozen more like it. Every one of the eight versions published to crates.io
//! carried it, while the README advertised a native desktop app for Windows, Linux,
//! and macOS.
//!
//! It survived nine months of green local builds because the author's machine was
//! Windows, where the manifest is correct, and because a section header is
//! invisible when you are reading the dependency underneath it — every individual
//! line was right.
//!
//! `cargo tree --target x86_64-unknown-linux-gnu` would have shown it at any point.
//! So this test asks the manifest the same question directly.

use std::collections::BTreeSet;

/// Features that must resolve on every platform.
///
/// This started as just `cli`, `gui`, and `remote-backends` — the obviously
/// cross-platform ones — and that was too narrow. `nvidia` names `nvml-wrapper`,
/// which was declared only in the Linux and Windows target sections, so
/// `--all-features` on macOS failed with `unresolved import 'nvml_wrapper'`. A
/// *feature* is not target-conditional even when the hardware it describes is:
/// enabling it on a platform that lacks the dependency is a build failure, not a
/// graceful absence. So every feature is checked, not a chosen few.
///
/// Crates that are target-gated on purpose, and whose *source* is gated to match.
///
/// Naming a target-gated crate from a feature is only safe when every `use` of it
/// is also behind a `cfg(target_os)`. `drm`/`drm-ffi` are Linux kernel interfaces
/// and `plist` is a macOS format; the code touching each is gated accordingly, so
/// enabling `intel` or `apple` elsewhere compiles to nothing rather than failing.
///
/// `nvml-wrapper` was *not* on this list and was not gated that way: the `nvidia`
/// feature named it, the source guarded only on the feature, and `--all-features`
/// on macOS failed with `unresolved import 'nvml_wrapper'`. It is now declared
/// cross-platform instead, which is why it does not appear here.
///
/// `objc2`/`objc2-app-kit` are AppKit bindings the `gui` feature uses to measure
/// the macOS screen; their only use is `screen_room` and `display_ppi` in
/// `src/gui/mod.rs`, each behind `cfg(target_os = "macos")`.
///
/// Anything new that reaches this list needs the same discipline, so adding to it
/// should be a conscious act rather than a way to quiet the test.
const PLATFORM_GATED_BY_DESIGN: &[&str] = &["drm", "drm-ffi", "plist", "objc2", "objc2-app-kit"];

fn features_to_check(manifest: &toml::Value) -> Vec<String> {
    manifest
        .get("features")
        .and_then(toml::Value::as_table)
        .map(|table| {
            table
                .keys()
                .filter(|name| name.as_str() != "default")
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn manifest() -> toml::Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let text = std::fs::read_to_string(path).expect("Cargo.toml should be readable");
    text.parse::<toml::Value>()
        .expect("Cargo.toml should be valid TOML")
}

/// Dependency names declared in the plain `[dependencies]` table — the ones that
/// exist regardless of target. Anything under `[target.'cfg(...)'.dependencies]`
/// is deliberately excluded: that is the distinction being tested.
fn portable_dependencies(manifest: &toml::Value) -> BTreeSet<String> {
    manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .map(|table| table.keys().cloned().collect())
        .unwrap_or_default()
}

/// Every crate a feature turns on, following feature-to-feature references.
fn required_crates(manifest: &toml::Value, feature: &str) -> BTreeSet<String> {
    let features = manifest
        .get("features")
        .and_then(toml::Value::as_table)
        .expect("[features] should exist");

    let mut resolved = BTreeSet::new();
    let mut pending = vec![feature.to_string()];
    let mut seen = BTreeSet::new();

    while let Some(name) = pending.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(entries) = features.get(&name).and_then(toml::Value::as_array) else {
            // Not a feature name, so it names a crate.
            resolved.insert(name);
            continue;
        };
        for entry in entries {
            let Some(entry) = entry.as_str() else {
                continue;
            };
            // `dep:foo` and `foo/bar` forms both ultimately name a crate.
            let base = entry
                .strip_prefix("dep:")
                .unwrap_or(entry)
                .split('/')
                .next()
                .unwrap_or(entry);
            if features.contains_key(base) {
                pending.push(base.to_string());
            } else {
                resolved.insert(base.to_string());
            }
        }
    }

    // The feature's own name is not a crate.
    resolved.remove(feature);
    resolved
}

#[test]
fn cross_platform_features_depend_only_on_cross_platform_crates() {
    let manifest = manifest();
    let portable = portable_dependencies(&manifest);
    assert!(
        !portable.is_empty(),
        "no [dependencies] table found; this check would pass vacuously"
    );

    let features = features_to_check(&manifest);
    assert!(
        !features.is_empty(),
        "no features to check; this test would pass vacuously"
    );

    let mut problems = Vec::new();
    for feature in &features {
        for krate in required_crates(&manifest, feature) {
            if PLATFORM_GATED_BY_DESIGN.contains(&krate.as_str()) {
                continue;
            }
            if !portable.contains(&krate) {
                problems.push(format!(
                    "feature `{feature}` needs `{krate}`, which is not in [dependencies] \
                     — so it is missing on at least one platform"
                ));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "the manifest does not build everywhere it claims to:\n  {}\n\
         Confirm with: cargo tree --target x86_64-unknown-linux-gnu --features full",
        problems.join("\n  ")
    );
}

/// Crates used unconditionally in the source must not be target-gated.
///
/// `chrono` and `async-trait` are imported without any `cfg`, yet both were trapped
/// in the Windows block. Nothing about their use hints at a platform, which is
/// exactly why nobody looked.
#[test]
fn unconditionally_used_crates_are_available_on_every_platform() {
    const ALWAYS_USED: &[&str] = &["chrono", "async-trait", "serde", "serde_json", "toml"];

    let manifest = manifest();
    let portable = portable_dependencies(&manifest);

    let missing: Vec<&str> = ALWAYS_USED
        .iter()
        .copied()
        .filter(|krate| !portable.contains(*krate))
        .collect();

    assert!(
        missing.is_empty(),
        "these crates are used without a cfg guard but are not in [dependencies], \
         so the build breaks on any platform whose target section omits them: {missing:?}"
    );
}
