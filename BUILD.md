# Build and Test Instructions

## Prerequisites

### For All Platforms

- Rust 1.70 or later
- Cargo (comes with Rust)

Install Rust:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### For Linux (Jetson & Desktop)

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install build-essential pkg-config

# For NVML support (optional but recommended)
sudo apt-get install nvidia-cuda-toolkit
```

### For Windows

- Visual Studio 2019 or later with C++ build tools
- CUDA Toolkit (for NVML support)

## Building

### Quick Build (Library Only)

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release
```

### Build with Features

```bash
# Build with NVML support
cargo build --release --features nvml

# Build with CLI tool
cargo build --release --features cli

# Build with all features
cargo build --release --features full
```

### Cross-Compilation for Jetson

From x86_64 Linux to ARM64 Jetson:

```bash
# Install target
rustup target add aarch64-unknown-linux-gnu

# Install cross-compiler
sudo apt-get install gcc-aarch64-linux-gnu

# Build
cargo build --release --target aarch64-unknown-linux-gnu

# The binary will be in:
# target/aarch64-unknown-linux-gnu/release/ironmon
```

Using `cross` (easier):

```bash
# Install cross
cargo install cross

# Build for Jetson
cross build --release --target aarch64-unknown-linux-gnu
```

## Testing

### Run All Tests

```bash
cargo test
```

### Run Tests with NVML

```bash
cargo test --features nvml
```

### Run Specific Test

```bash
cargo test cpu_monitoring
```

### Run with Logging

```bash
RUST_LOG=debug cargo test
```

## Running Examples

### Basic Example

```bash
cargo run --example basic
```

### Monitoring Example

```bash
cargo run --example monitoring
```

### GPU Control Example (Jetson, requires root)

```bash
sudo -E cargo run --example gpu_control
```

## Running the CLI

### Build and Install Locally

```bash
cargo install --path . --features cli
```

### Run without Installing

```bash
# Interactive mode
cargo run --features cli

# Specific commands
cargo run --features cli -- gpu
cargo run --features cli -- cpu
cargo run --features cli -- --format json all
```

## Benchmarking

```bash
# Install criterion (if not already in dev-dependencies)
cargo bench
```

## Documentation

### Build Documentation

```bash
cargo doc --no-deps
```

### Build and Open Documentation

```bash
cargo doc --no-deps --open
```

### Build with All Features

```bash
cargo doc --no-deps --all-features --open
```

## Platform-Specific Build Notes

### Linux Jetson

```bash
# On the Jetson device
cargo build --release --features full

# The binary will be in: target/release/ironmon
```

### Linux Desktop with NVIDIA GPU

```bash
# Ensure CUDA toolkit is installed
which nvcc

# Build with NVML
cargo build --release --features nvml,cli
```

### Windows Desktop

```powershell
# In PowerShell
cargo build --release --features full
```

## Troubleshooting

### NVML Not Found

```bash
# Linux: Install CUDA toolkit
sudo apt-get install nvidia-cuda-toolkit

# Or build without NVML
cargo build --release --no-default-features
```

### Permission Denied (Jetson)

```bash
# For reading most stats, no special permissions needed
# For control operations, use sudo:
sudo -E cargo run --example gpu_control
```

### Link Errors on Cross-Compilation

```bash
# Ensure you have the cross-compiler
sudo apt-get install gcc-aarch64-linux-gnu g++-aarch64-linux-gnu

# Set linker in .cargo/config.toml
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
```

## Performance Profiling

### Using perf (Linux)

```bash
# Build with debug symbols
cargo build --release

# Run with perf
perf record --call-graph dwarf target/release/ironmon
perf report
```

### Using flamegraph

```bash
cargo install flamegraph

# Generate flamegraph
cargo flamegraph --example monitoring
```

## Code Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Run coverage
cargo tarpaulin --out Html
```

## Continuous Integration

The project uses GitHub Actions for CI/CD. See `.github/workflows/` for configuration.

Local CI testing:

```bash
# Install act
brew install act  # macOS
# or
sudo snap install act  # Linux

# Run CI locally
act push
```

## Release Build Optimization

For maximum performance:

```bash
# Build with all optimizations
RUSTFLAGS="-C target-cpu=native" cargo build --release --features full

# Strip debug symbols (makes binary smaller)
strip target/release/ironmon
```

## Quick Commands Reference

```bash
# Development
cargo check                          # Fast compile check
cargo clippy                         # Linting
cargo fmt                            # Format code

# Building
cargo build                          # Debug build
cargo build --release                # Release build
cargo build --features cli           # Build with CLI

# Testing
cargo test                           # Run tests
cargo bench                          # Run benchmarks

# Documentation
cargo doc --open                     # Build and open docs

# Installation
cargo install --path . --features cli  # Install locally

# Cleaning
cargo clean                          # Remove build artifacts
```

