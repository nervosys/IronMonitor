//! Linux GPU monitoring

use crate::core::gpu::{GpuFrequency, GpuInfo, GpuStats, GpuStatus, GpuType};
use crate::error::Result;
use crate::platform::common::*;
use std::collections::HashMap;
use std::path::Path;

#[cfg(feature = "nvml")]
use nvml_wrapper::Nvml;

/// Read GPU statistics
pub fn read_gpu_stats() -> Result<GpuStats> {
    let mut stats = GpuStats::new();

    // Try NVML first (for Jetson 7.0+ and desktop GPUs)
    #[cfg(feature = "nvml")]
    {
        if let Ok(nvml_gpus) = read_nvml_gpus() {
            if !nvml_gpus.is_empty() {
                *stats.gpus_mut() = nvml_gpus;
                return Ok(stats);
            }
        }
    }

    // Fallback to Jetson sysfs method
    let jetson_gpus = read_jetson_gpus()?;
    *stats.gpus_mut() = jetson_gpus;

    Ok(stats)
}

#[cfg(feature = "nvml")]
fn read_nvml_gpus() -> Result<HashMap<String, GpuInfo>> {
    let nvml = Nvml::init()?;
    let mut gpus = HashMap::new();

    let device_count = nvml.device_count()?;

    for i in 0..device_count {
        let device = nvml.device_by_index(i)?;

        // Get device name
        let mut name = device.name()?;
        if name.starts_with("NVIDIA ") {
            name = name.replace("NVIDIA ", "");
        }

        // Get utilization.
        //
        // `GpuStatus::load` became `Option<f32>` in `549ca3e`, so that a card at
        // 0% and a card whose counter could not be read stop being the same
        // number. This path kept `unwrap_or(0.0)` and so kept publishing the
        // second as the first -- and, because the field is now an `Option`, it
        // also stopped compiling. **Every Linux CI job has been red since
        // 2026-09-03 for this one line**: `error[E0308]` in a
        // `cfg(feature = "nvml")` block that a Windows compiler never reads and
        // that the handoff's cross-check feature list does not enable.
        let load = device.utilization_rates().ok().map(|u| u.gpu as f32);

        // Get memory info
        let memory_info = device.memory_info().ok();
        let (memory_used, memory_total, memory_free) = if let Some(mem) = memory_info {
            (Some(mem.used), Some(mem.total), Some(mem.free))
        } else {
            (None, None, None)
        };

        // Get temperature
        let temperature = device
            .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            .ok()
            .map(|t| t as f32);

        // Get power info
        let power_draw = device.power_usage().ok().map(|p| p as f32 / 1000.0); // mW to W
        let power_limit = device
            .power_management_limit()
            .ok()
            .map(|p| p as f32 / 1000.0);

        // Get clock speeds
        let sm_clock = device
            .clock_info(nvml_wrapper::enum_wrappers::device::Clock::SM)
            .ok()
            .unwrap_or(0);
        let max_sm_clock = device
            .max_clock_info(nvml_wrapper::enum_wrappers::device::Clock::SM)
            .ok()
            .unwrap_or(0);

        let status = GpuStatus {
            load,
            railgate: None,
            tpc_pg_mask: None,
            scaling_3d: None,
            memory_used,
            memory_total,
            memory_free,
            temperature,
            power_draw,
            power_limit,
        };

        let frequency = GpuFrequency {
            current: sm_clock,
            min: 0, // NVML doesn't provide min
            max: max_sm_clock,
            governor: "nvml".to_string(),
            gpc: None,
        };

        let info = GpuInfo {
            // An NVML-enumerated device on Linux is a discrete card. The
            // integrated case is Jetson, which `read_jetson_gpus` below
            // enumerates through devfreq instead and labels correctly. This
            // said `Integrated` with the comment "Could be discrete" beside it.
            gpu_type: GpuType::Discrete,
            status,
            frequency,
            power_control: "nvml".to_string(),
        };

        gpus.insert(name, info);
    }

    Ok(gpus)
}

fn read_jetson_gpus() -> Result<HashMap<String, GpuInfo>> {
    let mut gpus = HashMap::new();

    use super::jetson::find_jetson_gpus;
    let gpu_list = find_jetson_gpus()?;

    for (name, device_path) in gpu_list {
        let frq_path = format!("/sys/class/devfreq/{}", name);

        // Read status
        let status = read_igpu_status(&device_path)?;

        // Read frequency
        let frequency = read_igpu_frequency(&frq_path)?;

        let info = GpuInfo {
            gpu_type: GpuType::Integrated,
            status,
            frequency,
            power_control: "sysfs".to_string(),
        };

        gpus.insert(name, info);
    }

    Ok(gpus)
}

fn read_igpu_status(device_path: &Path) -> Result<GpuStatus> {
    let railgate = if path_exists(device_path.join("railgate_enable")) {
        read_file_u32(device_path.join("railgate_enable"))
            .ok()
            .map(|v| v == 1)
    } else {
        None
    };

    let tpc_pg_mask = if path_exists(device_path.join("tpc_pg_mask")) {
        read_file_u32(device_path.join("tpc_pg_mask"))
            .ok()
            .map(|v| v == 1)
    } else {
        None
    };

    let scaling_3d = if path_exists(device_path.join("enable_3d_scaling")) {
        read_file_u32(device_path.join("enable_3d_scaling"))
            .ok()
            .map(|v| v == 1)
    } else {
        None
    };

    // Every sibling above uses `path_exists ... else None`; this one alone
    // answered `0.0` both when the file was missing and when it failed to
    // parse, which is a GPU reported as idle on a machine that exposes no
    // `load` node at all.
    let load = if path_exists(device_path.join("load")) {
        read_file_u32(device_path.join("load"))
            .ok()
            .map(|v| v as f32 / 10.0)
    } else {
        None
    };

    Ok(GpuStatus {
        load,
        railgate,
        tpc_pg_mask,
        scaling_3d,
        memory_used: None,
        memory_total: None,
        memory_free: None,
        temperature: None,
        power_draw: None,
        power_limit: None,
    })
}

fn read_igpu_frequency(frq_path: &str) -> Result<GpuFrequency> {
    let governor = read_file_string(format!("{}/governor", frq_path))
        .unwrap_or_else(|_| "unknown".to_string());

    let current = read_file_u32(format!("{}/cur_freq", frq_path)).unwrap_or(0) / 1000; // kHz to MHz

    let max = read_file_u32(format!("{}/max_freq", frq_path)).unwrap_or(0) / 1000;

    let min = read_file_u32(format!("{}/min_freq", frq_path)).unwrap_or(0) / 1000;

    Ok(GpuFrequency {
        current,
        min,
        max,
        governor,
        gpc: None,
    })
}
