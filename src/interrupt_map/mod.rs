//! Interrupt topology and IRQ mapping monitor.
//!
//! Maps interrupts to CPUs, detects MSI/MSI-X, tracks per-CPU interrupt
//! counts, identifies interrupt affinity, and detects potential interrupt
//! storms or imbalances.
//!
//! ## Platform Support
//!
//! - **Linux**: `/proc/interrupts`, `/proc/irq/*/`, `/sys/bus/pci/devices/*/msi_irqs/`
//! - **Windows**: Performance counters for interrupt rates
//! - **macOS**: Not directly exposed (stub)

use serde::{Deserialize, Serialize};

use crate::error::IronError;

/// Interrupt delivery type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterruptType {
    /// Legacy pin-based interrupt (INTA#-INTD#).
    LegacyPin,
    /// Message Signaled Interrupt.
    MSI,
    /// Extended MSI (multiple vectors).
    MSIX,
    /// Inter-Processor Interrupt.
    IPI,
    /// Local APIC timer.
    LocalTimer,
    /// Non-Maskable Interrupt.
    NMI,
    /// Spurious interrupt.
    Spurious,
    /// Software interrupt / tasklet.
    Software,
    /// Platform-specific (e.g. ARM GIC).
    Platform,
    Unknown,
}

impl std::fmt::Display for InterruptType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LegacyPin => write!(f, "Legacy Pin"),
            Self::MSI => write!(f, "MSI"),
            Self::MSIX => write!(f, "MSI-X"),
            Self::IPI => write!(f, "IPI"),
            Self::LocalTimer => write!(f, "Local Timer"),
            Self::NMI => write!(f, "NMI"),
            Self::Spurious => write!(f, "Spurious"),
            Self::Software => write!(f, "Software"),
            Self::Platform => write!(f, "Platform"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// An interrupt source and its statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptInfo {
    /// IRQ number (or name for special interrupts like NMI, LOC).
    pub irq: String,
    /// Interrupt type.
    pub interrupt_type: InterruptType,
    /// Per-CPU interrupt counts.
    pub per_cpu_counts: Vec<u64>,
    /// Total count across all CPUs.
    pub total_count: u64,
    /// Chip/controller name (e.g. "IO-APIC", "PCI-MSI").
    pub chip: String,
    /// Hardware IRQ description (e.g. device name).
    pub description: String,
    /// CPU affinity mask (which CPUs handle this IRQ).
    pub affinity_cpus: Vec<u32>,
    /// Whether the affinity is balanced across CPUs.
    pub affinity_balanced: bool,
}

/// Interrupt imbalance analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptAnalysis {
    /// Total interrupts per second (estimated from delta if available).
    pub total_interrupts: u64,
    /// Per-CPU interrupt totals.
    pub per_cpu_totals: Vec<u64>,
    /// CPU with the highest interrupt count.
    pub busiest_cpu: u32,
    /// CPU with the lowest interrupt count.
    pub quietest_cpu: u32,
    /// Imbalance ratio (max/min).
    pub imbalance_ratio: f64,
    /// Whether there's a significant imbalance (ratio > 3.0).
    pub significant_imbalance: bool,
    /// Top interrupt sources by count.
    pub top_sources: Vec<(String, u64)>,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// Interrupt map monitor.
pub struct InterruptMapMonitor {
    /// Current interrupt data.
    interrupts: Vec<InterruptInfo>,
    /// Number of CPUs.
    cpu_count: u32,
    /// Analysis.
    analysis: InterruptAnalysis,
}

impl InterruptMapMonitor {
    /// Create a new interrupt map monitor.
    pub fn new() -> Result<Self, IronError> {
        let (interrupts, cpu_count) = Self::read_interrupts()?;
        let analysis = Self::analyze(&interrupts, cpu_count);
        Ok(Self {
            interrupts,
            cpu_count,
            analysis,
        })
    }

    /// Refresh data.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        let (interrupts, cpu_count) = Self::read_interrupts()?;
        self.analysis = Self::analyze(&interrupts, cpu_count);
        self.interrupts = interrupts;
        self.cpu_count = cpu_count;
        Ok(())
    }

    /// Get all interrupts.
    pub fn interrupts(&self) -> &[InterruptInfo] {
        &self.interrupts
    }

    /// Get analysis.
    pub fn analysis(&self) -> &InterruptAnalysis {
        &self.analysis
    }

    /// Get MSI/MSI-X interrupts only.
    pub fn msi_interrupts(&self) -> Vec<&InterruptInfo> {
        self.interrupts
            .iter()
            .filter(|i| {
                i.interrupt_type == InterruptType::MSI || i.interrupt_type == InterruptType::MSIX
            })
            .collect()
    }

    /// Get interrupts for a specific CPU.
    pub fn interrupts_for_cpu(&self, cpu: u32) -> Vec<(&InterruptInfo, u64)> {
        self.interrupts
            .iter()
            .filter_map(|i| {
                i.per_cpu_counts
                    .get(cpu as usize)
                    .filter(|&&count| count > 0)
                    .map(|&count| (i, count))
            })
            .collect()
    }

    fn analyze(interrupts: &[InterruptInfo], cpu_count: u32) -> InterruptAnalysis {
        let mut per_cpu_totals = vec![0u64; cpu_count as usize];

        for irq in interrupts {
            for (i, &count) in irq.per_cpu_counts.iter().enumerate() {
                if i < per_cpu_totals.len() {
                    per_cpu_totals[i] += count;
                }
            }
        }

        let total: u64 = per_cpu_totals.iter().sum();
        let max_cpu = per_cpu_totals
            .iter()
            .enumerate()
            .max_by_key(|(_, &v)| v)
            .map(|(i, _)| i as u32)
            .unwrap_or(0);
        let min_cpu = per_cpu_totals
            .iter()
            .enumerate()
            .filter(|(_, &v)| v > 0)
            .min_by_key(|(_, &v)| v)
            .map(|(i, _)| i as u32)
            .unwrap_or(0);

        let max_count = per_cpu_totals.iter().copied().max().unwrap_or(0);
        let min_count = per_cpu_totals
            .iter()
            .copied()
            .filter(|&v| v > 0)
            .min()
            .unwrap_or(1)
            .max(1);
        let imbalance = max_count as f64 / min_count as f64;

        // Top sources
        let mut sources: Vec<(String, u64)> = interrupts
            .iter()
            .filter(|i| i.total_count > 0)
            .map(|i| {
                let label = if i.description.is_empty() {
                    format!("IRQ {}", i.irq)
                } else {
                    format!("IRQ {} ({})", i.irq, i.description)
                };
                (label, i.total_count)
            })
            .collect();
        sources.sort_by_key(|p| std::cmp::Reverse(p.1));
        sources.truncate(10);

        let mut recommendations = Vec::new();
        if imbalance > 5.0 {
            recommendations.push(format!(
                "Severe interrupt imbalance: CPU {} handles {:.1}x more than CPU {}",
                max_cpu, imbalance, min_cpu
            ));
            recommendations
                .push("Consider using irqbalance or setting IRQ affinity manually".into());
        } else if imbalance > 3.0 {
            recommendations
                .push("Moderate interrupt imbalance detected; consider enabling irqbalance".into());
        }

        InterruptAnalysis {
            total_interrupts: total,
            per_cpu_totals,
            busiest_cpu: max_cpu,
            quietest_cpu: min_cpu,
            imbalance_ratio: imbalance,
            significant_imbalance: imbalance > 3.0,
            top_sources: sources,
            recommendations,
        }
    }

    #[cfg(target_os = "linux")]
    fn read_interrupts() -> Result<(Vec<InterruptInfo>, u32), IronError> {
        let content = std::fs::read_to_string("/proc/interrupts").map_err(IronError::Io)?;
        let mut lines = content.lines();

        // First line has CPU headers
        let header = lines.next().unwrap_or("");
        let cpu_count = header.split_whitespace().count() as u32;

        let mut interrupts = Vec::new();

        for line in lines {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < (cpu_count as usize + 1) {
                continue;
            }

            let irq = parts[0].trim_end_matches(':').to_string();

            // Parse per-CPU counts
            let per_cpu_counts: Vec<u64> = parts[1..=(cpu_count as usize)]
                .iter()
                .map(|s| s.parse::<u64>().unwrap_or(0))
                .collect();

            let total: u64 = per_cpu_counts.iter().sum();

            // Remaining parts: chip name and description
            let remaining = &parts[(cpu_count as usize + 1)..];
            let chip = remaining.first().unwrap_or(&"").to_string();
            let description = if remaining.len() > 1 {
                remaining[1..].join(" ")
            } else {
                String::new()
            };

            let interrupt_type = Self::classify_interrupt(&irq, &chip);

            // Try reading affinity
            let affinity_cpus = Self::read_affinity(&irq, cpu_count);
            let affinity_balanced = if affinity_cpus.len() > 1 {
                affinity_cpus.len() as u32 >= cpu_count / 2
            } else {
                false
            };

            interrupts.push(InterruptInfo {
                irq,
                interrupt_type,
                per_cpu_counts,
                total_count: total,
                chip,
                description,
                affinity_cpus,
                affinity_balanced,
            });
        }

        Ok((interrupts, cpu_count))
    }

    #[cfg(target_os = "linux")]
    fn classify_interrupt(irq: &str, chip: &str) -> InterruptType {
        let chip_lower = chip.to_lowercase();

        if chip_lower.contains("pci-msi") || chip_lower.contains("pci-msix") {
            if chip_lower.contains("msix") {
                InterruptType::MSIX
            } else {
                InterruptType::MSI
            }
        } else if irq == "NMI" || irq == "PMI" {
            InterruptType::NMI
        } else if irq == "LOC" {
            InterruptType::LocalTimer
        } else if irq == "SPU" {
            InterruptType::Spurious
        } else if irq == "RES" || irq == "CAL" || irq == "TLB" {
            InterruptType::IPI
        // An IO-APIC chip and a bare numeric IRQ are two different ways of spotting
        // the same thing, so they share an arm rather than repeating the result.
        } else if chip_lower.contains("io-apic")
            || chip_lower.contains("ioapic")
            || irq.parse::<u32>().is_ok()
        {
            InterruptType::LegacyPin
        } else {
            InterruptType::Unknown
        }
    }

    #[cfg(target_os = "linux")]
    fn read_affinity(irq: &str, cpu_count: u32) -> Vec<u32> {
        // Only for numeric IRQs
        if irq.parse::<u32>().is_err() {
            return (0..cpu_count).collect();
        }

        let affinity_path = format!("/proc/irq/{}/smp_affinity_list", irq);
        match std::fs::read_to_string(&affinity_path) {
            Ok(s) => Self::parse_cpu_list(s.trim(), cpu_count),
            Err(_) => (0..cpu_count).collect(),
        }
    }

    #[cfg(target_os = "linux")]
    fn parse_cpu_list(s: &str, _max: u32) -> Vec<u32> {
        let mut cpus = Vec::new();
        for part in s.split(',') {
            let part = part.trim();
            if let Some((start, end)) = part.split_once('-') {
                if let (Ok(s), Ok(e)) = (start.parse::<u32>(), end.parse::<u32>()) {
                    cpus.extend(s..=e);
                }
            } else if let Ok(c) = part.parse::<u32>() {
                cpus.push(c);
            }
        }
        cpus
    }

    #[cfg(target_os = "windows")]
    fn read_interrupts() -> Result<(Vec<InterruptInfo>, u32), IronError> {
        // An empty list plus a CPU count read as "this machine has no
        // interrupts", which is impossible. The error carries the reason.
        Err(IronError::UnsupportedPlatform(
            "interrupt affinity is read from `/proc/interrupts` and `/proc/irq`, which Windows does not expose"
                .into(),
        ))
    }

    #[cfg(target_os = "macos")]
    fn read_interrupts() -> Result<(Vec<InterruptInfo>, u32), IronError> {
        // An empty list plus a CPU count read as "this machine has no
        // interrupts", which is impossible. The error carries the reason.
        Err(IronError::UnsupportedPlatform(
            "interrupt affinity is read from `/proc/interrupts` and `/proc/irq`, which macOS does not expose"
                .into(),
        ))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn read_interrupts() -> Result<(Vec<InterruptInfo>, u32), IronError> {
        // An empty list plus a CPU count read as "this machine has no
        // interrupts", which is impossible. The error carries the reason.
        Err(IronError::UnsupportedPlatform(
            "interrupt affinity is read from `/proc/interrupts` and `/proc/irq`, which this platform does not expose"
                .into(),
        ))
    }
}

impl Default for InterruptMapMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            interrupts: Vec::new(),
            cpu_count: 1,
            analysis: InterruptAnalysis {
                total_interrupts: 0,
                per_cpu_totals: Vec::new(),
                busiest_cpu: 0,
                quietest_cpu: 0,
                imbalance_ratio: 1.0,
                significant_imbalance: false,
                top_sources: Vec::new(),
                recommendations: Vec::new(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interrupt_type_display() {
        assert_eq!(InterruptType::MSI.to_string(), "MSI");
        assert_eq!(InterruptType::MSIX.to_string(), "MSI-X");
        assert_eq!(InterruptType::NMI.to_string(), "NMI");
    }

    #[test]
    fn test_analysis_balanced() {
        let interrupts = vec![InterruptInfo {
            irq: "0".into(),
            interrupt_type: InterruptType::LegacyPin,
            per_cpu_counts: vec![1000, 1000, 1000, 1000],
            total_count: 4000,
            chip: "IO-APIC".into(),
            description: "timer".into(),
            affinity_cpus: vec![0, 1, 2, 3],
            affinity_balanced: true,
        }];
        let analysis = InterruptMapMonitor::analyze(&interrupts, 4);
        assert!(!analysis.significant_imbalance);
        assert!((analysis.imbalance_ratio - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_analysis_imbalanced() {
        let interrupts = vec![InterruptInfo {
            irq: "42".into(),
            interrupt_type: InterruptType::MSI,
            per_cpu_counts: vec![10000, 100, 100, 100],
            total_count: 10300,
            chip: "PCI-MSI".into(),
            description: "nvme0q0".into(),
            affinity_cpus: vec![0],
            affinity_balanced: false,
        }];
        let analysis = InterruptMapMonitor::analyze(&interrupts, 4);
        assert!(analysis.significant_imbalance);
        assert!(analysis.imbalance_ratio > 3.0);
        assert!(!analysis.recommendations.is_empty());
    }

    #[test]
    fn test_monitor_default() {
        let monitor = InterruptMapMonitor::default();
        let _analysis = monitor.analysis();
    }

    #[test]
    fn test_serialization() {
        let info = InterruptInfo {
            irq: "42".into(),
            interrupt_type: InterruptType::MSI,
            per_cpu_counts: vec![100, 200],
            total_count: 300,
            chip: "PCI-MSI".into(),
            description: "eth0".into(),
            affinity_cpus: vec![0, 1],
            affinity_balanced: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("PCI-MSI"));
        let _: InterruptInfo = serde_json::from_str(&json).unwrap();
    }
}
