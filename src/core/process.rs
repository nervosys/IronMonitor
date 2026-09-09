//! Process monitoring

use serde::{Deserialize, Serialize};

/// Process information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    /// Process ID
    pub pid: u32,
    /// User running the process
    pub user: String,
    /// GPU used (I = Integrated, dX = Discrete)
    pub gpu: String,
    /// Process type (Graphic, System, etc.)
    pub process_type: String,
    /// Priority
    pub priority: i32,
    /// Process state (R, S, D, Z, T, etc.)
    pub state: char,
    /// CPU usage percentage
    pub cpu_percent: f32,
    /// Memory used in KB
    pub memory_kb: u64,
    /// GPU memory used in KB
    pub gpu_memory_kb: u64,
    /// Process name
    pub name: String,
}

/// Process statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessStats {
    /// GPU processes
    pub processes: Vec<ProcessInfo>,
    /// Total GPU memory used by all processes
    pub total_gpu_memory_kb: u64,
}

impl ProcessStats {
    /// A zeroed struct for a platform reader to fill in.
    ///
    /// **This reads nothing.** It returns no processes and zero GPU memory, and it was called `new()`
    /// until 6.0.0 — a name that reads like a constructor that gathers
    /// data. Two GUI defects, one in the HTTP server and one in the
    /// library's own `snapshot_process` came from exactly that misreading,
    /// found over three separate occasions.
    ///
    /// The real values come from the per-platform readers. Use those
    /// unless you are a reader building your own starting struct.
    pub fn empty() -> Self {
        Self {
            processes: Vec::new(),
            total_gpu_memory_kb: 0,
        }
    }

    /// Get process count
    pub fn process_count(&self) -> usize {
        self.processes.len()
    }

    /// Get processes sorted by GPU memory usage
    pub fn sorted_by_gpu_memory(&self) -> Vec<&ProcessInfo> {
        let mut procs: Vec<&ProcessInfo> = self.processes.iter().collect();
        procs.sort_by_key(|p| std::cmp::Reverse(p.gpu_memory_kb));
        procs
    }

    /// Get processes sorted by CPU usage
    pub fn sorted_by_cpu(&self) -> Vec<&ProcessInfo> {
        let mut procs: Vec<&ProcessInfo> = self.processes.iter().collect();
        procs.sort_by(|a, b| {
            b.cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        procs
    }
}

impl Default for ProcessStats {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(target_os = "linux")]
pub(crate) mod linux {
    use super::*;
    use crate::error::IronError;
    use crate::error::Result;
    use std::fs;
    use std::path::Path;

    /// Read process statistics (Linux)
    pub fn read_process_stats() -> Result<ProcessStats> {
        let mut stats = ProcessStats::empty();

        // Check if nvmap is available (Jetson)
        let nvmap_path = "/sys/kernel/debug/nvmap/iovmm/maps";
        if !Path::new(nvmap_path).exists() {
            // Not a Jetson or nvmap not available
            return Ok(stats);
        }

        // Parse nvmap table
        let (total, process_table) = parse_nvmap_table(nvmap_path)?;
        stats.total_gpu_memory_kb = total;

        // Read uptime for CPU calculation
        let uptime = read_uptime()?;

        // Get detailed info for each process
        for (pid, user, name, gpu_mem) in process_table {
            if let Ok(proc_info) = get_process_info(pid, &user, &name, gpu_mem, uptime) {
                stats.processes.push(proc_info);
            }
        }

        Ok(stats)
    }

    /// (pid, name, kind, bytes) for one nvmap client.
    ///
    /// Named because the bare tuple was long enough that clippy could not tell
    /// what any position meant — and neither could a reader.
    type NvmapRow = (u32, String, String, u64);

    fn parse_nvmap_table(path: &str) -> Result<(u64, Vec<NvmapRow>)> {
        let content = fs::read_to_string(path)?;
        let mut processes = Vec::new();
        let mut total = 0u64;

        for line in content.lines() {
            // Parse lines like: "user process PID sizeU"
            let parts: Vec<&str> = line.split_whitespace().collect();

            if parts.len() >= 4 {
                if parts[0] == "total" {
                    // Parse total line
                    if let Some(size_str) = parts.get(1) {
                        total = parse_size_with_unit(size_str);
                    }
                    continue;
                }

                // Parse process line
                if let Ok(pid) = parts[2].parse::<u32>() {
                    let user = parts[0].to_string();
                    let name = parts[1].to_string();
                    let size = parse_size_with_unit(parts[3]);

                    processes.push((pid, user, name, size));
                }
            }
        }

        Ok((total, processes))
    }

    fn parse_size_with_unit(size_str: &str) -> u64 {
        // Parse sizes like "1234K", "5M", "1G"
        if size_str.is_empty() {
            return 0;
        }

        // Security: Safely get last character without unwrap
        let last_char = match size_str.chars().last() {
            Some(c) => c,
            None => return 0,
        };

        let (num_str, unit) = if last_char.is_alphabetic() {
            let last_idx = size_str.len() - last_char.len_utf8();
            (&size_str[..last_idx], last_char)
        } else {
            (size_str, 'K')
        };

        let num: u64 = num_str.parse().unwrap_or(0);

        match unit {
            'K' | 'k' => num,
            'M' | 'm' => num * 1024,
            'G' | 'g' => num * 1024 * 1024,
            _ => num,
        }
    }

    fn read_uptime() -> Result<f64> {
        let uptime_str = fs::read_to_string("/proc/uptime")?;
        let uptime: f64 = uptime_str
            .split_whitespace()
            .next()
            .ok_or_else(|| IronError::Parse("Invalid uptime format".to_string()))?
            .parse()
            .map_err(|e| IronError::Parse(format!("Failed to parse uptime: {}", e)))?;
        Ok(uptime)
    }

    fn get_process_info(
        pid: u32,
        user: &str,
        name: &str,
        gpu_mem: u64,
        uptime: f64,
    ) -> Result<ProcessInfo> {
        let proc_path = format!("/proc/{}", pid);

        // Check if process still exists
        if !Path::new(&proc_path).exists() {
            return Err(IronError::DeviceNotFound(format!(
                "Process {} not found",
                pid
            )));
        }

        // Read /proc/[pid]/stat
        let stat_content = fs::read_to_string(format!("{}/stat", proc_path))?;
        let stat_parts: Vec<&str> = stat_content.split_whitespace().collect();

        if stat_parts.len() < 22 {
            return Err(IronError::Parse("Invalid stat format".to_string()));
        }

        // Extract fields (indices from man proc)
        let state = stat_parts[2].chars().next().unwrap_or('?');
        let utime: f64 = stat_parts[13].parse().unwrap_or(0.0);
        let stime: f64 = stat_parts[14].parse().unwrap_or(0.0);
        let priority: i32 = stat_parts[17].parse().unwrap_or(0);
        let starttime: f64 = stat_parts[21].parse().unwrap_or(0.0);

        // Calculate CPU percentage
        let clk_tck = 100.0; // SC_CLK_TCK, typically 100
        let total_time = (utime + stime) / clk_tck;
        let proc_uptime = (uptime - (starttime / clk_tck)).max(1.0);
        let cpu_percent = (100.0 * (total_time / proc_uptime)) as f32;

        // Read /proc/[pid]/statm for memory
        let statm_content = fs::read_to_string(format!("{}/statm", proc_path))?;
        let statm_parts: Vec<&str> = statm_content.split_whitespace().collect();

        let vm_rss = if statm_parts.len() > 1 {
            statm_parts[1].parse::<u64>().unwrap_or(0) * 4 // pages to KB
        } else {
            0
        };

        Ok(ProcessInfo {
            pid,
            user: user.to_string(),
            gpu: "I".to_string(), // Integrated GPU
            process_type: "Graphic".to_string(),
            priority,
            state,
            cpu_percent,
            memory_kb: vm_rss,
            gpu_memory_kb: gpu_mem,
            name: name.to_string(),
        })
    }
}
