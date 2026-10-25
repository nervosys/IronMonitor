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
    /// Process type (Graphic, System, etc.), or `None` where it was not read.
    pub process_type: Option<String>,
    /// Priority
    pub priority: i32,
    /// Process state (R, S, D, Z, T, etc.), or `None` where it was not read.
    /// Windows used to report `R` for any process with a thread, which was
    /// every process.
    pub state: Option<char>,
    /// CPU usage percentage, `None` where it was not read. Windows used to
    /// report `0.0` when the process times could not be queried.
    pub cpu_percent: Option<f32>,
    /// Memory used in KB
    pub memory_kb: u64,
    /// GPU memory used in KB, `None` where it was not read. Windows never
    /// filled this in; it was `0` and read as "no GPU memory".
    pub gpu_memory_kb: Option<u64>,
    /// Process name
    pub name: String,
}

/// Process statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessStats {
    /// GPU processes
    pub processes: Vec<ProcessInfo>,
    /// Total GPU memory used by all processes, `None` where the platform reader
    /// did not measure it. `0` here used to mean both "no GPU memory" and "not
    /// read".
    pub total_gpu_memory_kb: Option<u64>,
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
            total_gpu_memory_kb: None,
        }
    }

    /// Get process count
    pub fn process_count(&self) -> usize {
        self.processes.len()
    }

    /// Get processes sorted by GPU memory usage
    pub fn sorted_by_gpu_memory(&self) -> Vec<&ProcessInfo> {
        let mut procs: Vec<&ProcessInfo> = self.processes.iter().collect();
        // `None` (not read) is the smallest `Option`, so under `Reverse` it ranks
        // after every measured figure.
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
        // `None` compares below every value, so unread CPU ranks last.
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
            // Not a Jetson or nvmap not available. Nothing was read, so the
            // total stays `None`: it used to be `0`, which is a measured "no GPU
            // memory in use".
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

    /// The total is `None` when its line is absent or unparseable. A client
    /// whose size does not parse is left out rather than reported as holding
    /// zero bytes of GPU memory.
    fn parse_nvmap_table(path: &str) -> Result<(Option<u64>, Vec<NvmapRow>)> {
        let content = fs::read_to_string(path)?;
        let mut processes = Vec::new();
        let mut total = None;

        for line in content.lines() {
            // Parse lines like: "user process PID sizeU"
            let parts: Vec<&str> = line.split_whitespace().collect();

            // The total line is checked before the field count: it may carry
            // only the size, and the count check used to drop it.
            if parts.first() == Some(&"total") {
                if let Some(size_str) = parts.get(1) {
                    total = parse_size_with_unit(size_str);
                }
                continue;
            }

            if parts.len() >= 4 {
                // Parse process line
                if let Ok(pid) = parts[2].parse::<u32>() {
                    if let Some(size) = parse_size_with_unit(parts[3]) {
                        let user = parts[0].to_string();
                        let name = parts[1].to_string();
                        processes.push((pid, user, name, size));
                    }
                }
            }
        }

        Ok((total, processes))
    }

    /// Parse a size such as `1234K`, `5M` or `1G`. `None` when the number does
    /// not parse. It used to be `0`, which read as a client holding no memory.
    fn parse_size_with_unit(size_str: &str) -> Option<u64> {
        // Security: Safely get last character without unwrap
        let last_char = size_str.chars().last()?;

        let (num_str, unit) = if last_char.is_alphabetic() {
            let last_idx = size_str.len() - last_char.len_utf8();
            (&size_str[..last_idx], last_char)
        } else {
            (size_str, 'K')
        };

        let num: u64 = num_str.parse().ok()?;

        Some(match unit {
            'K' | 'k' => num,
            'M' | 'm' => num * 1024,
            'G' | 'g' => num * 1024 * 1024,
            _ => num,
        })
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

    /// Parse one numeric field of `/proc/<pid>` as read. Errors rather than
    /// returning a default: a caller would otherwise report 0 for a value it
    /// never obtained.
    fn parse_proc_field<T: std::str::FromStr>(text: &str, field: &str, pid: u32) -> Result<T> {
        text.parse().map_err(|_| {
            IronError::Parse(format!(
                "/proc/{}: {} is not a number: {:?}",
                pid, field, text
            ))
        })
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

        // Extract fields (indices from man proc). A field that does not parse is
        // an error: a zero CPU time would read as an idle process.
        let state = stat_parts[2].chars().next().unwrap_or('?');
        let utime: f64 = parse_proc_field(stat_parts[13], "utime", pid)?;
        let stime: f64 = parse_proc_field(stat_parts[14], "stime", pid)?;
        let priority: i32 = parse_proc_field(stat_parts[17], "priority", pid)?;
        let starttime: f64 = parse_proc_field(stat_parts[21], "starttime", pid)?;

        // Calculate CPU percentage
        let clk_tck = crate::platform::linux::clock_ticks_per_second()
            .ok_or_else(|| IronError::Other("sysconf(_SC_CLK_TCK) returned no tick rate".into()))?
            as f64;
        let total_time = (utime + stime) / clk_tck;
        let proc_uptime = (uptime - (starttime / clk_tck)).max(1.0);
        let cpu_percent = Some((100.0 * (total_time / proc_uptime)) as f32);

        // Read /proc/[pid]/statm for memory
        let statm_content = fs::read_to_string(format!("{}/statm", proc_path))?;
        let statm_parts: Vec<&str> = statm_content.split_whitespace().collect();

        // A missing or unparseable RSS is an error, not an empty process.
        let rss_pages: u64 = match statm_parts.get(1) {
            Some(&text) => parse_proc_field(text, "statm resident", pid)?,
            None => {
                return Err(IronError::Parse(format!(
                    "/proc/{}/statm has no resident field",
                    pid
                )))
            }
        };
        let vm_rss = rss_pages
            * crate::platform::linux::page_size().ok_or_else(|| {
                IronError::Other("sysconf(_SC_PAGESIZE) returned no page size".into())
            })?
            / 1024;

        Ok(ProcessInfo {
            pid,
            user: user.to_string(),
            gpu: "I".to_string(), // Integrated GPU
            // Not read. This was "Graphic" on every process, the same constant
            // the Windows reader used to report as "System".
            process_type: None,
            priority,
            state: Some(state),
            cpu_percent,
            memory_kb: vm_rss,
            gpu_memory_kb: Some(gpu_mem),
            name: name.to_string(),
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn nvmap_sizes_are_parsed_or_refused() {
            assert_eq!(parse_size_with_unit("1234K"), Some(1234));
            assert_eq!(parse_size_with_unit("5M"), Some(5 * 1024));
            assert_eq!(parse_size_with_unit("1G"), Some(1024 * 1024));
            assert_eq!(parse_size_with_unit("77"), Some(77));
            assert_eq!(parse_size_with_unit(""), None);
            assert_eq!(parse_size_with_unit("K"), None);
            assert_eq!(parse_size_with_unit("abcM"), None);
        }

        #[test]
        fn a_client_with_an_unreadable_size_is_left_out_not_zeroed() {
            let dir = std::env::temp_dir().join(format!(
                "ironmon-nvmap-{}-{}",
                std::process::id(),
                "unreadable-size"
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join("maps");
            let table = [
                "user name PID size",
                "total 2048K",
                "root Xorg 100 512K",
                "root broken 101 ?M",
            ]
            .join("\n");
            fs::write(&path, table).unwrap();
            let (total, rows) = parse_nvmap_table(path.to_str().unwrap()).unwrap();
            assert_eq!(total, Some(2048));
            assert_eq!(
                rows.len(),
                1,
                "the unparseable client must not appear as 0 KB"
            );
            assert_eq!(rows[0].0, 100);
            let _ = fs::remove_dir_all(dir);
        }
    }
}
