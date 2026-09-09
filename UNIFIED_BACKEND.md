# Unified Backend Architecture

## Overview

IronMonitor now provides a **unified backend** (`src/backend.rs`) that ensures consistent data access and AI agent availability across all three frontend modes:

1. **CLI Mode** (`ironmon-cli`) - Command-line interface for quick queries
2. **TUI Mode** (`ironmon-cli tui`) - Terminal user interface with real-time monitoring
3. **GUI Mode** (`ironmon`) - Graphical user interface with egui

## Key Components

### MonitoringBackend

The `MonitoringBackend` struct is the central data access point:

```rust
use ironmon::{MonitoringBackend, BackendConfig};

// Create with default settings (agent enabled)
let backend = MonitoringBackend::new()?;

// Or customize configuration
let config = BackendConfig::default()
    .with_history_size(120)  // 2 minutes of history
    .with_update_interval(Duration::from_millis(500));
let backend = MonitoringBackend::with_config(config)?;
```

### What It Provides

| Feature              | Access Method                                                                         |
| -------------------- | ------------------------------------------------------------------------------------- |
| CPU Stats            | `backend.cpu_stats()`, `backend.cpu_utilization()`, `backend.cpu_history()`           |
| Memory Stats         | `backend.memory_stats()`, `backend.memory_utilization()`, `backend.memory_history()`  |
| GPU/Accelerator Info | `backend.gpu_collection()`, `backend.gpu_static_info()`, `backend.gpu_dynamic_info()` |
| Process List         | `backend.processes()`, `backend.processes_by_cpu()`, `backend.processes_by_gpu(idx)`  |
| Network Monitor      | `backend.network_monitor()`                                                           |
| Connections          | `backend.connections()`, `backend.connections_filtered(protocol, state)`              |
| Disk Info            | `backend.disks()`                                                                     |
| System Info          | `backend.system_info()`, `backend.hostname()`, `backend.os_info()`                    |
| AI Agent             | `backend.ask_agent("question")`, `backend.has_agent()`                                |

### FullSystemState

For AI context or data export, get a complete snapshot:

```rust
let state = backend.get_full_system_state();

// Convert to natural language for AI context
let context = state.to_context_string();

// Serialize to JSON for export
let json = serde_json::to_string(&state)?;
```

## AI Agent Integration

The AI agent is now consistently available across all modes:

### GUI
- New "🤖 AI" tab provides chat interface
- Agent initialized on startup
- Chat history preserved during session

### TUI  
- Press `a` to enter agent query mode
- Agent responses displayed in dedicated area
- History maintained across queries

### CLI
- Use `amon.exe` for AI-assisted monitoring
- Same agent backend as GUI/TUI

## History Buffers

All modes share the same history buffer implementation:

```rust
use ironmon::HistoryBuffer;

let mut history: HistoryBuffer<f32> = HistoryBuffer::new(60);
history.push(42.5);

// Get for rendering
let values: Vec<f32> = history.to_vec();
let latest = history.latest();
```

## Configuration

### BackendConfig

```rust
pub struct BackendConfig {
    /// Enable AI agent (default: true)
    pub enable_agent: bool,
    
    /// AI model size (default: Medium)
    pub agent_model_size: ModelSize,
    
    /// Agent timeout in seconds (default: 10)
    pub agent_timeout_secs: u64,
    
    /// History buffer size (default: 60)
    pub history_size: usize,
    
    /// Update interval (default: 1 second)
    pub update_interval: Duration,
}
```

### Quick Configuration Options

```rust
// Fast startup without AI agent
let config = BackendConfig::without_agent();

// Custom history and interval
let config = BackendConfig::default()
    .with_history_size(300)  // 5 minutes at 1s interval
    .with_update_interval(Duration::from_millis(250));
```

## Example Usage

```rust
use ironmon::{MonitoringBackend, BackendConfig};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create backend
    let mut backend = MonitoringBackend::new()?;
    
    // Main monitoring loop
    loop {
        // Update if needed
        backend.update_if_needed()?;
        
        // Read CPU
        if let Some(cpu) = backend.cpu_stats() {
            println!("CPU: {:.1}%", 100.0 - cpu.total.idle);
        }
        
        // Read GPUs
        for (i, gpu) in backend.gpu_dynamic_info().iter().enumerate() {
            println!("GPU {}: {}% util, {}°C", 
                i, 
                gpu.utilization,
                gpu.thermal.temperature.unwrap_or(0)
            );
        }
        
        // Ask AI agent
        if backend.has_agent() {
            let response = backend.ask_agent("What's using my GPU?")?;
            println!("AI: {}", response.response);
        }
        
        std::thread::sleep(Duration::from_secs(1));
    }
}
```

## Migration Guide

### From TUI-specific Code

Before:
```rust
// TUI was reading directly from platform modules
let cpu = crate::platform::linux::read_cpu_stats()?;
```

After:
```rust
// Use unified backend
let backend = MonitoringBackend::new()?;
let cpu = backend.cpu_stats();
```

### From GUI-specific Code

Before:
```rust
// GUI had separate initialization for each monitor
let gpu_collection = GpuCollection::auto_detect()?;
let process_monitor = ProcessMonitor::new()?;
```

After:
```rust
// Backend handles all initialization
let backend = MonitoringBackend::new()?;
// All monitors available through backend methods
```

## Architecture Benefits

1. **Consistency** - Same data across all modes
2. **Efficiency** - Single initialization, shared state
3. **Maintainability** - One place to update data access
4. **AI Context** - Full system context available to agent
5. **Testability** - Mock backend for unit tests
