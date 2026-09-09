# IronMonitor (ironmon) - AI Agent Instructions

## Project Overview

**IronMonitor** is a comprehensive Rust library for cross-platform hardware monitoring, providing unified APIs for CPUs, GPUs (NVIDIA/AMD/Intel), memory, disks, motherboards, processes, and network interfaces. Originally evolved from a Jetson-specific monitoring tool (ironmon), it now supports multi-vendor GPU monitoring across Linux/Windows/macOS.

**Key Architecture**: Trait-based GPU abstraction (`Device` trait in `src/gpu/traits.rs`) with vendor-specific backends, platform-specific implementations under `src/platform/`, and unified monitoring APIs in `src/`.

## Critical Patterns & Conventions

### 1. GPU Backend Architecture (Multi-Vendor Abstraction)

All GPU backends implement the `Device` trait from `src/gpu/traits.rs`. When adding GPU features:

```rust
// ✅ CORRECT: Use the Device trait
use crate::gpu::traits::Device;

impl Device for MyGpuBackend {
    fn name(&self) -> Result<String, Error> { /* ... */ }
    fn vendor(&self) -> Vendor { /* ... */ }
    fn temperature(&self) -> Result<Temperature, Error> { /* ... */ }
    // ... other required methods
}

// ✅ CORRECT: Use GpuCollection for auto-detection
let gpus = GpuCollection::auto_detect()?;
```

**Don't** create vendor-specific APIs - extend the unified `Device` trait. Legacy code exists (`src/gpu/nvidia.rs`, `src/gpu/amd.rs`) but new code should use trait-based system (`src/gpu/nvidia_new.rs`, `src/gpu/amd_rocm.rs`, `src/gpu/intel_levelzero.rs`).

### 2. Feature Flags Control Platform/Vendor Support

Feature flags are mandatory for platform-specific or vendor-specific code:

```rust
// ✅ CORRECT: GPU vendor features
#[cfg(feature = "nvidia")]
pub mod nvidia_new;

#[cfg(feature = "amd")]
pub mod amd_rocm;

// ✅ CORRECT: Platform-specific implementations
#[cfg(target_os = "linux")]
mod linux;

#[cfg(windows)]
use windows::Win32::System::SystemInformation;
```

**Available features**: `nvidia`, `amd`, `intel`, `apple`, `cli`, `full` (see `Cargo.toml`). CLI/TUI features require `cli` flag.

### 3. Platform-Specific Implementations Pattern

Platform implementations follow a consistent structure under `src/platform/`:

```
src/platform/
├── common.rs         # Shared utilities
├── mod.rs            # Platform selector
├── linux/            # Linux implementation
│   ├── cpu.rs
│   ├── gpu.rs
│   ├── jetson.rs     # Jetson-specific (NVIDIA embedded)
│   └── ...
└── windows.rs        # Windows stub/implementation
```

**Linux**: Reads from `/proc`, `/sys/class/`, and device-specific paths (e.g., `/sys/class/drm/card*/device/` for AMD/Intel GPUs)
**Windows**: Uses Windows API via `windows` crate (currently partial implementation)
**macOS**: Uses sysfs equivalents and IOKit (partial implementation)

### 4. Process Monitoring with GPU Attribution

`ProcessMonitor` correlates system processes with GPU usage by matching PIDs:

```rust
// ✅ CORRECT: Always initialize with GpuCollection for attribution
let gpus = GpuCollection::auto_detect()?;
let mut monitor = ProcessMonitor::with_gpus(gpus)?;

// Get GPU processes
let gpu_procs = monitor.processes_by_gpu_memory()?;
```

GPU process data comes from NVML (`nvidia_new.rs`), AMD sysfs (`/sys/class/drm/card*/device/fdinfo`), or Intel debugfs.

### 5. Error Handling Convention

Use `thiserror` for error types. Main error type is `IronError` in `src/error.rs`:

```rust
// ✅ CORRECT: Use typed errors
use crate::error::{IronError, Result};

pub fn my_function() -> Result<Data> {
    // Platform-specific check
    #[cfg(not(target_os = "linux"))]
    return Err(IronError::NotImplemented(
        "Only supported on Linux".into()
    ));
    
    // GPU errors use GpuError from traits
    gpu.temperature()
        .map_err(|e| IronError::GpuError(e.to_string()))
}
```

GPU-specific errors use `gpu::traits::Error`, wrapped into `IronError::GpuError` at API boundaries.

### 6. Serde Serialization for All Metrics

All metric structs derive `Serialize`/`Deserialize` for JSON export:

```rust
// ✅ CORRECT: Always add serde derives to new metric types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyMetrics {
    pub value: f64,
    pub timestamp: u64,
}
```

This enables `--format json` in CLI and API integration.

## Development Workflows

### Building & Testing

```bash
# Fast check (no codegen)
cargo check --features full

# Build with specific vendor
cargo build --release --features nvidia
cargo build --release --features amd,intel
cargo build --release --features full  # All vendors

# Run examples (best way to test)
cargo run --release --features nvidia --example gpu_monitor
cargo run --release --example process_monitor --features nvidia
cargo run --release --example tui --features cli

# Tests (limited, mostly on GPU backends)
cargo test --features full
```

**Important**: Most testing is done via examples (`examples/*.rs`). Run relevant examples after changes.

### Adding New Metrics to Existing Monitors

1. Add field to metric struct in `src/core/` or `src/gpu/traits.rs` with serde derives
2. Update platform implementations (`src/platform/linux/*.rs`, etc.)
3. Update display logic if using TUI (`src/tui/ui.rs`)
4. Update relevant example in `examples/`
5. Test with `cargo run --example <name>`

### Adding New GPU Backend

1. Create `src/gpu/my_vendor.rs` (or `my_vendor_backend.rs` for new trait system)
2. Implement `Device` trait from `src/gpu/traits.rs`
3. Add feature flag to `Cargo.toml`: `my_vendor = []`
4. Add conditional compilation in `src/gpu/mod.rs`:
   ```rust
   #[cfg(feature = "my_vendor")]
   pub mod my_vendor;
   ```
5. Update `GpuCollection::auto_detect()` to include new backend
6. Create example: `examples/my_vendor_monitor.rs`
7. Update README.md platform support matrix

### Security Considerations

**CRITICAL**: Code in `src/utils/` (swap, clocks, power_mode) has security issues - see `SECURITY.md`:
- ❌ **DO NOT USE** in production: Command injection risks in `utils/swap.rs`
- ❌ **DO NOT USE**: Unchecked sudo in `utils/clocks.rs` and `utils/power_mode.rs`
- ✅ **SAFE**: All monitoring code (read-only sysfs/procfs) in `src/core/`, `src/gpu/`, `src/platform/`

When adding utilities that execute commands or require elevated privileges, implement proper validation and auditing first.

## File Organization Reference

```
src/
├── lib.rs                    # Public API, re-exports, IronMonitor wrapper
├── error.rs                  # IronError, Result type
├── stats.rs                  # ironmon legacy API (Jetson-focused)
├── gpu/
│   ├── mod.rs                # GpuCollection, vendor enum, unified API
│   ├── traits.rs             # Device trait (PREFERRED for new code)
│   ├── nvidia_new.rs         # NVML backend (new trait-based)
│   ├── amd_rocm.rs           # AMD sysfs backend (new trait-based)
│   ├── intel_levelzero.rs    # Intel i915/xe backend (new trait-based)
│   ├── nvidia.rs             # Legacy NVIDIA (backward compat)
│   └── amd.rs, intel.rs      # Legacy backends
├── process_monitor.rs        # System processes + GPU attribution
├── network_monitor.rs        # Network interface stats
├── core/                     # Core metric structs (CPU, memory, power, etc.)
├── platform/                 # Platform-specific implementations
│   ├── linux/                # Linux: /proc, /sys, device paths
│   │   ├── jetson.rs         # NVIDIA Jetson embedded specific
│   │   └── ...
│   └── windows.rs            # Windows: Win32 API (partial)
├── tui/                      # Terminal UI (requires 'cli' feature)
│   ├── app.rs                # State management
│   └── ui.rs                 # ratatui rendering
└── utils/                    # ⚠️ UNSAFE utilities (swap, clocks, power_mode)
```

## Common Tasks

### "Add support for a new GPU metric"

1. Add field to `gpu::traits::Temperature`, `Power`, `Clocks`, etc. struct
2. Update vendor implementations (`nvidia_new.rs`, `amd_rocm.rs`, `intel_levelzero.rs`)
3. Update `GpuInfo` and `GpuDynamicInfo` in `src/gpu/mod.rs` if needed
4. Test with `cargo run --example all_gpus --features full`

### "Fix Windows/macOS support"

1. Check `src/platform/windows.rs` or `src/platform/macos.rs` for stubs
2. Implement platform-specific system calls (e.g., Windows: use `windows` crate APIs)
3. Add `#[cfg(windows)]` or `#[cfg(target_os = "macos")]` guards
4. Update platform support matrix in README.md

### "Add a new monitoring category"

1. Create module under `src/` (e.g., `src/my_monitor.rs`)
2. Define metric structs with `Serialize`/`Deserialize` derives
3. Implement platform-specific readers in `src/platform/linux/`, `src/platform/windows.rs`
4. Add public API to `src/lib.rs`
5. Create example in `examples/my_monitor.rs`
6. Update README.md with new feature

## Naming Conventions

- **Modules**: snake_case (`network_monitor`, `process_monitor`)
- **Structs**: PascalCase (`GpuCollection`, `ProcessMonitor`, `NetworkInterfaceInfo`)
- **Functions**: snake_case (`snapshot_all`, `auto_detect`, `bandwidth_rate`)
- **Feature flags**: lowercase with hyphens (`nvidia`, `full`, `cli`)
- **Examples**: descriptive names with vendor (`nvidia_monitor.rs`, `all_gpus.rs`, `process_monitor.rs`)

## Documentation Standards

- All public APIs require `///` doc comments with examples (see `src/lib.rs`, `src/gpu/mod.rs`)
- Module-level docs explain architecture and usage patterns (`//!` at top of file)
- Examples are runnable code blocks: `/// # Examples\n/// ```no_run`
- Platform requirements documented in module docs (e.g., "Requires amdgpu driver")

