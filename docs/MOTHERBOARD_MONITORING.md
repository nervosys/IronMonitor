# Motherboard and System Monitoring Implementation Guide

## Overview

IronMonitor (ironmon) provides comprehensive motherboard sensor monitoring and detailed system information gathering across all major platforms.

## Features

### ✅ Implemented (Linux)

1. **Motherboard Sensors** (via hwmon)
   - Temperature sensors (CPU, chipset, VRM, ambient, M.2 slots)
   - Voltage rails (VCore, +3.3V, +5V, +12V, VDIMM, etc.)
   - Fan monitoring (RPM, PWM duty cycle)
   - Fan control (manual PWM or automatic mode)

2. **System Information** (via DMI/SMBIOS)
   - Operating system details (name, version, kernel, architecture)
   - BIOS/UEFI information (vendor, version, release date, firmware type)
   - Hardware details (manufacturer, model, serial number, UUID)
   - Motherboard information (vendor, model, version)
   - CPU information (model, cores, threads)

3. **Driver Versions** (via /sys/module)
   - GPU drivers (NVIDIA, AMD, Intel)
   - Storage drivers (NVMe, AHCI, SATA, RAID controllers)
   - Network drivers (e1000e, igb, ixgbe, r8169, etc.)

### 🚧 Partially Implemented

- **Windows Support**: Skeleton implementation with WMI integration planned
- **macOS Support**: Skeleton implementation with IOKit/SMC integration planned

### ❌ Not Yet Implemented

1. **Windows WMI Integration**
   - Win32_BaseBoard, Win32_BIOS, Win32_OperatingSystem queries
   - MSAcpi_ThermalZoneTemperature for ACPI temperatures
   - Win32_PnPSignedDriver for driver information
   - LibreHardwareMonitor integration for detailed sensors

2. **macOS IOKit/SMC Integration**
   - IOKit AppleSMC for temperature, fan, voltage sensors
   - IOPlatformExpertDevice for system information
   - system_profiler parsing for hardware/software details
   - kextstat parsing for loaded kernel extensions

## Architecture

```
┌─────────────────────────────────────────────────┐
│    Motherboard Monitoring API (Rust)            │
│  - MotherboardDevice trait                      │
│  - SystemInfo structure                         │
│  - DriverInfo collection                        │
└──────────────┬──────────────────────────────────┘
               │
       ┌───────┴────────────────────┐
       │                            │
┌──────▼──────┐            ┌────────▼────────┐
│   Linux     │            │  Windows/macOS  │
├─────────────┤            ├─────────────────┤
│ - hwmon     │            │ - WMI           │
│ - DMI/      │            │ - IOKit SMC     │
│   SMBIOS    │            │ - system_       │
│ - /sys/     │            │   profiler      │
│   module    │            │ - kextstat      │
└─────────────┘            └─────────────────┘
```

## Data Structures

### TemperatureSensor
Temperature readings from motherboard sensors:
- `label`: Sensor description (e.g., "CPU", "Chipset", "VRM")
- `temperature`: Current temperature in Celsius
- `max`: Maximum safe temperature (optional)
- `critical`: Critical temperature threshold (optional)
- `sensor_type`: Classification (CPU, Chipset, VRM, Ambient, M2Slot, PCH, Other)

### VoltageRail
Voltage measurements from power rails:
- `label`: Rail description (e.g., "VCore", "+12V", "VDIMM")
- `voltage`: Current voltage in volts
- `min`: Minimum acceptable voltage (optional)
- `max`: Maximum acceptable voltage (optional)

### FanInfo
Fan speed and control information:
- `label`: Fan description (e.g., "CPU Fan", "Case Fan 1")
- `rpm`: Current speed in RPM (None if stopped)
- `pwm`: PWM duty cycle (0-255, if available)
- `min_rpm`: Minimum RPM threshold (optional)
- `max_rpm`: Maximum RPM capability (optional)
- `controllable`: Whether fan speed can be controlled

### FanControl
Fan control mode:
- `Manual(u8)`: Set specific PWM value (0-255)
- `Automatic`: Use automatic hardware/firmware control

### SystemInfo
Comprehensive system information:

**Operating System:**
- `os_name`: OS distribution/product name
- `os_version`: Version number
- `kernel_version`: Kernel version string (optional)
- `architecture`: CPU architecture (x86_64, aarch64, etc.)
- `hostname`: System hostname (optional)

**BIOS/UEFI:**
- `bios`: BiosInfo structure with vendor, version, date, type, secure boot

**Hardware:**
- `manufacturer`: System manufacturer (Dell, ASUS, etc.)
- `product_name`: Model name
- `serial_number`: System serial number
- `uuid`: System UUID

**Motherboard:**
- `board_vendor`: Board manufacturer
- `board_name`: Board model
- `board_version`: Board revision

**CPU:**
- `cpu_name`: Processor model name
- `cpu_cores`: Physical core count
- `cpu_threads`: Logical thread count

### BiosInfo
BIOS/UEFI firmware information:
- `vendor`: BIOS vendor (AMI, Award, Phoenix, etc.)
- `version`: BIOS version string
- `release_date`: Release date
- `revision`: BIOS revision (optional)
- `firmware_type`: Legacy BIOS or UEFI
- `secure_boot`: Secure Boot status (UEFI only, optional)

### DriverInfo
Driver/module information:
- `name`: Driver name
- `version`: Version string
- `driver_type`: Classification (GPU, Chipset, Storage, Network, Audio, USB, Other)
- `description`: Human-readable description (optional)
- `vendor`: Vendor name (optional)
- `date`: Driver/release date (optional)

## Usage Examples

### Basic Sensor Enumeration

```rust
use ironmon::motherboard;

// Enumerate all sensor chips
let sensors = motherboard::enumerate_sensors()?;

for sensor in sensors {
    println!("Chip: {}", sensor.name());
    
    // Get temperatures
    for temp in sensor.temperature_sensors()? {
        println!("  {}: {:.1}°C", temp.label, temp.temperature);
    }
    
    // Get voltages
    for volt in sensor.voltage_rails()? {
        println!("  {}: {:.3}V", volt.label, volt.voltage);
    }
    
    // Get fans
    for fan in sensor.fans()? {
        if let Some(rpm) = fan.rpm {
            println!("  {}: {} RPM", fan.label, rpm);
        }
    }
}
```

### System Information

```rust
use ironmon::motherboard;

let info = motherboard::get_system_info()?;

println!("OS: {} {}", info.os_name, info.os_version);
println!("Kernel: {}", info.kernel_version.unwrap_or_default());
println!("Architecture: {}", info.architecture);

println!("\nBIOS: {} {}", 
    info.bios.vendor.unwrap_or_default(),
    info.bios.version.unwrap_or_default()
);
println!("Firmware Type: {:?}", info.bios.firmware_type);

println!("\nHardware: {} {}", 
    info.manufacturer.unwrap_or_default(),
    info.product_name.unwrap_or_default()
);

println!("\nCPU: {}", info.cpu_name.unwrap_or_default());
println!("Cores: {} | Threads: {}", 
    info.cpu_cores.unwrap_or(0),
    info.cpu_threads.unwrap_or(0)
);
```

### Driver Versions

```rust
use ironmon::motherboard;

let drivers = motherboard::get_driver_versions()?;

for driver in drivers {
    println!("{} ({}): {}", 
        driver.name,
        driver.driver_type,
        driver.version
    );
}
```

### Fan Control (Linux)

```rust
use ironmon::motherboard::{self, FanControl};

let sensors = motherboard::enumerate_sensors()?;
let sensor = &sensors[0];

// Set fan to 50% speed
sensor.set_fan_speed(0, FanControl::Manual(128))?;

// Wait a bit
std::thread::sleep(std::time::Duration::from_secs(5));

// Return to automatic control
sensor.set_fan_speed(0, FanControl::Automatic)?;
```

### Temperature Monitoring

```rust
use ironmon::motherboard::{self, SensorType};

let sensors = motherboard::enumerate_sensors()?;

for sensor in sensors {
    let temps = sensor.temperature_sensors()?;
    
    // Filter for CPU temperatures
    for temp in temps.iter().filter(|t| t.sensor_type == SensorType::Cpu) {
        println!("{}: {:.1}°C", temp.label, temp.temperature);
        
        if let Some(crit) = temp.critical {
            if temp.temperature > crit - 10.0 {
                println!("⚠️  WARNING: Temperature approaching critical!");
            }
        }
    }
}
```

## Platform-Specific Implementation Details

### Linux

**Data Sources:**

1. **hwmon** (`/sys/class/hwmon/*`)
   - `hwmon*/name` - Sensor chip name
   - `temp*_input` - Temperature in millidegrees Celsius
   - `temp*_label` - Temperature sensor label
   - `temp*_max` - Maximum temperature threshold
   - `temp*_crit` - Critical temperature threshold
   - `in*_input` - Voltage in millivolts
   - `in*_label` - Voltage rail label
   - `fan*_input` - Fan speed in RPM
   - `fan*_label` - Fan label
   - `pwm*` - PWM duty cycle (0-255)
   - `pwm*_enable` - PWM mode (1=manual, 2=automatic)

2. **DMI/SMBIOS** (`/sys/class/dmi/id/*`)
   - `bios_vendor` - BIOS vendor
   - `bios_version` - BIOS version
   - `bios_date` - BIOS release date
   - `sys_vendor` - System manufacturer
   - `product_name` - Product model
   - `product_serial` - Serial number
   - `product_uuid` - System UUID
   - `board_vendor` - Motherboard vendor
   - `board_name` - Motherboard model
   - `board_version` - Motherboard revision

3. **Module Versions** (`/sys/module/*/version`)
   - GPU drivers: nvidia, amdgpu, i915
   - Storage: nvme, ahci, sata_nv, megaraid_sas
   - Network: e1000e, igb, ixgbe, r8169, bnx2x

4. **OS Information**
   - `/etc/os-release` or `/usr/lib/os-release` - OS details
   - `/proc/version` - Kernel version
   - `/proc/cpuinfo` - CPU information
   - `/sys/firmware/efi` - UEFI detection

**Sensor Chip Examples:**
- `coretemp` - Intel CPU temperatures
- `k10temp` - AMD CPU temperatures
- `nct6798` - Nuvoton Super I/O chip
- `it8792e` - ITE Super I/O chip
- `asus-ec-sensors` - ASUS motherboard sensors

**Requirements:**
- `lm-sensors` package for sensor detection
- Proper kernel modules loaded (detected via `sensors-detect`)
- Root/CAP_SYS_ADMIN for fan control

### Windows (Planned)

**Data Sources:**

1. **WMI Classes:**
   - `Win32_BaseBoard` - Motherboard information
   - `Win32_BIOS` - BIOS/UEFI information
   - `Win32_OperatingSystem` - OS details
   - `Win32_ComputerSystem` - System information
   - `Win32_Processor` - CPU information
   - `Win32_PnPSignedDriver` - Driver versions
   - `MSAcpi_ThermalZoneTemperature` - ACPI temperatures

2. **LibreHardwareMonitor Integration:**
   - COM interop or embedded library
   - Detailed sensor readings (temps, voltages, fans)
   - Support for various sensor chips

**Challenges:**
- WMI sensors limited compared to Linux hwmon
- May require third-party tools (LibreHardwareMonitor, HWiNFO)
- Administrator privileges for detailed information
- Different APIs across Windows versions

### macOS (Planned)

**Data Sources:**

1. **IOKit:**
   - `AppleSMC` - System Management Controller
     * Temperature sensors (TC0P, TG0P, Th0H, etc.)
     * Fan speeds (F0Ac, F0Mn, F0Mx)
     * Voltages and power
   - `IOPlatformExpertDevice` - System information
     * Manufacturer, model, serial number

2. **Command-line Tools:**
   - `system_profiler SPHardwareDataType` - Hardware details
   - `system_profiler SPSoftwareDataType` - OS version
   - `kextstat` - Loaded kernel extensions (drivers)

**SMC Keys:**
- Temperature: `TC0P` (CPU proximity), `TG0P` (GPU proximity), `Th0H` (HDD)
- Fan: `F0Ac` (actual speed), `F0Mn` (minimum), `F0Mx` (maximum)
- Voltage: Various SMC voltage keys

**Challenges:**
- Requires IOKit framework integration
- SMC key database varies by Mac model
- May require entitlements for IOKit access
- Different across Intel vs Apple Silicon Macs

## Common Sensor Chips

### Super I/O Chips
These chips integrate multiple monitoring functions:

- **Nuvoton**: NCT6798D, NCT6799D, NCT6796D
- **ITE**: IT8792E, IT8628E, IT8686E
- **Winbond**: W83627DHG, W83795G
- **ASUS EC**: ASUS motherboard embedded controllers

### CPU-Integrated Sensors
- **Intel**: coretemp (package/core temperatures)
- **AMD**: k10temp (Tdie, Tctl temperatures)

### Chipset Sensors
- **Intel PCH**: Various chipset temperature sensors
- **AMD**: Chipset temperature monitoring

## Sensor Type Classification

The library automatically classifies sensors based on their labels:

| Sensor Type | Label Keywords     | Description                    |
| ----------- | ------------------ | ------------------------------ |
| CPU         | cpu, core, package | Processor temperatures         |
| Chipset     | chipset, pch       | Chipset temperatures           |
| VRM         | vrm, vcore         | Voltage regulator temperatures |
| Ambient     | ambient, system    | System/case temperatures       |
| M2Slot      | m.2, nvme          | M.2 slot temperatures          |
| PCH         | pch                | Platform Controller Hub        |
| Other       | -                  | Unclassified sensors           |

## Fan Control

### PWM Modes

Most motherboards support these PWM modes:
- `0` - Full speed (no PWM)
- `1` - Manual control (software sets PWM value)
- `2` - Automatic/BIOS control
- `3` - Fan speed cruise control (some chips)
- `4` - Smart Fan IV (some ASUS boards)
- `5` - Enhanced automatic (some chips)

### Safety Considerations

**IMPORTANT**: Fan control can be dangerous!

- Never set fan speed too low on critical fans (especially CPU fan)
- Monitor temperatures closely when testing manual control
- Always have a fallback to automatic mode
- Some BIOSes may override software fan control
- Incorrect fan control can cause overheating and hardware damage

### Best Practices

1. **Query current state first** before making changes
2. **Set reasonable minimum speeds** (typically 30-40% for case fans, 50%+ for CPU fans)
3. **Implement temperature-based control** with safety margins
4. **Return to automatic mode** if software crashes
5. **Test changes gradually** and monitor temperatures

## Performance Considerations

- **Sensor Reads**: hwmon reads are fast (~1ms per sensor)
- **DMI Reads**: File-based, cache system info on startup
- **Module Queries**: Relatively fast, cache driver versions
- **Polling Intervals**: 
  - Temperature: 1-2 seconds is reasonable
  - Voltage: 1-2 seconds
  - Fan Speed: 1-2 seconds
  - System Info: Read once or on-demand
  - Driver Versions: Read once or on-demand

## Security Considerations

### Permissions

**Linux:**
- **Read Sensors**: Usually world-readable (no special permissions)
- **Fan Control**: Requires root or membership in appropriate group
- **DMI Information**: Some fields may require root
- **Module Versions**: Usually world-readable

**Windows:**
- **WMI Queries**: May require administrator privileges
- **Sensor Access**: Third-party tools may need admin rights

**macOS:**
- **IOKit**: May require entitlements
- **SMC Access**: May require root or specific entitlements

### Safety

- All sensor reads are **read-only** and safe
- **Fan control** can potentially cause hardware damage if misused
- No firmware modifications are performed
- No permanent settings are changed (fan control is temporary)

## Testing

Run the example:
```bash
# Linux
cargo run --example motherboard_monitor

# With detailed sensor output (requires lm-sensors)
sudo sensors-detect  # First time setup
cargo run --example motherboard_monitor

# With fan control (requires root)
sudo cargo run --example motherboard_monitor
```

Expected output shows:
- Operating system details
- BIOS/UEFI information
- Hardware manufacturer and model
- Motherboard details
- CPU information
- Driver versions
- Sensor chip readings (temperatures, voltages, fans)

## Troubleshooting

### Linux: No sensors found

1. Install lm-sensors:
   ```bash
   sudo apt install lm-sensors  # Debian/Ubuntu
   sudo dnf install lm_sensors  # Fedora
   sudo pacman -S lm_sensors    # Arch
   ```

2. Run sensor detection:
   ```bash
   sudo sensors-detect
   # Answer YES to probe for different chips
   # Answer YES to add modules to /etc/modules
   ```

3. Load kernel modules:
   ```bash
   sudo modprobe <module_name>
   # Or reboot to auto-load
   ```

4. Verify sensors work:
   ```bash
   sensors
   ```

### Linux: Fan control not working

1. Check if PWM files exist:
   ```bash
   ls /sys/class/hwmon/hwmon*/pwm*
   ```

2. Check permissions:
   ```bash
   ls -l /sys/class/hwmon/hwmon*/pwm*
   ```

3. May need root:
   ```bash
   sudo cargo run --example motherboard_monitor
   ```

4. Some BIOSes prevent software fan control (check BIOS settings)

### Windows: No sensor data

- Windows implementation is currently a skeleton
- Full implementation will require WMI integration
- Consider using LibreHardwareMonitor for now

### macOS: No sensor data

- macOS implementation is currently a skeleton
- Full implementation will require IOKit/SMC integration
- Consider using iStat Menus or similar tools for now

## Future Enhancements

1. **LibreHardwareMonitor Integration (Windows)**
   - Embedded library or COM interop
   - Access to detailed sensor data
   - Fan control support

2. **IOKit SMC Integration (macOS)**
   - Direct SMC key reading
   - Fan control via SMC
   - Temperature sensor enumeration

3. **Enhanced Linux Features**
   - Auto-discovery of optimal sensor chips
   - Sensor configuration files
   - Custom fan curves
   - Alarm/threshold monitoring

4. **Cross-Platform Features**
   - Unified sensor naming
   - Historical sensor data
   - Alert/notification system
   - Web dashboard for remote monitoring

5. **Advanced Monitoring**
   - Power consumption tracking
   - Efficiency calculations
   - Thermal trend analysis
   - Predictive maintenance warnings

## Related Tools

- [lm-sensors](https://github.com/lm-sensors/lm-sensors) - Linux hardware monitoring
- [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) - Windows/Linux monitoring
- [OpenHardwareMonitor](https://openhardwaremonitor.org/) - Windows monitoring
- [iStat Menus](https://bjango.com/mac/istatmenus/) - macOS monitoring
- [HWiNFO](https://www.hwinfo.com/) - Windows detailed monitoring
