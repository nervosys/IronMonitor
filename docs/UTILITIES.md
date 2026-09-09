# Advanced Utilities

The ironmon CLI provides advanced utilities for managing NVIDIA Jetson devices, including performance optimization, power mode management, and swap configuration.

## Table of Contents

- [Jetson Clocks](#jetson-clocks)
- [NVPModel](#nvpmodel)
- [Swap Management](#swap-management)

## Jetson Clocks

Jetson Clocks is a performance maximization utility that sets all frequencies (CPU, GPU, EMC, engines) to their maximum values for optimal performance.

### Commands

#### Enable Performance Mode

```bash
# Maximize all frequencies
sudo ironmon cli jetson clocks enable
```

This will:
- Set all CPU cores to maximum frequency
- Set GPU to maximum frequency
- Set EMC (memory controller) to maximum frequency
- Set all engines (NVENC, NVDEC, etc.) to maximum frequency
- Lock frequencies to prevent throttling

#### Disable Performance Mode

```bash
# Restore original settings
sudo ironmon cli jetson clocks disable
```

#### Check Status

```bash
# Show jetson_clocks status
ironmon cli jetson clocks status
```

Output:
```
=== Jetson Clocks Status ===
Active: YES

Configured Engines:
  - CPU
  - GPU
  - EMC
  - NVENC
  - NVDEC
```

#### Store Configuration

```bash
# Save current configuration
sudo ironmon cli jetson clocks store
```

### Use Cases

**Maximum Performance**: Enable jetson_clocks when you need maximum performance for compute-intensive tasks like deep learning inference, video encoding, or real-time processing.

**Power Saving**: Disable jetson_clocks when running on battery or when performance is not critical.

## NVPModel

NVPModel controls power modes on Jetson devices. Each mode represents a different power budget with various CPU/GPU configurations.

### Commands

#### Show Current Mode

```bash
# Display current power mode
ironmon cli jetson powermode show
```

Output:
```
=== Current Power Mode ===
ID: 0
Name: MAXN
```

#### List All Modes

```bash
# List all available power modes
ironmon cli jetson powermode list
```

Output:
```
=== Available Power Modes ===

Current Mode:
  ID: 0 - MAXN (default)

All Modes:
  ID: 0 - MAXN (default)
  ID: 1 - MODE_15W
  ID: 2 - MODE_10W

Default Mode:
  ID: 0 - MAXN
```

#### Set Mode by ID

```bash
# Set power mode by ID
sudo ironmon cli jetson powermode set 1

# Force mode change (skip confirmation)
sudo ironmon cli jetson powermode set 1 --force
```

#### Set Mode by Name

```bash
# Set power mode by name
sudo ironmon cli jetson powermode set-name MODE_15W

# Force mode change
sudo ironmon cli jetson powermode set-name MODE_15W --force
```

### Power Modes

Common power modes on Jetson devices:

| Mode | Power Budget | Description                           |
| ---- | ------------ | ------------------------------------- |
| MAXN | 30W+         | Maximum performance, all cores online |
| 15W  | 15W          | Balanced performance and power        |
| 10W  | 10W          | Power-efficient mode                  |

### Use Cases

**MAXN**: Best for AC-powered applications requiring maximum performance
**15W**: Good balance for battery-powered applications
**10W**: Maximum battery life for low-power applications

## Swap Management

Create and manage swap files to extend available memory on Jetson devices.

### Commands

#### Check Swap Status

```bash
# Show current swap status
ironmon cli jetson swap status
```

Output:
```
=== Active Swap ===
NAME                           TYPE       SIZE       USED       PRIO      
/swapfile                      file       8.0G       512M       -2        
```

#### Create Swap File

```bash
# Create an 8GB swap file (default)
sudo ironmon cli jetson swap create

# Create with custom path and size
sudo ironmon cli jetson swap create --path /mnt/swapfile --size 16

# Create and enable on boot
sudo ironmon cli jetson swap create --auto
```

Parameters:
- `--path`: Swap file location (default: `/swapfile`)
- `--size`: Size in GB (default: `8`)
- `--auto`: Add to `/etc/fstab` for automatic enable on boot

#### Enable Swap

```bash
# Enable an existing swap file
sudo ironmon cli jetson swap enable /swapfile
```

#### Disable Swap

```bash
# Temporarily disable swap
sudo ironmon cli jetson swap disable /swapfile
```

#### Remove Swap

```bash
# Disable and remove swap file
sudo ironmon cli jetson swap remove /swapfile
```

This will:
1. Disable the swap file
2. Delete the swap file
3. Remove from `/etc/fstab` (if present)

### Swap Size Recommendations

| RAM Size | Recommended Swap               |
| -------- | ------------------------------ |
| 4GB      | 8GB                            |
| 8GB      | 8-16GB                         |
| 16GB+    | 8-32GB (depending on workload) |

### Use Cases

**Deep Learning**: Create large swap for models that exceed available RAM
**Long-Running Services**: Prevent OOM kills for memory-intensive applications
**Compilation**: Extra memory for building large projects

## Combined Workflow Examples

### Maximum Performance Setup

```bash
# Set to MAXN mode
sudo ironmon cli jetson powermode set-name MAXN --force

# Enable jetson_clocks
sudo ironmon cli jetson clocks enable

# Verify
ironmon cli jetson powermode show
ironmon cli jetson clocks status
```

### Power-Efficient Setup

```bash
# Set to 10W mode
sudo ironmon cli jetson powermode set-name MODE_10W --force

# Disable jetson_clocks
sudo ironmon cli jetson clocks disable

# Verify
ironmon cli jetson powermode show
ironmon cli jetson clocks status
```

### First-Time Setup

```bash
# Create swap
sudo ironmon cli jetson swap create --size 8 --auto

# Check system status
ironmon cli all

# Monitor continuously
ironmon
```

## Notes

- All utilities require `sudo` for making changes
- Query operations (show, status, list) don't require `sudo`
- Changes are immediate but may not persist across reboots (except swap with `--auto`)
- jetson_clocks and nvpmodel are Jetson-specific and won't work on desktop systems
- Swap management works on any Linux system

## Troubleshooting

### jetson_clocks not found

```
Error: jetson_clocks is not available on this system
```

**Solution**: These utilities are only available on Jetson devices. Install jetson_clocks from NVIDIA JetPack.

### nvpmodel not found

```
Error: nvpmodel is not available on this system
```

**Solution**: Install nvpmodel from NVIDIA JetPack or ensure it's in your PATH.

### Permission denied

```
Error: Permission denied
```

**Solution**: Use `sudo` for operations that modify system settings:
```bash
sudo ironmon cli jetson clocks enable
```

### Swap creation failed

```
Error: Swap file already exists
```

**Solution**: Remove the existing swap file first:
```bash
sudo ironmon cli jetson swap remove /swapfile
```

