# Enhanced CPU Monitoring

## Overview

IronMonitor provides comprehensive CPU monitoring across all platforms with support for:
- Per-core frequency tracking
- Per-core utilization
- Temperature monitoring
- Hybrid architecture support (P-cores + E-cores)
- CPU cluster detection and aggregation

## Platform Support

| Feature                | Linux | Windows | macOS |
| ---------------------- | ----- | ------- | ----- |
| Per-core frequency     | ✅     | 🚧       | ✅     |
| Per-core utilization   | ✅     | ✅       | ✅     |
| Temperature monitoring | ✅     | ❌†      | ❌*    |
| Hybrid CPU support     | ✅     | 🚧       | ✅     |
| Power consumption      | 🚧     | 🚧       | ✅     |

✅ = Implemented | 🚧 = Partial/TODO | ❌ = Not available from platform

*macOS: Thermal pressure available, but not per-core temperatures

†Windows: `ironmon cli temperature` reports no sensors on a desktop Ryzen system.
Most board sensors need a signed kernel driver, which IronMonitor does not ship.

> **This table describes `src/silicon/`, not `CpuStats`.** The two are separate:
> `silicon` is the enhanced per-core and cluster layer, and `CpuStats` /
> `stats::IronMonitor` is the general path — which has **no macOS implementation at all**
> (see the platform table in the README). A ✅ here does not imply the general path
> works on that platform.
>
> The Windows column previously read 🚧 throughout, describing
> `src/silicon/windows.rs` as "basic structure and skeleton" with a "placeholder
> for WMI and Performance Counter integration". It is 644 lines across 25
> functions, and `ironmon cli cpu` returns twenty-four distinct per-core figures on a
> Ryzen 9 9900X — not one average replicated, which is the defect this repository
> has hit before. Per-core utilization is corrected to ✅ on that evidence.
>
> The macOS column is **unverified**. It has never been exercised: until 2.1.2 the
> crate could not build on macOS at all. Treat those marks as claims awaiting a
> Mac, not as measurements.

## Implementation Status

### ✅ Completed

1. **Core Data Structures** (`src/silicon/mod.rs`)
   - `CpuCore`: Per-core metrics (frequency, utilization, temperature)
   - `CpuCluster`: Cluster aggregation for hybrid architectures
   - `CpuClusterType`: Performance, Efficiency, Standard

2. **Linux Implementation** (`src/silicon/linux.rs`)
   - CPU frequency: `/sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq`
   - CPU utilization: `/proc/stat` parsing
   - Temperature: `hwmon`, `thermal_zone`, `coretemp` support
   - Hybrid CPU detection: Intel 12th gen+ via max frequency heuristic
   - Cluster grouping: P-cores vs E-cores

3. **Windows Implementation** (`src/silicon/windows.rs`)
   - Per-core utilization via performance counters — verified returning distinct
     per-core figures, not a replicated aggregate
   - CPU identity, core count and clock
   - No temperature: board sensors need a signed kernel driver IronMonitor does not ship

4. **macOS Implementation** (`src/silicon/apple.rs`)
   - Full powermetrics integration
   - E-core and P-core cluster tracking
   - Per-core frequency and utilization
   - Power consumption per cluster

### 🚧 Partial/TODO

1. **Linux Enhancements**
   - RAPL power consumption tracking
   - Better hybrid CPU topology detection
   - AMD-specific optimizations

2. **Windows Enhancements**
   - WMI queries for temperature: `MSAcpi_ThermalZoneTemperature`
   - Performance Counter integration: `\Processor(*)\% Processor Time`
   - Frequency monitoring: `\Processor Information(*)\Processor Frequency`
   - Hybrid CPU detection: `GetSystemCpuSetInformation` API

## Architecture

### Linux Temperature Monitoring

The Linux implementation uses a multi-tiered approach for temperature monitoring:

```rust
1. hwmon sensors (primary)
   - Modern systems: /sys/class/hwmon/hwmon*/
   - Supports: coretemp (Intel), k10temp (AMD), zenpower (AMD)
   - Per-core temperature labels

2. thermal_zone (fallback)
   - Legacy systems: /sys/class/thermal/thermal_zone*/
   - Zone type matching for core identification

3. Package temperature (last resort)
   - Single temperature for entire CPU package
   - Used when per-core temps unavailable
```

### Hybrid CPU Detection

```rust
// Linux: Heuristic based on max frequency
if max_freq < 4.0 GHz → Efficiency core
if max_freq >= 4.0 GHz → Performance core

// macOS: Explicit from powermetrics
E-cluster: cpuN with name starting with 'E'
P-cluster: cpuN with name starting with 'P'

// Windows: TODO - Use GetSystemCpuSetInformation
```

### Cluster Aggregation

Cores are grouped into clusters based on their type:

```rust
CpuCluster {
    cluster_type: Performance | Efficiency | Standard,
    core_ids: [0, 1, 2, 3],      // Cores in this cluster
    frequency_mhz: 3500,          // Average frequency
    utilization: 45,              // Average utilization (0-100)
    power_watts: Some(15.5),      // Total power (if available)
}
```

## Usage Example

```rust
use ironmon::silicon::{IronMonitor, CpuClusterType};

// Linux
#[cfg(target_os = "linux")]
use ironmon::silicon::linux::LinuxSiliconMonitor;

let monitor = LinuxSiliconMonitor::new()?;
let (cores, clusters) = monitor.cpu_info()?;

// Display cluster information
for cluster in clusters {
    match cluster.cluster_type {
        CpuClusterType::Performance => {
            println!("P-cores: {} MHz, {}% util", 
                cluster.frequency_mhz, 
                cluster.utilization);
        }
        CpuClusterType::Efficiency => {
            println!("E-cores: {} MHz, {}% util", 
                cluster.frequency_mhz, 
                cluster.utilization);
        }
        _ => {}
    }
}

// Display per-core details
for core in cores {
    println!("Core {}: {}MHz, {}% util, {}°C",
        core.id,
        core.frequency_mhz,
        core.utilization,
        core.temperature.unwrap_or(0)
    );
}
```

## Temperature Monitoring Details

### Linux

#### Intel CPUs
```bash
# hwmon sensors
/sys/class/hwmon/hwmon*/name        # "coretemp"
/sys/class/hwmon/hwmon*/temp*_label # "Core 0", "Core 1", ...
/sys/class/hwmon/hwmon*/temp*_input # Temperature in millidegrees C
```

#### AMD CPUs
```bash
# k10temp (Zen architecture)
/sys/class/hwmon/hwmon*/name        # "k10temp"
/sys/class/hwmon/hwmon*/temp1_input # Tdie temperature

# zenpower (alternative driver)
/sys/class/hwmon/hwmon*/name        # "zenpower"
/sys/class/hwmon/hwmon*/temp*_label # Per-CCD temperatures
```

### Windows

Temperature monitoring on Windows requires administrator privileges:

```rust
// WMI Query (requires admin)
SELECT * FROM MSAcpi_ThermalZoneTemperature
NAMESPACE: root\wmi

// Convert from tenths of Kelvin to Celsius
temp_celsius = (temp_kelvin / 10.0) - 273.15
```

### macOS

macOS doesn't expose per-core temperatures via powermetrics. Only thermal pressure is available:

```rust
// From powermetrics plist
thermal_pressure: "Normal" | "Light" | "Moderate" | "Heavy"
```

## Hybrid CPU Examples

### Intel 12th Gen (Alder Lake)
```
P-cores: 4-8 cores at 4.5-5.0 GHz
E-cores: 4-8 cores at 3.5-3.8 GHz

Example: i7-12700K
- 8 P-cores (16 threads)
- 4 E-cores (4 threads)
- Total: 20 threads
```

### Apple M-series
```
M1: 4 E-cores + 4 P-cores
M1 Pro: 2 E-cores + 8 P-cores
M1 Max: 2 E-cores + 8 P-cores
M1 Ultra: 4 E-cores + 16 P-cores

M2: 4 E-cores + 4 P-cores
M2 Pro: 4 E-cores + 8 P-cores
M2 Max: 4 E-cores + 8 P-cores

M3: 4 E-cores + 4 P-cores
M3 Pro: 6 E-cores + 6 P-cores
M3 Max: 4 E-cores + 12 P-cores

M4: 4 E-cores + 6 P-cores
M4 Pro: 4 E-cores + 10 P-cores
M4 Max: 4 E-cores + 12 P-cores
```

## Future Enhancements

1. **RAPL Power Monitoring (Linux)**
   ```rust
   // Intel RAPL
   /sys/class/powercap/intel-rapl/intel-rapl:0/energy_uj
   
   // AMD Ryzen
   /sys/class/hwmon/hwmon*/power*_average
   ```

2. **Windows Performance Counters**
   ```rust
   use windows::Win32::System::Performance::*;
   
   // Query: \Processor(*)\% Processor Time
   // Query: \Processor Information(*)\Processor Frequency
   ```

3. **Advanced Topology Detection**
   - NUMA nodes
   - Cache hierarchy (L1, L2, L3)
   - Physical vs logical cores
   - SMT/Hyperthreading detection

4. **Real-time Monitoring**
   - Continuous sampling
   - History tracking
   - Anomaly detection
   - Performance profiling

## Testing

Run the CPU monitoring example:

```bash
# Linux
cargo run --example cpu_monitor --features cpu

# Windows
cargo run --example cpu_monitor --features cpu

# macOS with Apple Silicon
cargo run --example cpu_monitor --features apple,cpu
```

## References

- Linux kernel documentation: https://www.kernel.org/doc/Documentation/hwmon/
- Intel Performance Counter Monitor: https://github.com/intel/pcm
- AMD uProf: https://developer.amd.com/amd-uprof/
- Windows Performance Counters: https://learn.microsoft.com/en-us/windows/win32/perfctrs/
- macOS powermetrics: `man powermetrics`

---

**Status**: Linux ✅ | Windows ✅ per-core utilization, no temperature | macOS
unverified — never exercised, since the crate could not build there until 2.1.2
