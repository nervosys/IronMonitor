# IronMonitor (ironmon) - Comprehensive Feature Matrix

## Goal: 10x Better Experience - All Silicon Metrics in One Place with Modern Graphs

### Comparison with Existing Tools

| Feature Category           | nvidia-smi | rocm-smi | nvitop     | gpustat   | gpu-exporter | **ironmon** (Target) |
| -------------------------- | ---------- | -------- | ---------- | --------- | ------------ | ------------------ |
| **Platform Support**       |
| NVIDIA GPUs                | ✅ Full     | ❌        | ✅ Full     | ✅ Full    | ✅ Full       | ✅ **Full**         |
| AMD GPUs                   | ❌          | ✅ Full   | ❌          | ❌         | ❌            | ✅ **Full**         |
| Intel GPUs                 | ❌          | ❌        | ❌          | ❌         | ❌            | ✅ **Full**         |
| Apple Silicon              | ❌          | ❌        | ❌          | ❌         | ❌            | ✅ **Full**         |
| **Device Information**     |
| GPU Name/Model             | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| Driver Version             | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| CUDA/ROCm Version          | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| UUID/Serial Number         | ✅          | ✅        | ✅          | ❌         | ✅            | ✅                  |
| PCI Bus ID                 | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| Persistence Mode           | ✅          | ❌        | ✅          | ❌         | ✅            | ✅                  |
| Compute Mode               | ✅          | ❌        | ✅          | ❌         | ✅            | ✅                  |
| MIG Mode                   | ✅          | ❌        | ✅          | ❌         | ✅            | ✅                  |
| vGPU Support               | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **Enhanced**     |
| **Temperature Monitoring** |
| GPU Temperature            | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| Edge Temperature           | ✅          | ✅        | ✅          | ❌         | ✅            | ✅                  |
| Junction/Hotspot           | ✅          | ✅        | ✅          | ❌         | ✅            | ✅                  |
| Memory Temperature         | ✅          | ✅        | ✅          | ❌         | ✅            | ✅                  |
| HBM Temperature            | ✅          | ✅        | ❌          | ❌         | ✅            | ✅                  |
| VR Temperature             | ✅          | ✅        | ❌          | ❌         | ❌            | ✅ **Enhanced**     |
| **Power Management**       |
| Current Power Draw         | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| Average Power Draw         | ✅          | ✅        | ✅          | ❌         | ✅            | ✅                  |
| Power Limit                | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| Power Cap Control          | ✅          | ✅        | ❌          | ❌         | ❌            | ✅ **Interactive**  |
| Power Smoothing            | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **New**          |
| Power Profiles             | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **New**          |
| Energy Accumulator         | ✅          | ✅        | ❌          | ❌         | ✅            | ✅                  |
| **Utilization Metrics**    |
| GPU Utilization            | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| Memory Utilization         | ✅          | ✅        | ✅          | ❌         | ✅            | ✅                  |
| Encoder Utilization        | ✅          | ✅        | ✅          | ❌         | ✅            | ✅                  |
| Decoder Utilization        | ✅          | ✅        | ✅          | ❌         | ✅            | ✅                  |
| JPEG Utilization           | ✅          | ✅        | ✅          | ❌         | ❌            | ✅                  |
| OFA Utilization            | ✅          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| **Clock Frequencies**      |
| Graphics Clock             | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| Memory Clock               | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| SM Clock                   | ✅          | ❌        | ✅          | ❌         | ✅            | ✅                  |
| Video Clock                | ✅          | ❌        | ❌          | ❌         | ❌            | ✅                  |
| Application Clocks         | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **Control**      |
| Clock Locking              | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **Control**      |
| **Memory Management**      |
| Total Memory               | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| Used Memory                | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| Free Memory                | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| BAR1 Memory                | ✅          | ❌        | ✅          | ❌         | ✅            | ✅                  |
| Memory Percent             | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| **Performance States**     |
| Performance State          | ✅          | ❌        | ✅          | ❌         | ✅            | ✅                  |
| Throttle Reasons           | ✅          | ✅        | ❌          | ❌         | ✅            | ✅ **Enhanced**     |
| Clock Event Reasons        | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **New**          |
| Boost Mode                 | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **Control**      |
| **Process Monitoring**     |
| Process List               | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| Per-Process GPU Memory     | ✅          | ✅        | ✅          | ✅         | ✅            | ✅                  |
| Per-Process SM Util        | ✅          | ✅        | ✅          | ❌         | ✅            | ✅                  |
| Per-Process Encoder        | ✅          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| Per-Process Decoder        | ✅          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| Process Type (C/G)         | ✅          | ❌        | ✅          | ✅         | ✅            | ✅                  |
| CPU Percent                | ❌          | ❌        | ✅          | ❌         | ❌            | ✅ **Enhanced**     |
| Host Memory                | ❌          | ❌        | ✅          | ❌         | ❌            | ✅ **Enhanced**     |
| Process Tree View          | ❌          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| Environment Variables      | ❌          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| Process Control (Kill)     | ❌          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| Accounting Mode            | ✅          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| **Error Monitoring**       |
| ECC Errors                 | ✅          | ✅        | ✅          | ❌         | ✅            | ✅                  |
| Page Retirement            | ✅          | ❌        | ✅          | ❌         | ✅            | ✅                  |
| Row Remapping              | ✅          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| PCIe Replay Errors         | ✅          | ❌        | ❌          | ❌         | ✅            | ✅                  |
| Xid Errors                 | ✅          | ❌        | ❌          | ❌         | ❌            | ✅                  |
| **Connectivity**           |
| PCIe Generation            | ✅          | ✅        | ✅          | ❌         | ✅            | ✅                  |
| PCIe Link Width            | ✅          | ✅        | ✅          | ❌         | ✅            | ✅                  |
| PCIe Throughput            | ✅          | ✅        | ❌          | ❌         | ✅            | ✅                  |
| NVLink Status              | ✅          | ❌        | ❌          | ❌         | ✅            | ✅ **Enhanced**     |
| NVLink Throughput          | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **New**          |
| NVLink Error Counters      | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **New**          |
| C2C Support                | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **New**          |
| **Topology**               |
| GPU-to-GPU Matrix          | ✅          | ❌        | ❌          | ❌         | ❌            | ✅                  |
| CPU Affinity               | ✅          | ❌        | ❌          | ❌         | ❌            | ✅                  |
| NUMA Node                  | ✅          | ❌        | ❌          | ❌         | ❌            | ✅                  |
| P2P Capabilities           | ✅          | ❌        | ❌          | ❌         | ❌            | ✅                  |
| **Advanced Features**      |
| MIG Instance Mgmt          | ✅          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| vGPU Management            | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **New**          |
| GPU Reset                  | ✅          | ✅        | ❌          | ❌         | ❌            | ✅ **Safe**         |
| Fabric Info                | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **New**          |
| Confidential Compute       | ✅          | ❌        | ❌          | ❌         | ❌            | ✅ **New**          |
| **User Interface**         |
| CLI Output                 | ✅ Basic    | ✅ Basic  | ✅ Rich     | ✅ Minimal | ❌            | ✅ **Colorful**     |
| Monitor Mode               | ❌          | ❌        | ✅ Advanced | ✅ Basic   | ❌            | ✅ **Modern TUI**   |
| History Graphs             | ❌          | ❌        | ✅ 300s     | ❌         | ❌            | ✅ **Configurable** |
| Bar Charts                 | ❌          | ❌        | ✅          | ✅         | ❌            | ✅ **Enhanced**     |
| Mouse Support              | ❌          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| Process Filtering          | ❌          | ❌        | ✅          | ❌         | ❌            | ✅ **Advanced**     |
| Process Sorting            | ❌          | ❌        | ✅          | ✅         | ❌            | ✅ **Multi-key**    |
| Real-time Metrics          | ❌          | ❌        | ✅          | ❌         | ❌            | ✅ **Per-Process**  |
| **Export Formats**         |
| Text Output                | ✅          | ✅        | ✅          | ✅         | ❌            | ✅                  |
| XML Output                 | ✅          | ❌        | ❌          | ❌         | ❌            | ✅                  |
| CSV Output                 | ✅          | ❌        | ✅          | ❌         | ❌            | ✅ **Enhanced**     |
| JSON Output                | ❌          | ✅        | ✅          | ✅         | ❌            | ✅ **Rich**         |
| Prometheus Metrics         | ❌          | ❌        | ❌          | ❌         | ✅            | ✅ **Full**         |
| **Data Collection**        |
| Daemon Mode                | ✅          | ❌        | ❌          | ❌         | ✅            | ✅ **Enhanced**     |
| Metric Collector           | ❌          | ❌        | ✅          | ❌         | ✅            | ✅ **Async**        |
| Time-series Aggregation    | ❌          | ❌        | ✅          | ❌         | ❌            | ✅ **Mean/Min/Max** |
| Callback Functions         | ❌          | ❌        | ✅          | ❌         | ❌            | ✅ **Flexible**     |
| Log Rotation               | ✅          | ❌        | ❌          | ❌         | ❌            | ✅                  |
| **CPU Monitoring**         |
| CPU Utilization            | ❌          | ❌        | ✅          | ❌         | ❌            | ✅ **Per-Core**     |
| CPU Temperature            | ❌          | ❌        | ❌          | ❌         | ❌            | ✅ **Multi-source** |
| Hybrid CPU Detection       | ❌          | ❌        | ❌          | ❌         | ❌            | ✅ **P/E Cores**    |
| Load Average               | ❌          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| Uptime                     | ❌          | ❌        | ❌          | ❌         | ❌            | ✅                  |
| **Memory Monitoring**      |
| Host Memory Usage          | ❌          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| Swap Memory                | ❌          | ❌        | ✅          | ❌         | ❌            | ✅                  |
| Memory Percent             | ❌          | ❌        | ✅          | ❌         | ❌            | ✅                  |

### Legend
- ✅ = Fully supported
- 🚧 = Partial support
- ❌ = Not supported
- **Bold** = Enhanced/New feature in ironmon

### IronMonitor's 10x Better Experience

1. **Unified Multi-Vendor Support**
   - NVIDIA (NVML), AMD (ROCm SMI), Intel (Level Zero), Apple (Metal/IOKit)
   - Single tool for all GPU vendors
   - Consistent API across platforms

2. **Comprehensive Metrics**
   - 150+ metrics per GPU vs. 30-50 in existing tools
   - Advanced features: power smoothing, profiles, confidential compute
   - Complete topology and connectivity information

3. **Modern Interactive TUI**
   - Built with ratatui (modern Rust TUI framework)
   - Smooth animations and responsive updates
   - Mouse support for all operations
   - Process metrics with 300s history graphs

4. **Enhanced Process Monitoring**
   - Per-process encoder/decoder utilization
   - CPU and host memory tracking
   - Process tree view with parent-child relationships
   - Environment variable inspection
   - Signal control (TERM/KILL/INT)

5. **Professional Data Export**
   - Prometheus exposition format for monitoring
   - Time-series CSV with mean/min/max aggregation
   - JSON with rich metadata
   - XML compatibility with nvidia-smi tools

6. **Resource Metric Collector**
   - Async collection in background
   - Configurable intervals (50ms to hours)
   - Automatic aggregation and export
   - TensorBoard integration for ML training

7. **Cross-Platform CPU/Memory**
   - Per-core CPU utilization and temperature
   - Hybrid CPU support (Intel P/E cores, Apple clusters)
   - Load average, uptime, memory pressure
   - All platforms: Linux, Windows, macOS

8. **Advanced Error Monitoring**
   - ECC single/double bit errors with location
   - Page retirement and row remapping tracking
   - PCIe replay counters and rollover detection
   - Xid error event streaming

9. **Connectivity Features**
   - NVLink bandwidth, error counters, power states
   - C2C link management
   - Multi-node fabric topology
   - P2P capability matrix

10. **Developer-Friendly API**
    - Rust library with zero-cost abstractions
    - Python bindings for data science workflows
    - Callback system for custom monitoring
    - Plugin architecture for extensions

## Implementation Roadmap

### Phase 1: GPU Monitoring Backends (Task 6, 8)
1. NVIDIA NVML integration (nvidia-ml-rs)
2. AMD ROCm SMI integration
3. Intel Level Zero integration
4. Apple Metal/IOKit enhancement
5. Unified Device trait abstraction

### Phase 2: Process Monitoring (Task 9)
1. Per-process GPU metrics
2. Host process tracking (CPU, memory)
3. Process control and signals
4. Filtering and sorting
5. Process tree relationships

### Phase 3: Modern TUI (Task 7)
1. Ratatui framework setup
2. Multi-panel layout (device, host, process)
3. Interactive controls (mouse, keyboard)
4. History graphs and visualizations
5. Tree-view and metrics screens

### Phase 4: Data Collection (Task 10)
1. ResourceMetricCollector implementation
2. Async metric aggregation
3. Export formats (CSV, JSON, Prometheus)
4. Daemonization and callbacks
5. TensorBoard plugin

### Phase 5: NPU and Advanced Features (Tasks 11-14)
1. NPU/ASIC monitoring (ANE, Intel NPU, XDNA, Tensor Cores)
2. I/O controller tracking
3. Network silicon monitoring
4. Enhanced memory monitoring
5. Confidential compute support

### Phase 6: Polish and Documentation
1. Comprehensive testing
2. Performance optimization
3. Documentation and examples
4. Python bindings
5. Package and release
