# Contributing to IronMonitor

Thank you for your interest in contributing! We welcome contributions from the community.

## Contributor License Agreement (CLA)

Before your contribution can be accepted, you must agree to our
[Contributor License Agreement](CLA.md). By submitting a pull request, you
indicate your agreement to the CLA terms.

**Why a CLA?** IronMonitor is dual-licensed under the AGPL v3 (open source)
and a commercial license. The CLA ensures that contributions can be distributed
under both licenses, enabling the project to remain sustainable while staying
open source.

## Getting Started

1. **Fork** the repository and create a feature branch from `master`.
2. **Verify** with all five checks below before opening a pull request.

### Verifying a change

CI runs `cargo test --all-features` on Linux, Windows and macOS, plus separate
Clippy and Format jobs. A change is verified locally when all five of these
pass — the last one run last, after every other edit:

```bash
cargo check --all-targets --all-features                          # your platform
cargo clippy --all-targets --all-features                         # zero warnings
cargo check --target aarch64-apple-darwin --features full --lib   # macOS, from anywhere
cargo test --all-features                                         # includes doctests
cargo fmt --all -- --check                                        # immediately before commit
```

Each line is there because leaving it out has broken `master`:

- **`--all-features`, not `--features full`.** `full` omits `fleet-store`, so
  code behind that feature goes unchecked.
- **`cargo test`, not `cargo test --lib`.** `--lib` skips the doctests and every
  integration test in `tests/`. A doc example is compiled against the public API,
  so it is exactly what a type change breaks.
- **The macOS cross-check.** Code under `#[cfg(target_os = "macos")]` does not
  compile anywhere else, so a Windows or Linux build cannot see it. `cargo check`
  does not link, so this works without a Mac: run
  `rustup target add aarch64-apple-darwin` once. It uses `--features full`
  because `fleet-store` needs a C cross-compiler for `zstd-sys`. Linux-only code
  needs a Linux build (WSL2 works).
- **`fmt --check` last.** Formatting partway through and then adding more code —
  tests appended by a script, say — commits unformatted code that CI rejects.

Two cautions about the results:

- **Read the exit code from `cargo` itself**, not from the end of a pipe.
  `cargo test | grep ...` reports `grep`'s status, so a run that was killed
  partway through reads as a pass.
- **Don't run two `cargo` commands against the same target directory at once.**
  The second one relinks the `ironmon` binary while the first is still running
  it, and fails with a linker error that looks transient and isn't.

Several integration tests drive the built binary against real hardware, so the
full suite takes minutes rather than seconds.

## Development Guidelines

- Follow the patterns documented in [.github/copilot-instructions.md](.github/copilot-instructions.md).
- All GPU code should use the `Device` trait from `src/gpu/traits.rs`.
- Platform-specific code must use `#[cfg]` guards and feature flags.
- All metric structs must derive `Serialize`/`Deserialize`.
- Public APIs require `///` doc comments.

## Pull Request Process

1. Ensure your PR has a clear title and description.
2. Link any related issues.
3. All CI checks must pass.
4. Maintainers will review and may request changes.

## Code of Conduct

Be respectful, constructive, and inclusive. We follow the
[Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct).

## Questions?

Open a [Discussion](https://github.com/nervosys/IronMonitor/discussions) or
reach out at licensing@nervosys.com for licensing questions.
