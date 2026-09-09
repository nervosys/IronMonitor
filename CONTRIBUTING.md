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
2. **Build** with `cargo check --features full` to ensure your changes compile.
3. **Test** with `cargo test --lib --features full`.
4. **Lint** — aim for zero warnings (`cargo clippy --features full`).

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
