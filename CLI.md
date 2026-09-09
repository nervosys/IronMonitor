# ironmon CLI - Complete Command Reference

A comprehensive command-line tool for NVIDIA GPU monitoring and Jetson device management.

## Installation

```bash
cargo build --release --features full
sudo cp target/release/ironmon /usr/local/bin/
```

## Command Structure

```
ironmon [OPTIONS] [COMMAND]
```

## Agent-facing commands

These describe and read the machine through a stable, machine-readable schema. Each
value carries a provenance saying whether it was measured, taken from a
specification, derived, or is unavailable here — see [AGENTS.md](AGENTS.md) for the
full contract.

```bash
ironmon describe                    # every reportable value: id, unit, provenance
ironmon describe --commands         # the command surface, generated from the parser
ironmon describe --writable         # only what can be written back
ironmon describe --search thermal   # find ids without knowing the namespace
ironmon get memory.total            # read one value by id
ironmon snapshot --validate         # read everything, range-checked
```

`ironmon get` exits 1 for an unknown id and 2 for a known id with no value here, so a
caller can tell "no such thing" from "nothing to report".

The interactive surfaces render headlessly for callers with no terminal or display:

```bash
ironmon tui --frame --tab CPU       # one TUI frame as text
ironmon tui --script script.txt     # drive the TUI and assert on what it shows
ironmon gui --script script.txt     # navigate GUI tabs and assert on painted text
ironmon gui --frame --tab profiles  # the text a GUI tab paints
```

## Global Options

- `-i, --interval <SECONDS>` - Update interval in seconds (default: 1.0)
- `-f, --format <FORMAT>` - Output format: `text` or `json` (default: text)
- `--version` - Show version information
- `--help` - Show help information

## Monitoring Commands

### Interactive Mode (Default)

```bash
ironmon
```

Launches an interactive terminal UI showing real-time stats. Press 'q' to quit.

### Board Information

```bash
ironmon cli board
ironmon cli board --format json
```

Shows hardware information:
- Model name
- Module ID
- JetPack version (Jetson)
- L4T version (Jetson)
- Hardware revision
- Serial number

### GPU Monitoring

```bash
ironmon cli gpu
ironmon cli gpu --format json
```

Displays GPU statistics:
- GPU type (Integrated/Discrete)
- Frequency (current/min/max)
- Utilization percentage
- Memory usage
- Temperature
- Power consumption

### CPU Monitoring

```bash
ironmon cli cpu
ironmon cli cpu --format json
```

Shows CPU information:
- Total CPU usage
- Per-core usage
- Current frequencies
- CPU governor
- Online/offline cores

### Memory Monitoring

```bash
ironmon cli memory
ironmon cli memory --format json
```

Memory statistics:
- RAM (total/used/free/cached)
- SWAP (total/used/cached)
- EMC frequency (Jetson)
- IRAM (Jetson)

### Power Monitoring

```bash
ironmon cli power
ironmon cli power --format json
```

Power consumption:
- Total power (watts)
- Per-rail power (INA3221 sensors on Jetson)
- Voltage and current per rail
- Average power

### Temperature Monitoring

```bash
ironmon cli temperature
ironmon cli temperature --format json
```

Temperature readings:
- All thermal zones
- Maximum temperature
- Per-zone temperatures

### Process Monitoring

```bash
ironmon cli processes
ironmon cli processes --format json
```

GPU process information:
- PID
- User
- GPU assignment
- Process type
- CPU usage
- Memory usage
- GPU memory usage
- Process name

### Engine Monitoring

```bash
ironmon cli engines
ironmon cli engines --format json
```

Hardware accelerator status:
- APE (Audio Processing Engine)
- DLA (Deep Learning Accelerator)
- PVA (Programmable Vision Accelerator)
- VIC (Video Image Compositor)
- NVJPG (JPEG Encoder/Decoder)
- NVENC (Video Encoder)
- NVDEC (Video Decoder)
- SE (Security Engine)
- CVNAS
- MSENC
- OFA

### All Statistics

```bash
ironmon cli all
ironmon cli all --format json
```

## Advanced Utilities

### Jetson Clocks

Performance maximization tool.

#### Enable (Maximize Performance)

```bash
sudo ironmon cli jetson clocks enable
```

Sets all frequencies to maximum:
- CPU: max frequency, all cores online
- GPU: max frequency
- EMC: max frequency
- All engines: max frequency

#### Disable (Restore Settings)

```bash
sudo ironmon cli jetson clocks disable
```

Restores saved configuration or default settings.

#### Status

```bash
ironmon cli jetson clocks status
```

Shows:
- Whether jetson_clocks is active
- Configured engines
- Current frequency settings

#### Store Configuration

```bash
sudo ironmon cli jetson clocks store
```

Saves current configuration for later restoration.

### NVPModel

Power mode management.

#### Show Current Mode

```bash
ironmon cli jetson powermode show
```

Displays:
- Current power mode ID
- Current power mode name

#### List All Modes

```bash
ironmon cli jetson powermode list
```

Shows:
- All available power modes
- Mode IDs and names
- Default mode
- Current mode

#### Set Mode by ID

```bash
sudo ironmon cli jetson powermode set <MODE_ID>
sudo ironmon cli jetson powermode set <MODE_ID> --force
```

Changes power mode by ID (0, 1, 2, etc.).

Options:
- `--force, -f` - Skip confirmation prompt

#### Set Mode by Name

```bash
sudo ironmon cli jetson powermode set-name <MODE_NAME>
sudo ironmon cli jetson powermode set-name <MODE_NAME> --force
```

Changes power mode by name (MAXN, MODE_15W, MODE_10W, etc.).

Options:
- `--force, -f` - Skip confirmation prompt

### Swap Management

Swap file creation and management.

#### Status

```bash
ironmon cli jetson swap status
```

Shows active swap files:
- Path
- Type (file/partition)
- Size
- Used space
- Priority

#### Create Swap

```bash
sudo ironmon cli jetson swap create
sudo ironmon cli jetson swap create --path <PATH> --size <GB> --auto
```

Creates a new swap file.

Options:
- `--path, -p <PATH>` - Swap file path (default: /swapfile)
- `--size, -s <GB>` - Size in GB (default: 8)
- `--auto, -a` - Enable on boot (add to /etc/fstab)

Examples:
```bash
# Create 8GB swap at /swapfile
sudo ironmon cli jetson swap create

# Create 16GB swap with custom path
sudo ironmon cli jetson swap create --path /mnt/swap16g --size 16

# Create and enable on boot
sudo ironmon cli jetson swap create --size 12 --auto
```

#### Enable Swap

```bash
sudo ironmon cli jetson swap enable <PATH>
```

Activates an existing swap file.

#### Disable Swap

```bash
sudo ironmon cli jetson swap disable <PATH>
```

Temporarily deactivates swap file.

#### Remove Swap

```bash
sudo ironmon cli jetson swap remove <PATH>
```

Disables and deletes swap file.

## Usage Examples

### Basic Monitoring

```bash
# Interactive monitoring
ironmon

# One-time snapshot
ironmon cli all

# JSON output for integration
ironmon cli all --format json | jq '.gpus'
```

### Performance Profiling

```bash
# Check current status
ironmon cli gpu
ironmon cli cpu
ironmon cli memory

# Enable maximum performance
sudo ironmon cli jetson powermode set-name MAXN --force
sudo ironmon cli jetson clocks enable

# Verify
ironmon cli jetson clocks status
ironmon cli jetson powermode show
```

### Power Management

```bash
# List available modes
ironmon cli jetson powermode list

# Switch to 15W mode
sudo ironmon cli jetson powermode set 1

# Disable jetson_clocks
sudo ironmon cli jetson clocks disable
```

### Memory Management

```bash
# Check swap status
ironmon cli jetson swap status

# Create swap if needed
sudo ironmon cli jetson swap create --size 8 --auto

# Check memory after
ironmon cli memory
```

### Process Tracking

```bash
# Monitor GPU processes
ironmon cli processes

# Watch process changes
watch -n 1 'ironmon processes'
```

### System Setup

```bash
# First-time setup
sudo ironmon cli jetson swap create --size 8 --auto
sudo ironmon cli jetson powermode set-name MAXN
ironmon cli board

# Start monitoring
ironmon
```

## Output Formats

### Text Format (Default)

Human-readable output with labels and formatting.

```bash
ironmon cli gpu
```

```
=== GPU Information ===
GPU 0 (Integrated):
  Frequency: 1300 MHz (204-1300 MHz)
  Load: 45%
  Memory: 1234 MB / 4096 MB
```

### JSON Format

Machine-readable JSON for scripting and integration.

```bash
ironmon cli gpu --format json
```

```json
{
  "gpu0": {
    "type": "Integrated",
    "freq": {
      "current": 1300,
      "min": 204,
      "max": 1300
    },
    "load": 45.0,
    "memory": {
      "used": 1234,
      "total": 4096
    }
  }
}
```

## Permissions

- **Read Operations**: No special permissions required
  - `ironmon board`, `ironmon gpu`, `ironmon cpu`, etc.
  
- **Write Operations**: Require `sudo`
  - `sudo ironmon cli jetson clocks enable`
  - `sudo ironmon cli jetson powermode set <ID>`
  - `sudo ironmon cli jetson swap create`

## Platform Availability

| Command       | Jetson | Linux Desktop | Windows |
| ------------- | ------ | ------------- | ------- |
| board         | ✅      | ✅             | 🚧       |
| gpu           | ✅      | ✅             | 🚧       |
| cpu           | ✅      | ✅             | 🚧       |
| memory        | ✅      | ✅             | 🚧       |
| power         | ✅      | ✅             | 🚧       |
| temperature   | ✅      | ✅             | 🚧       |
| processes     | ✅      | ❌             | ❌       |
| engines       | ✅      | ❌             | ❌       |
| jetson-clocks | ✅      | ❌             | ❌       |
| nvpmodel      | ✅      | ❌             | ❌       |
| swap          | ✅      | ✅             | ❌       |

## Exit Codes

- `0` - Success
- `1` - Error occurred
- `2` - Invalid arguments

## Environment Variables

- `RUST_LOG` - Set logging level (error, warn, info, debug, trace)

Example:
```bash
RUST_LOG=debug ironmon all
```

## See Also

- [README.md](README.md) - Main documentation
- [docs/UTILITIES.md](docs/UTILITIES.md) - Detailed utility documentation
- [AGENTS.md](AGENTS.md) - Driving ironmon from an AI agent
- [BUILD.md](BUILD.md) - Build instructions

