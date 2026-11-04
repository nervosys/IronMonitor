# IronMonitor Grafana Dashboards

Pre-built Grafana dashboards for IronMonitor (ironmon) metrics.

## Setup

### 1. Enable Prometheus Endpoint

Start ironmon with the HTTP server and Prometheus exporter:

```bash
ironmon serve --port 9100
```

By default this binds to loopback and serves read-only endpoints without
authentication. That is safe only on the machine itself. To scrape it from another
host, bind to the network and require a key, because the bind makes full hardware
telemetry readable by anything that can reach the port:

```bash
ironmon serve --port 9100 --bind 0.0.0.0 --api-key-file /etc/ironmon/api-key
```

The key is read from a file, never from the command line. Without a key,
`--bind 0.0.0.0` still starts, but prints a warning on stderr; that setup is
supported for existing scrapers and is not recommended. Responses carry no CORS
headers, which Prometheus does not need.

### 2. Configure Prometheus

Metrics are served at `/api/v1/metrics/prometheus`, not at Prometheus's default
`/metrics`, so `metrics_path` must be set explicitly:

```yaml
scrape_configs:
  - job_name: 'ironmon'
    scrape_interval: 5s
    metrics_path: /api/v1/metrics/prometheus
    authorization:
      type: Bearer
      credentials_file: /etc/prometheus/ironmon-api-key
    static_configs:
      - targets: ['localhost:9100']
        labels:
          instance: 'my-host'
```

The `authorization` block is needed only when the server was started with a key. The
credentials file must hold the same key as the server's `--api-key-file`.

Collection starts when the endpoint is scraped and stops after 5 minutes without a
scrape. The first scrape after an idle period answers at once, from the values last
recorded, so it can show readings older than the scrape. Fresh values arrive within
one collection interval, so the first panels fill in on the second or third scrape.
A metric with no reading is absent from the output, not zero, so a gap in a panel
means no reading, not a reading of zero.

### 3. Import Dashboards

1. Open Grafana → Dashboards → Import
2. Upload JSON file or paste contents
3. Select your Prometheus data source
4. Click Import

## Dashboards

| Dashboard | File | Description |
|-----------|------|-------------|
| Fleet Overview | `fleet-overview.json` | Multi-host fleet monitoring with health scores |
| GPU Detail | `gpu-detail.json` | Per-GPU metrics with temperature, utilization, memory, power |
| Host Detail | `host-detail.json` | Single-host deep dive with CPU, memory, disk, network |

## Metric Reference

All metrics use the `ironmon_` prefix:

| Metric | Type | Description |
|--------|------|-------------|
| `ironmon_cpu_usage_percent` | gauge | CPU usage per core |
| `ironmon_cpu_frequency_mhz` | gauge | CPU frequency per core |
| `ironmon_cpu_temperature_celsius` | gauge | CPU temperature |
| `ironmon_memory_used_bytes` | gauge | Used memory |
| `ironmon_memory_total_bytes` | gauge | Total memory |
| `ironmon_memory_usage_percent` | gauge | Memory usage percentage |
| `ironmon_swap_used_bytes` | gauge | Used swap |
| `ironmon_swap_total_bytes` | gauge | Total swap |
| `ironmon_gpu_temperature_celsius` | gauge | GPU temperature |
| `ironmon_gpu_utilization_percent` | gauge | GPU utilization |
| `ironmon_gpu_memory_used_bytes` | gauge | GPU memory used |
| `ironmon_gpu_memory_total_bytes` | gauge | GPU memory total |
| `ironmon_gpu_power_watts` | gauge | GPU power draw |
| `ironmon_gpu_clock_graphics_mhz` | gauge | GPU graphics clock |
| `ironmon_gpu_clock_memory_mhz` | gauge | GPU memory clock |
| `ironmon_gpu_fan_speed_percent` | gauge | GPU fan speed |
| `ironmon_disk_used_bytes` | gauge | Disk space used |
| `ironmon_disk_total_bytes` | gauge | Disk space total |
| `ironmon_disk_usage_percent` | gauge | Disk usage percentage |
| `ironmon_network_rx_bytes_total` | counter | Network bytes received |
| `ironmon_network_tx_bytes_total` | counter | Network bytes transmitted |
| `ironmon_process_count` | gauge | Total process count |
| `ironmon_load_average_1m` | gauge | 1-minute load average |
| `ironmon_load_average_5m` | gauge | 5-minute load average |
| `ironmon_uptime_seconds` | gauge | System uptime |

## Customization

Dashboards use template variables for filtering:
- `$instance` — filter by host
- `$gpu` — filter by GPU index
- `$disk` — filter by disk device
- `$interface` — filter by network interface
