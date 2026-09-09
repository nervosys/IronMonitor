// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Shared Windows GPU monitoring helpers
//!
//! Provides common functionality for Windows GPU backends using DXGI, WMI
//! performance counters, and hardware monitor integration. Used by both
//! the AMD and Intel Windows GPU implementations.

#![cfg(windows)]

use serde::Deserialize;
use std::collections::HashMap;
use wmi::{COMLibrary, Variant, WMIConnection};

/// DXGI adapter information obtained via `IDXGIFactory1::EnumAdapters`.
#[derive(Debug, Clone)]
pub struct DxgiAdapterInfo {
    /// Adapter description / name
    pub description: String,
    /// Dedicated video memory in bytes (accurate, no 4GB cap)
    pub dedicated_video_memory: u64,
    /// Shared system memory in bytes
    pub shared_system_memory: u64,
    /// Vendor ID (e.g., 0x1002 for AMD, 0x8086 for Intel, 0x10DE for NVIDIA)
    pub vendor_id: u32,
    /// Device ID
    pub device_id: u32,
    /// LUID low part — used to match with GPU performance counters
    pub luid_low: u32,
    /// LUID high part
    pub luid_high: i32,
}

/// Enumerate DXGI adapters to get accurate VRAM size and LUID.
///
/// Returns a list of all GPU adapters found via DXGI, each with accurate
/// 64-bit dedicated video memory and a LUID for matching with Windows
/// GPU performance counters.
pub fn enumerate_dxgi_adapters() -> Vec<DxgiAdapterInfo> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    let mut adapters = Vec::new();

    let factory: IDXGIFactory1 = match unsafe { CreateDXGIFactory1() } {
        Ok(f) => f,
        Err(_) => return adapters,
    };

    let mut index = 0u32;
    loop {
        let adapter = match unsafe { factory.EnumAdapters(index) } {
            Ok(a) => a,
            Err(_) => break,
        };

        let desc = match unsafe { adapter.GetDesc() } {
            Ok(d) => d,
            Err(_) => {
                index += 1;
                continue;
            }
        };

        // Convert wide-char description to String
        let description = String::from_utf16_lossy(
            &desc
                .Description
                .iter()
                .copied()
                .take_while(|&c| c != 0)
                .collect::<Vec<u16>>(),
        );

        adapters.push(DxgiAdapterInfo {
            description,
            dedicated_video_memory: desc.DedicatedVideoMemory as u64,
            shared_system_memory: desc.SharedSystemMemory as u64,
            vendor_id: desc.VendorId,
            device_id: desc.DeviceId,
            luid_low: desc.AdapterLuid.LowPart,
            luid_high: desc.AdapterLuid.HighPart,
        });

        index += 1;
    }

    adapters
}

/// Find a DXGI adapter matching a GPU name (case-insensitive substring match).
pub fn find_dxgi_adapter(adapters: &[DxgiAdapterInfo], gpu_name: &str) -> Option<DxgiAdapterInfo> {
    let name_lower = gpu_name.to_lowercase();
    adapters
        .iter()
        .find(|a| a.description.to_lowercase().contains(&name_lower))
        .cloned()
}

/// Find a DXGI adapter by vendor ID (0x1002=AMD, 0x8086=Intel, 0x10DE=NVIDIA).
pub fn find_dxgi_adapter_by_vendor(
    adapters: &[DxgiAdapterInfo],
    vendor_id: u32,
) -> Option<DxgiAdapterInfo> {
    adapters.iter().find(|a| a.vendor_id == vendor_id).cloned()
}

/// Per-engine GPU utilization from Windows performance counters.
#[derive(Debug, Default, Clone)]
pub struct EngineUtilization {
    /// 3D / graphics engine utilization (0-100)
    pub graphics: Option<u8>,
    /// Compute engine utilization (0-100)
    pub compute: Option<u8>,
    /// Video decode engine utilization (0-100)
    pub video_decode: Option<u8>,
    /// Video encode engine utilization (0-100)
    pub video_encode: Option<u8>,
    /// Copy engine utilization (0-100)
    pub copy: Option<u8>,
    /// Overall combined utilization (0-100)
    pub overall: u8,
}

/// Full GPU performance data from Windows APIs.
#[derive(Debug, Default, Clone)]
pub struct WinGpuPerfData {
    /// Per-engine utilization breakdown
    pub engines: EngineUtilization,
    /// Dedicated VRAM used in bytes
    pub dedicated_used: u64,
    /// Shared memory used in bytes
    pub shared_used: u64,
    /// GPU temperature from OHM/LHM in Celsius
    pub temperature: Option<i32>,
}

/// Format a LUID as the hex string used in GPU perf counter names.
///
/// Windows GPU perf counter names use format:
/// `pid_NNNN_luid_0xHHHHHHHH_0xHHHHHHHH_phys_0_eng_N_engtype_TYPE`
///
/// This returns the `0xHHHHHHHH_0xHHHHHHHH` portion for matching.
pub fn format_luid(luid_high: i32, luid_low: u32) -> String {
    format!("0x{:08x}_0x{:08x}", luid_high, luid_low)
}

/// One WMI sampling of the GPU performance-counter classes, shared by all adapters.
struct PerfCounterRows {
    taken: std::time::Instant,
    /// `(instance name, utilization percent)` from the GPUEngine class.
    engines: Vec<(String, f64)>,
    /// `(instance name, dedicated bytes, shared bytes)` from GPUAdapterMemory.
    memory: Vec<(String, u64, u64)>,
}

static PERF_ROWS: std::sync::Mutex<Option<std::sync::Arc<PerfCounterRows>>> =
    std::sync::Mutex::new(None);

/// How long one sampling of the counter classes is reused.
///
/// Must stay below the collector's tick so every tick still gets a fresh reading;
/// its only job is to collapse the calls made by different adapters within a tick.
const PERF_ROWS_TTL: std::time::Duration = std::time::Duration::from_millis(400);

/// Sample both counter classes once, or reuse a recent sampling.
///
/// These classes are not per-adapter: each returns every adapter's instances, and
/// `GPUEngine` carries one row per process per engine. Every GPU adapter was
/// separately querying both whole classes, so a three-GPU machine issued six
/// whole-class queries per tick against a single WMI provider. They do not run in
/// parallel there — they queue — which is why an NVIDIA card served by NVML in 10ms
/// was taking 200-475ms, and why the cost swung by a factor of three between ticks.
/// Sampling once and sharing the rows removes both the duplicated work and the
/// variance.
///
/// Holding the lock across the query is deliberate: a concurrent caller waits for the
/// sampling already in flight instead of starting a competing one.
fn perf_counter_rows() -> Option<std::sync::Arc<PerfCounterRows>> {
    let mut cached = PERF_ROWS.lock().ok()?;
    if let Some(rows) = cached.as_ref() {
        if rows.taken.elapsed() < PERF_ROWS_TTL {
            return Some(std::sync::Arc::clone(rows));
        }
    }

    let com = COMLibrary::new().ok()?;
    let wmi = WMIConnection::with_namespace_path("root\\CIMV2", com).ok()?;

    // A `WHERE Name LIKE` predicate was tried here to make the provider do the
    // filtering. It made the query four to eight times slower — performance-counter
    // providers re-enumerate the class to evaluate the predicate — so the rows are
    // fetched whole and filtered below.
    let engines = wmi
        .raw_query::<HashMap<String, Variant>>(
            "SELECT Name, UtilizationPercentage FROM \
             Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine",
        )
        .unwrap_or_default()
        .iter()
        .filter_map(|item| {
            let name = match item.get("Name") {
                Some(Variant::String(s)) => s.to_lowercase(),
                _ => return None,
            };
            let util = match item.get("UtilizationPercentage") {
                Some(Variant::UI8(v)) => *v as f64,
                Some(Variant::UI4(v)) => *v as f64,
                _ => return None,
            };
            Some((name, util))
        })
        .collect();

    let memory = wmi
        .raw_query::<HashMap<String, Variant>>(
            "SELECT Name, DedicatedUsage, SharedUsage FROM \
             Win32_PerfFormattedData_GPUPerformanceCounters_GPUAdapterMemory",
        )
        .unwrap_or_default()
        .iter()
        .filter_map(|item| {
            let name = match item.get("Name") {
                Some(Variant::String(s)) => s.to_lowercase(),
                _ => return None,
            };
            let as_u64 = |v: Option<&Variant>| match v {
                Some(Variant::UI8(x)) => *x,
                Some(Variant::UI4(x)) => *x as u64,
                _ => 0,
            };
            Some((
                name,
                as_u64(item.get("DedicatedUsage")),
                as_u64(item.get("SharedUsage")),
            ))
        })
        .collect();

    let rows = std::sync::Arc::new(PerfCounterRows {
        taken: std::time::Instant::now(),
        engines,
        memory,
    });
    *cached = Some(std::sync::Arc::clone(&rows));
    Some(rows)
}

/// Query GPU performance counters from WMI, filtered by LUID for correct
/// per-adapter attribution on multi-GPU systems.
///
/// If `luid_filter` is `None`, all adapters are included (single-GPU fallback).
pub fn query_gpu_perf_counters(luid_filter: Option<&str>) -> WinGpuPerfData {
    let mut data = WinGpuPerfData::default();

    let Some(rows) = perf_counter_rows() else {
        return data;
    };

    // ── Engine utilization per type ──────────────────────────────────────
    {
        let results = &rows.engines;
        let mut gfx_total: f64 = 0.0;
        let mut gfx_count: u32 = 0;
        let mut compute_total: f64 = 0.0;
        let mut compute_count: u32 = 0;
        let mut vdec_total: f64 = 0.0;
        let mut vdec_count: u32 = 0;
        let mut venc_total: f64 = 0.0;
        let mut venc_count: u32 = 0;
        let mut copy_total: f64 = 0.0;
        let mut copy_count: u32 = 0;

        for (name, util) in results {
            let (name, util) = (name.as_str(), *util);

            // Filter by LUID if provided (ensures correct adapter on multi-GPU)
            if let Some(luid) = luid_filter {
                if !name.contains(&luid.to_lowercase()) {
                    continue;
                }
            }

            // Classify by engine type
            if name.contains("engtype_3d") {
                gfx_total += util;
                gfx_count += 1;
            } else if name.contains("engtype_compute") {
                compute_total += util;
                compute_count += 1;
            } else if name.contains("engtype_videodecode") {
                vdec_total += util;
                vdec_count += 1;
            } else if name.contains("engtype_videoencode") {
                venc_total += util;
                venc_count += 1;
            } else if name.contains("engtype_copy") {
                copy_total += util;
                copy_count += 1;
            }
        }

        // Average per engine type
        let avg = |total: f64, count: u32| -> Option<u8> {
            if count > 0 {
                Some((total / count as f64).min(100.0) as u8)
            } else {
                None
            }
        };

        data.engines.graphics = avg(gfx_total, gfx_count);
        data.engines.compute = avg(compute_total, compute_count);
        data.engines.video_decode = avg(vdec_total, vdec_count);
        data.engines.video_encode = avg(venc_total, venc_count);
        data.engines.copy = avg(copy_total, copy_count);

        // Overall = max of graphics and compute (most representative)
        let total_sum = gfx_total + compute_total + vdec_total + venc_total + copy_total;
        let total_count = gfx_count + compute_count + vdec_count + venc_count + copy_count;
        data.engines.overall = if total_count > 0 {
            // Use the graphics engine as primary, fall back to average
            data.engines
                .graphics
                .unwrap_or((total_sum / total_count as f64).min(100.0) as u8)
        } else {
            0
        };
    }

    // ── Adapter memory usage ────────────────────────────────────────────
    for (name, dedicated, shared) in &rows.memory {
        // Filter by LUID if provided
        if let Some(luid) = luid_filter {
            if !name.contains(&luid.to_lowercase()) {
                continue;
            }
        }

        data.dedicated_used = *dedicated;
        data.shared_used = *shared;

        if data.dedicated_used > 0 || data.shared_used > 0 {
            break;
        }
    }

    data
}

/// OHM/LHM sensor result
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct HwMonSensor {
    name: String,
    #[allow(dead_code)]
    sensor_type: String,
    value: f32,
    parent: String,
}

/// Namespaces this probe understands, in preference order.
const HWMON_NAMESPACES: [&str; 2] = ["root\\OpenHardwareMonitor", "root\\LibreHardwareMonitor"];

/// Which hardware-monitor namespace is present, and when that was last determined.
///
/// Neither namespace exists unless the user installed OHM or LHM, and connecting to
/// an absent WMI namespace is not cheap. This probe runs once per GPU per collector
/// tick, so on the common machine it was paying for two failed namespace connections
/// every tick, forever, to learn the same thing each time. The answer is cached and
/// re-probed occasionally, so installing a hardware monitor is still picked up
/// without restarting ironmon.
static HWMON_NAMESPACE: std::sync::Mutex<Option<(std::time::Instant, Option<&'static str>)>> =
    std::sync::Mutex::new(None);

/// How long a namespace probe result stays good.
const HWMON_PROBE_TTL: std::time::Duration = std::time::Duration::from_secs(60);

/// Locate an installed OHM/LHM WMI namespace, using the cached answer when fresh.
fn hwmon_namespace(com: COMLibrary) -> Option<&'static str> {
    let mut cached = HWMON_NAMESPACE.lock().ok()?;
    if let Some((probed_at, answer)) = *cached {
        if probed_at.elapsed() < HWMON_PROBE_TTL {
            return answer;
        }
    }

    let answer = HWMON_NAMESPACES
        .into_iter()
        .find(|ns| WMIConnection::with_namespace_path(ns, com).is_ok());
    *cached = Some((std::time::Instant::now(), answer));
    answer
}

/// Query GPU-specific temperature from OpenHardwareMonitor or LibreHardwareMonitor.
///
/// Filters by parent containing the GPU name hint (e.g., "Radeon", "Intel", "Arc").
/// Falls back to any GPU-tagged temperature sensor if exact match fails.
///
/// Returns `None` when no hardware monitor is installed. It used to fall back to an
/// ACPI thermal zone, but a chassis thermal zone is not this GPU's temperature — it
/// is a different sensor on a different part of the machine, and reporting it as the
/// GPU's reading is a guess presented as a measurement.
pub fn query_gpu_temperature_ohm(gpu_name_hint: &str) -> Option<i32> {
    let com = COMLibrary::new().ok()?;
    let namespace = hwmon_namespace(com)?;

    let hint_lower = gpu_name_hint.to_lowercase();

    {
        let wmi = WMIConnection::with_namespace_path(namespace, com).ok()?;

        let sensors: Vec<HwMonSensor> = wmi
            .raw_query(
                "SELECT Name, SensorType, Value, Parent FROM Sensor WHERE SensorType = 'Temperature'",
            )
            .unwrap_or_default();

        // First pass: exact match on GPU name
        for sensor in &sensors {
            let parent_lower = sensor.parent.to_lowercase();
            let name_lower = sensor.name.to_lowercase();

            if (parent_lower.contains(&hint_lower) || name_lower.contains(&hint_lower))
                && sensor.value > 0.0
                && sensor.value < 150.0
            {
                return Some(sensor.value as i32);
            }
        }

        // Second pass: any GPU temperature
        for sensor in &sensors {
            let parent_lower = sensor.parent.to_lowercase();
            let name_lower = sensor.name.to_lowercase();

            let is_gpu = parent_lower.contains("gpu")
                || parent_lower.contains("radeon")
                || parent_lower.contains("amd")
                || parent_lower.contains("intel")
                || parent_lower.contains("arc")
                || name_lower.contains("gpu");

            if is_gpu && sensor.value > 0.0 && sensor.value < 150.0 {
                return Some(sensor.value as i32);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_luid() {
        assert_eq!(format_luid(0, 0x0000ABCD), "0x00000000_0x0000abcd");
        assert_eq!(format_luid(1, 0xDEADBEEF), "0x00000001_0xdeadbeef");
    }

    #[test]
    fn test_engine_utilization_default() {
        let eng = EngineUtilization::default();
        assert!(eng.graphics.is_none());
        assert!(eng.compute.is_none());
        assert!(eng.video_decode.is_none());
        assert!(eng.video_encode.is_none());
        assert!(eng.copy.is_none());
        assert_eq!(eng.overall, 0);
    }

    #[test]
    fn test_win_gpu_perf_data_default() {
        let data = WinGpuPerfData::default();
        assert_eq!(data.engines.overall, 0);
        assert_eq!(data.dedicated_used, 0);
        assert_eq!(data.shared_used, 0);
        assert!(data.temperature.is_none());
    }

    #[test]
    fn test_dxgi_adapter_info_fields() {
        let info = DxgiAdapterInfo {
            description: "Test GPU".to_string(),
            dedicated_video_memory: 8 * 1024 * 1024 * 1024, // 8 GB
            shared_system_memory: 16 * 1024 * 1024 * 1024,
            vendor_id: 0x1002,
            device_id: 0x73BF,
            luid_low: 0x0000ABCD,
            luid_high: 0,
        };
        assert_eq!(info.dedicated_video_memory, 8589934592);
        assert_eq!(info.vendor_id, 0x1002);
    }
}
