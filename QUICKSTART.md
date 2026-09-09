# IronMonitor — Quick Start Guide

Get up and running with IronMonitor in under 5 minutes.

## Prerequisites

- **Rust 1.70+** — [install here](https://rustup.rs/)
- **Linux**: `build-essential`, `pkg-config`, `libdrm-dev` (for AMD/Intel GPU support)
- **Windows**: Visual Studio Build Tools (installed with Rust)
- **GPU drivers**: NVIDIA CUDA toolkit or driver (for NVIDIA support)

## Installation

### As a Library

Add to your `Cargo.toml`:

```toml
[dependencies]
iron-monitor = { version = "0.3", features = ["nvidia"] }

# Or pick specific vendors:
# iron-monitor = { version = "0.3", features = ["nvidia", "amd", "intel"] }

# Or everything:
# iron-monitor = { version = "0.3", features = ["full"] }
```

### CLI Tool

```bash
# Install from source
git clone https://github.com/nervosys/IronMonitor
cd IronMonitor
cargo build --release --features cli

# Run the TUI monitor
cargo run --release --features cli --example tui

# Or use the CLI directly
cargo run --release --features cli -- gpu
cargo run --release --features cli -- cpu
cargo run --release --features cli -- --format json all
```

## First Program

```rust
use ironmonlib::gpu::GpuCollection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Auto-detect all available GPUs
    let gpus = GpuCollection::auto_detect()?;

    for (idx, info) in gpus.snapshot_all()?.iter().enumerate() {
        println!("GPU {}: {}", idx, info.static_info.name);
        println!("  Vendor: {:?}", info.static_info.vendor);
        println!("  Utilization: {}%", info.dynamic_info.utilization);
        println!("  Memory: {} / {} MB",
            info.dynamic_info.memory.used / 1024 / 1024,
            info.dynamic_info.memory.total / 1024 / 1024);
        if let Some(temp) = info.dynamic_info.thermal.temperature {
            println!("  Temperature: {}°C", temp);
        }
        if let Some(power) = info.dynamic_info.power.draw {
            println!("  Power: {:.1}W", power as f64 / 1000.0);
        }
    }

    Ok(())
}
```

Run it:

```bash
cargo run --features nvidia
```

## Common Tasks

### Monitor CPU

```rust
use ironmonlib::CpuStats;

let stats = CpuStats::new()?;
println!("Cores: {}, Online: {}", stats.core_count(), stats.online_count());
println!("User: {:.1}%  System: {:.1}%  Idle: {:.1}%",
    stats.total.user, stats.total.system, stats.total.idle);
```

### Monitor Memory

```rust
use ironmonlib::MemoryMonitor;
use ironmonlib::memory_management::format_bytes;

let monitor = MemoryMonitor::new()?;
println!("RAM: {} / {} ({:.1}% used)",
    format_bytes(monitor.memory.used),
    format_bytes(monitor.memory.total),
    monitor.memory.usage_percent());
println!("Swap: {} / {}",
    format_bytes(monitor.swap.used),
    format_bytes(monitor.swap.total));
```

### Monitor Processes with GPU Attribution

```rust
use ironmonlib::{ProcessMonitor, GpuCollection};

let gpus = GpuCollection::auto_detect()?;
let mut monitor = ProcessMonitor::with_gpus(gpus)?;

let processes = monitor.processes()?;
for proc in processes.iter().take(5) {
    println!("{} (PID {}): CPU={:.1}%, Mem={} MB",
        proc.name, proc.pid, proc.cpu_percent,
        proc.memory_bytes / 1024 / 1024);
}
```

### Monitor Network Interfaces

```rust
use ironmonlib::network_monitor::NetworkMonitor;

let mut monitor = NetworkMonitor::new()?;
let interfaces = monitor.interfaces()?;

for iface in interfaces {
    if iface.is_active() {
        let (rx_rate, tx_rate) = monitor.bandwidth_rate(&iface.name, &iface);
        println!("{}: ↓{:.2} MB/s ↑{:.2} MB/s",
            iface.name, rx_rate / 1_000_000.0, tx_rate / 1_000_000.0);
    }
}
```

### Network Diagnostics

```rust
use ironmonlib::{ping, dns_lookup, scan_ports, traceroute};

// Ping
let result = ping("8.8.8.8", 4)?;
println!("RTT: min={:.2}ms avg={:.2}ms max={:.2}ms",
    result.rtt_min_ms, result.rtt_avg_ms, result.rtt_max_ms);

// DNS lookup
let ips = dns_lookup("google.com")?;

// Port scan
let results = scan_ports("192.168.1.1", &[22, 80, 443])?;

// Traceroute
let hops = traceroute("google.com", 30)?;
```

### Use the AI Agent

```rust
use ironmonlib::agent::{Agent, AgentConfig, ModelSize};
use ironmonlib::IronMonitor;

let monitor = IronMonitor::new()?;
let config = AgentConfig::new(ModelSize::Medium);
let mut agent = Agent::new(config)?;

let response = agent.ask("What's my GPU temperature?", &monitor)?;
println!("{}", response.response);
```

Or via CLI:

```bash
# Interactive AI session
amon

# Single query
amon query "What's my GPU temperature?"
```

### JSON Export

All metrics support JSON serialization via `serde`:

```rust
use ironmonlib::gpu::GpuCollection;

let gpus = GpuCollection::auto_detect()?;
let snapshots = gpus.snapshot_all()?;
let json = serde_json::to_string_pretty(&snapshots)?;
println!("{}", json);
```

## Feature Flags

| Feature        | Description                               |
| -------------- | ----------------------------------------- |
| `nvidia`       | NVIDIA GPU support (NVML)                 |
| `amd`          | AMD GPU support (sysfs/DRM)               |
| `intel`        | Intel GPU support (i915/xe)               |
| `apple`        | Apple Silicon (M1/M2/M3/M4)               |
| `cli`          | Command-line interface + TUI              |
| `gui`          | Native GUI (egui)                         |
| `full`         | All features enabled                      |
| `jetson-utils` | Unsafe Jetson utilities (see SECURITY.md) |

## Examples

Run the bundled examples:

```bash
cargo run --example all_gpus --features nvidia,amd,intel
cargo run --example cpu_monitor
cargo run --example process_monitor --features nvidia
cargo run --example network_monitor
cargo run --example disk_monitor
cargo run --example motherboard_monitor
cargo run --example tui --features cli
```

## Troubleshooting

**No GPUs detected**: Ensure GPU drivers are installed and the correct feature flag is enabled.

**NVML not found on Linux**: Install `libnvidia-ml` (comes with NVIDIA driver) or set `LD_LIBRARY_PATH`.

**NVML not found on Windows**: Install the NVIDIA driver; `nvml.dll` is in `C:\Windows\System32`.

**Permission denied on Linux**: Some sysfs paths require root. Run with `sudo` for full hardware access.

**Build errors with `drm` crate**: Install `libdrm-dev` (`apt install libdrm-dev` on Debian/Ubuntu).

See the full [README](README.md) for detailed API documentation and architecture.

