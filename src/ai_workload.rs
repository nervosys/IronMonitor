//! AI Training and Inference Workload Monitoring
//!
//! This module provides real-time monitoring capabilities for AI/ML workloads including:
//! - Training metrics (epochs, steps, loss, accuracy)
//! - Inference metrics (throughput, latency, batch size)
//! - Multi-GPU/TPU distributed training tracking
//! - Cloud provider integration (AWS, Azure, GCP)
//! - Framework detection (PyTorch, TensorFlow, JAX, ONNX)
//!
//! # Examples
//!
//! ## Monitor Training Workload
//!
//! ```no_run
//! use ironmonlib::ai_workload::{AiWorkloadMonitor, WorkloadType};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut monitor = AiWorkloadMonitor::new()?;
//!
//! // Detect AI workloads
//! let workloads = monitor.detect_workloads()?;
//! for workload in &workloads {
//!     println!("Detected {} workload: PID {} - {}",
//!         workload.workload_type,
//!         workload.pid,
//!         workload.framework
//!     );
//!     
//!     if let Some(training) = &workload.training_metrics {
//!         println!("  Epoch: {}/{}", training.current_epoch, training.total_epochs);
//!         println!("  Loss: {:.4}", training.current_loss);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Monitor Inference Latency
//!
//! ```no_run
//! use ironmonlib::ai_workload::AiWorkloadMonitor;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut monitor = AiWorkloadMonitor::new()?;
//!
//! let workloads = monitor.detect_workloads()?;
//! for workload in workloads.iter().filter(|w| w.is_inference()) {
//!     if let Some(inference) = &workload.inference_metrics {
//!         println!("Inference workload PID {}:", workload.pid);
//!         println!("  Throughput: {:.2} samples/sec", inference.throughput);
//!         println!("  Latency: p50={:.2}ms, p95={:.2}ms, p99={:.2}ms",
//!             inference.latency_p50_ms,
//!             inference.latency_p95_ms,
//!             inference.latency_p99_ms
//!         );
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// AI/ML Framework
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiFramework {
    PyTorch,
    TensorFlow,
    JAX,
    ONNX,
    TensorRT,
    MXNet,
    Keras,
    Caffe,
    Unknown,
}

impl std::fmt::Display for AiFramework {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PyTorch => write!(f, "PyTorch"),
            Self::TensorFlow => write!(f, "TensorFlow"),
            Self::JAX => write!(f, "JAX"),
            Self::ONNX => write!(f, "ONNX"),
            Self::TensorRT => write!(f, "TensorRT"),
            Self::MXNet => write!(f, "MXNet"),
            Self::Keras => write!(f, "Keras"),
            Self::Caffe => write!(f, "Caffe"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Workload type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkloadType {
    Training,
    Inference,
    DataPreprocessing,
    FineTuning,
    Evaluation,
    Unknown,
}

impl std::fmt::Display for WorkloadType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Training => write!(f, "Training"),
            Self::Inference => write!(f, "Inference"),
            Self::DataPreprocessing => write!(f, "Data Preprocessing"),
            Self::FineTuning => write!(f, "Fine-tuning"),
            Self::Evaluation => write!(f, "Evaluation"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Training metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingMetrics {
    /// Current epoch number
    pub current_epoch: u32,
    /// Total epochs
    pub total_epochs: u32,
    /// Current step within epoch
    pub current_step: u32,
    /// Total steps per epoch
    pub steps_per_epoch: u32,
    /// Current training loss
    pub current_loss: f64,
    /// Current validation loss (if available)
    pub validation_loss: Option<f64>,
    /// Training accuracy (0-1)
    pub training_accuracy: Option<f64>,
    /// Validation accuracy (0-1)
    pub validation_accuracy: Option<f64>,
    /// Learning rate
    pub learning_rate: Option<f64>,
    /// Gradient norm
    pub gradient_norm: Option<f64>,
    /// Last checkpoint path
    pub last_checkpoint: Option<PathBuf>,
    /// Estimated time remaining in seconds
    pub eta_seconds: Option<u64>,
}

/// Inference metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceMetrics {
    /// Samples processed per second
    pub throughput: f64,
    /// Batch size
    pub batch_size: u32,
    /// P50 latency in milliseconds
    pub latency_p50_ms: f64,
    /// P95 latency in milliseconds
    pub latency_p95_ms: f64,
    /// P99 latency in milliseconds
    pub latency_p99_ms: f64,
    /// Average latency in milliseconds
    pub latency_avg_ms: f64,
    /// Total samples processed
    pub total_samples: u64,
    /// Model name/path
    pub model_name: Option<String>,
}

/// Distributed training configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedConfig {
    /// World size (total number of processes)
    pub world_size: u32,
    /// Rank of this process
    pub rank: u32,
    /// Local rank (GPU index on this node)
    pub local_rank: u32,
    /// Backend (nccl, gloo, mpi)
    pub backend: String,
    /// Master address
    pub master_addr: Option<String>,
    /// Master port
    pub master_port: Option<u16>,
}

/// TPU configuration
///
/// Every field the environment may omit is an `Option`. `num_cores` used to fall
/// back to `8` — a v2-8's core count, so a pod whose `TPU_NUM_CORES` was unset
/// was described as a specific, entirely plausible accelerator. `tpu_type` and
/// `topology` fell back to the string `"unknown"`, which `resolve.rs` rejects as
/// a value precisely because it reads as one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpuConfig {
    /// TPU type (v2, v3, v4, v5), or `None` when `TPU_TYPE` was not set.
    pub tpu_type: Option<String>,
    /// Number of TPU cores, or `None` when `TPU_NUM_CORES` was not set or did
    /// not parse.
    pub num_cores: Option<u32>,
    /// TPU topology (e.g., "2x2" for v2-8, "4x4" for v3-32), or `None` when
    /// `TPU_TOPOLOGY` was not set.
    pub topology: Option<String>,
    /// TPU zone (for cloud TPUs)
    pub zone: Option<String>,
    /// TPU project (for cloud TPUs)
    pub project: Option<String>,
}

/// Cloud provider
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudProvider {
    AWS,
    Azure,
    GCP,
    OnPremise,
    Unknown,
}

/// AI workload information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiWorkload {
    /// Process ID
    pub pid: u32,
    /// Process name
    pub name: String,
    /// Command line
    pub cmdline: String,
    /// Framework being used
    pub framework: AiFramework,
    /// Workload type
    pub workload_type: WorkloadType,
    /// Training metrics (if applicable)
    pub training_metrics: Option<TrainingMetrics>,
    /// Inference metrics (if applicable)
    pub inference_metrics: Option<InferenceMetrics>,
    /// GPU indices used
    pub gpu_indices: Vec<usize>,
    /// TPU configuration (if applicable)
    pub tpu_config: Option<TpuConfig>,
    /// Distributed configuration
    pub distributed_config: Option<DistributedConfig>,
    /// Cloud provider
    pub cloud_provider: CloudProvider,
    /// Environment variables relevant to AI frameworks
    pub env_vars: HashMap<String, String>,
    /// Start time
    pub start_time: std::time::SystemTime,
}

impl AiWorkload {
    /// Check if this is a training workload
    pub fn is_training(&self) -> bool {
        matches!(
            self.workload_type,
            WorkloadType::Training | WorkloadType::FineTuning
        )
    }

    /// Check if this is an inference workload
    pub fn is_inference(&self) -> bool {
        self.workload_type == WorkloadType::Inference
    }

    /// Get progress percentage for training
    pub fn training_progress_percent(&self) -> Option<f32> {
        self.training_metrics.as_ref().map(|m| {
            if m.total_epochs == 0 {
                0.0
            } else {
                let epoch_progress =
                    m.current_epoch as f32 + (m.current_step as f32 / m.steps_per_epoch as f32);
                (epoch_progress / m.total_epochs as f32 * 100.0).min(100.0)
            }
        })
    }
}

/// AI workload monitor
pub struct AiWorkloadMonitor {
    /// Last detected workloads
    workloads: Vec<AiWorkload>,
    /// Update interval in seconds
    update_interval: u64,
    /// Last update time
    last_update: std::time::Instant,
}

impl AiWorkloadMonitor {
    /// Create a new AI workload monitor
    pub fn new() -> Result<Self> {
        Ok(Self {
            workloads: Vec::new(),
            update_interval: 5, // Default 5 second update
            last_update: std::time::Instant::now(),
        })
    }

    /// Create with custom update interval
    pub fn with_update_interval(interval_secs: u64) -> Result<Self> {
        Ok(Self {
            workloads: Vec::new(),
            update_interval: interval_secs,
            last_update: std::time::Instant::now(),
        })
    }

    /// Detect AI workloads running on the system
    pub fn detect_workloads(&mut self) -> Result<Vec<AiWorkload>> {
        // Check if we need to update
        if self.last_update.elapsed().as_secs() < self.update_interval {
            return Ok(self.workloads.clone());
        }

        self.workloads.clear();
        self.last_update = std::time::Instant::now();

        // Platform-specific detection
        #[cfg(target_os = "linux")]
        {
            self.detect_linux()?;
        }

        #[cfg(target_os = "windows")]
        {
            self.detect_windows()?;
        }

        #[cfg(target_os = "macos")]
        {
            self.detect_macos()?;
        }

        Ok(self.workloads.clone())
    }

    /// Get current workloads without refresh
    pub fn workloads(&self) -> &[AiWorkload] {
        &self.workloads
    }

    /// Set update interval
    pub fn set_update_interval(&mut self, interval_secs: u64) {
        self.update_interval = interval_secs;
    }

    #[cfg(target_os = "linux")]
    fn detect_linux(&mut self) -> Result<()> {
        use std::fs;

        // Scan /proc for Python/AI processes
        let proc_entries = fs::read_dir("/proc").map_err(crate::error::IronError::Io)?;

        for entry in proc_entries.filter_map(|e| e.ok()) {
            if let Ok(file_name) = entry.file_name().into_string() {
                if let Ok(pid) = file_name.parse::<u32>() {
                    if let Some(workload) = self.analyze_process_linux(pid)? {
                        self.workloads.push(workload);
                    }
                }
            }
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn analyze_process_linux(&self, pid: u32) -> Result<Option<AiWorkload>> {
        use std::fs;

        // Read command line
        let cmdline_path = format!("/proc/{}/cmdline", pid);
        let cmdline = fs::read_to_string(&cmdline_path)
            .ok()
            .map(|s| s.replace('\0', " ").trim().to_string())
            .unwrap_or_default();

        // Check if this looks like an AI workload
        let framework = self.detect_framework(&cmdline);
        if framework == AiFramework::Unknown {
            return Ok(None);
        }

        // Read process name
        let comm_path = format!("/proc/{}/comm", pid);
        let name = fs::read_to_string(&comm_path)
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| format!("pid_{}", pid));

        // Detect workload type
        let workload_type = self.detect_workload_type(&cmdline);

        // Read environment variables
        let environ_path = format!("/proc/{}/environ", pid);
        let env_vars = fs::read_to_string(&environ_path)
            .ok()
            .map(|s| self.parse_environ(&s))
            .unwrap_or_default();

        // Check for distributed training
        let distributed_config = self.detect_distributed_config(&env_vars);

        // Check for TPU
        let tpu_config = self.detect_tpu_config(&env_vars);

        // Detect cloud provider
        let cloud_provider = self.detect_cloud_provider(&env_vars);

        // Try to parse training metrics from logs or environment
        let training_metrics = if workload_type == WorkloadType::Training {
            self.try_parse_training_metrics(pid, &env_vars)?
        } else {
            None
        };

        // Try to parse inference metrics
        let inference_metrics = if workload_type == WorkloadType::Inference {
            self.try_parse_inference_metrics(pid)?
        } else {
            None
        };

        Ok(Some(AiWorkload {
            pid,
            name,
            cmdline,
            framework,
            workload_type,
            training_metrics,
            inference_metrics,
            gpu_indices: self.detect_gpu_usage(pid)?,
            tpu_config,
            distributed_config,
            cloud_provider,
            env_vars,
            start_time: std::time::SystemTime::now(), // Approximation
        }))
    }

    #[cfg(target_os = "windows")]
    fn detect_windows(&mut self) -> Result<()> {
        use crate::error::IronError;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };

        // Take a snapshot of all processes
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .map_err(|e| IronError::System(format!("Failed to create snapshot: {}", e)))?;

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        // Get first process
        if unsafe { Process32FirstW(snapshot, &mut entry) }.is_err() {
            unsafe { CloseHandle(snapshot).ok() };
            return Ok(());
        }

        loop {
            let exe_name: String = entry
                .szExeFile
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8 as char)
                .collect();

            let exe_lower = exe_name.to_lowercase();
            let pid = entry.th32ProcessID;

            // Check if this is a Python process or known AI runtime
            let is_ai_candidate = exe_lower.contains("python")
                || exe_lower.contains("pytorch")
                || exe_lower.contains("tensorflow")
                || exe_lower.contains("ollama")
                || exe_lower.contains("llama")
                || exe_lower.contains("triton")
                || exe_lower.contains("vllm")
                || exe_lower.contains("onnx")
                || exe_lower.contains("tensorrt")
                || exe_lower.contains("nvinfer")
                || exe_lower.contains("cuda");

            if is_ai_candidate {
                // Try to get full command line (requires opening process)
                if let Ok(Some(w)) = self.analyze_process_windows(pid, &exe_name) {
                    self.workloads.push(w);
                }
            }

            // Get next process
            if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                break;
            }
        }

        unsafe { CloseHandle(snapshot).ok() };
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn analyze_process_windows(&self, pid: u32, exe_name: &str) -> Result<Option<AiWorkload>> {
        // Try to detect framework from executable name
        let exe_lower = exe_name.to_lowercase();

        // Determine framework
        let framework = if exe_lower.contains("python") {
            // Could be any Python-based framework, mark as unknown and use GPU detection
            AiFramework::Unknown
        } else if exe_lower.contains("ollama") {
            AiFramework::ONNX // Ollama uses GGML/ONNX internally
        } else if exe_lower.contains("triton")
            || exe_lower.contains("nvinfer")
            || exe_lower.contains("tensorrt")
        {
            AiFramework::TensorRT
        } else if exe_lower.contains("onnx") {
            AiFramework::ONNX
        } else {
            AiFramework::Unknown
        };

        // Check if this process is using GPU via NVML
        let gpu_indices = self.detect_gpu_usage(pid)?;

        // If no GPU usage and unknown framework, skip
        if gpu_indices.is_empty() && framework == AiFramework::Unknown {
            return Ok(None);
        }

        // Determine workload type
        let workload_type = if exe_lower.contains("train") {
            WorkloadType::Training
        } else if exe_lower.contains("serve") || exe_lower.contains("server") {
            WorkloadType::Inference
        } else if !gpu_indices.is_empty() {
            // Has GPU, likely inference or training
            WorkloadType::Inference
        } else {
            WorkloadType::Unknown
        };

        Ok(Some(AiWorkload {
            pid,
            name: exe_name.to_string(),
            cmdline: exe_name.to_string(), // Full cmdline requires more complex WMI query
            // Previously promoted an unidentified framework to PyTorch whenever
            // the process held a GPU. A Python process on a GPU is as likely to
            // be JAX, TensorFlow, ONNX Runtime or vLLM -- or a CUDA program that
            // is not machine learning at all -- and the guess reached the user
            // through `tuning::serve`, which reports `ai_frameworks` beside the
            // evidence string "an AI framework was identified in a running
            // process". Identified is exactly what it was not.
            //
            // `ai_present` does not depend on this and is unaffected: holding a
            // GPU is still evidence of an AI workload. Only the claim about
            // *which* framework is withdrawn.
            framework,
            workload_type,
            training_metrics: None,
            inference_metrics: None,
            gpu_indices,
            tpu_config: None,
            distributed_config: None,
            cloud_provider: CloudProvider::Unknown,
            env_vars: HashMap::new(),
            start_time: std::time::SystemTime::now(),
        }))
    }

    #[cfg(target_os = "macos")]
    fn detect_macos(&mut self) -> Result<()> {
        use crate::error::IronError;
        use std::process::Command;

        // Use ps to list all processes with full command lines
        let output = Command::new("ps")
            .args(["-axo", "pid,command"])
            .output()
            .map_err(|e| IronError::System(format!("Failed to run ps: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // AI-related keywords to look for
        let ai_keywords = [
            "python",
            "ollama",
            "mlx",
            "coreml",
            "pytorch",
            "tensorflow",
            "torch",
            "transformers",
            "llama",
            "llama.cpp",
            "whisper",
            "stable-diffusion",
            "diffusion",
            "comfyui",
            "automatic1111",
            "vllm",
        ];

        for line in stdout.lines().skip(1) {
            // Skip header
            let parts: Vec<&str> = line.trim().splitn(2, ' ').collect();
            if parts.len() < 2 {
                continue;
            }

            let pid: u32 = match parts[0].trim().parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let cmdline = parts[1].to_lowercase();

            // Check if this looks like an AI workload
            let is_ai_process = ai_keywords.iter().any(|kw| cmdline.contains(kw));
            if !is_ai_process {
                continue;
            }

            // Analyze the process
            if let Some(workload) = self.analyze_process_macos(pid, &cmdline)? {
                self.workloads.push(workload);
            }
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn analyze_process_macos(&self, pid: u32, cmdline: &str) -> Result<Option<AiWorkload>> {
        use std::process::Command;

        let process_name = cmdline
            .split('/')
            .next_back()
            .unwrap_or("unknown")
            .split_whitespace()
            .next()
            .unwrap_or("unknown")
            .to_string();

        // Skip common non-AI processes
        if matches!(
            process_name.as_str(),
            "grep" | "ps" | "sh" | "bash" | "zsh" | "cat" | "head" | "tail"
        ) {
            return Ok(None);
        }

        // Get memory usage via ps
        let mem_output = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "rss="])
            .output()
            .ok();

        let memory_mb = mem_output
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .unwrap_or(0)
            / 1024; // Convert KB to MB

        let framework = self.detect_framework(cmdline);
        let workload_type = self.detect_workload_type(cmdline);

        // This function was written against an older `AiWorkload` — it set
        // process_name, gpu_utilization, gpu_memory_mb, system_memory_mb,
        // cpu_percent, model_name, and an `accelerator` of a `HardwareAccelerator`
        // type that does not exist, and reached for a `CloudProvider::HuggingFace`
        // variant that does not either. Being macOS-only it had never been
        // compiled, so none of that was ever caught. It now builds the struct the
        // rest of the module uses.
        //
        // `memory_mb` is still read above because `ps` is already being run, but
        // the struct has no field for resident memory; the process monitor owns
        // that. Dropping the read entirely would remove a working measurement in
        // case this grows a home for it, so it is bound and explicitly unused.
        let _ = memory_mb;

        let cloud_provider = if cmdline.contains("ollama") {
            CloudProvider::OnPremise
        } else {
            CloudProvider::Unknown
        };

        Ok(Some(AiWorkload {
            pid,
            name: process_name,
            cmdline: cmdline.to_string(),
            framework,
            workload_type,
            // `ps` gives no training or inference telemetry, and there is no Metal
            // performance API call here to supply GPU figures.
            training_metrics: None,
            inference_metrics: None,
            gpu_indices: Vec::new(),
            tpu_config: None,
            distributed_config: None,
            cloud_provider,
            env_vars: HashMap::new(),
            start_time: std::time::SystemTime::now(),
        }))
    }

    #[allow(dead_code)]
    fn detect_framework(&self, cmdline: &str) -> AiFramework {
        if cmdline.contains("torch") || cmdline.contains("pytorch") {
            AiFramework::PyTorch
        } else if cmdline.contains("tensorflow") || cmdline.contains("tf.") {
            AiFramework::TensorFlow
        } else if cmdline.contains("jax") {
            AiFramework::JAX
        } else if cmdline.contains("onnx") {
            AiFramework::ONNX
        } else if cmdline.contains("tensorrt") || cmdline.contains("trt") {
            AiFramework::TensorRT
        } else if cmdline.contains("mxnet") {
            AiFramework::MXNet
        } else if cmdline.contains("keras") {
            AiFramework::Keras
        } else if cmdline.contains("caffe") {
            AiFramework::Caffe
        } else {
            AiFramework::Unknown
        }
    }

    #[allow(dead_code)]
    fn detect_workload_type(&self, cmdline: &str) -> WorkloadType {
        if cmdline.contains("train") || cmdline.contains("fit") {
            WorkloadType::Training
        } else if cmdline.contains("infer")
            || cmdline.contains("predict")
            || cmdline.contains("serve")
        {
            WorkloadType::Inference
        } else if cmdline.contains("fine-tune") || cmdline.contains("finetune") {
            WorkloadType::FineTuning
        } else if cmdline.contains("eval") || cmdline.contains("test") {
            WorkloadType::Evaluation
        } else if cmdline.contains("preprocess") || cmdline.contains("transform") {
            WorkloadType::DataPreprocessing
        } else {
            WorkloadType::Unknown
        }
    }

    #[allow(dead_code)]
    fn parse_environ(&self, environ: &str) -> HashMap<String, String> {
        environ
            .split('\0')
            .filter_map(|s| {
                let parts: Vec<&str> = s.splitn(2, '=').collect();
                if parts.len() == 2 {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect()
    }

    #[allow(dead_code)]
    fn detect_distributed_config(
        &self,
        env_vars: &HashMap<String, String>,
    ) -> Option<DistributedConfig> {
        // PyTorch distributed
        if let (Some(world_size), Some(rank)) = (
            env_vars.get("WORLD_SIZE").and_then(|s| s.parse().ok()),
            env_vars.get("RANK").and_then(|s| s.parse().ok()),
        ) {
            return Some(DistributedConfig {
                world_size,
                rank,
                local_rank: env_vars
                    .get("LOCAL_RANK")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
                backend: env_vars
                    .get("BACKEND")
                    .or_else(|| env_vars.get("NCCL_BACKEND"))
                    .cloned()
                    .unwrap_or_else(|| "nccl".to_string()),
                master_addr: env_vars.get("MASTER_ADDR").cloned(),
                master_port: env_vars.get("MASTER_PORT").and_then(|s| s.parse().ok()),
            });
        }

        // TensorFlow distributed
        if let Some(_tf_config) = env_vars.get("TF_CONFIG") {
            // TF_CONFIG is JSON, would need to parse
            // For now, just detect presence
            return Some(DistributedConfig {
                world_size: 1,
                rank: 0,
                local_rank: 0,
                backend: "tensorflow".to_string(),
                master_addr: None,
                master_port: None,
            });
        }

        None
    }

    #[allow(dead_code)]
    fn detect_tpu_config(&self, env_vars: &HashMap<String, String>) -> Option<TpuConfig> {
        // Check for TPU environment variables
        env_vars
            .get("TPU_NAME")
            .or_else(|| env_vars.get("TPU_WORKER_NAME"))
            .map(|_tpu_name| TpuConfig {
                tpu_type: env_vars.get("TPU_TYPE").cloned(),
                num_cores: env_vars.get("TPU_NUM_CORES").and_then(|s| s.parse().ok()),
                topology: env_vars.get("TPU_TOPOLOGY").cloned(),
                zone: env_vars
                    .get("TPU_ZONE")
                    .or_else(|| env_vars.get("GCP_ZONE"))
                    .cloned(),
                project: env_vars.get("GCP_PROJECT").cloned(),
            })
    }

    #[allow(dead_code)]
    fn detect_cloud_provider(&self, env_vars: &HashMap<String, String>) -> CloudProvider {
        if env_vars.contains_key("AWS_REGION") || env_vars.contains_key("AWS_DEFAULT_REGION") {
            CloudProvider::AWS
        } else if env_vars.contains_key("AZURE_SUBSCRIPTION_ID") {
            CloudProvider::Azure
        } else if env_vars.contains_key("GCP_PROJECT")
            || env_vars.contains_key("GOOGLE_CLOUD_PROJECT")
        {
            CloudProvider::GCP
        } else if env_vars.contains_key("K8S_POD_NAME") {
            // Could be any cloud, but at least we know it's containerized
            CloudProvider::Unknown
        } else {
            CloudProvider::OnPremise
        }
    }

    #[allow(dead_code)]
    fn detect_gpu_usage(&self, _pid: u32) -> Result<Vec<usize>> {
        // Check which GPUs this process is using
        // This would ideally query GPU drivers, but for now we can check CUDA_VISIBLE_DEVICES
        #[cfg(target_os = "linux")]
        {
            use std::fs;
            let environ_path = format!("/proc/{}/environ", _pid);
            if let Ok(environ) = fs::read_to_string(&environ_path) {
                let env_map = self.parse_environ(&environ);
                if let Some(cuda_devices) = env_map.get("CUDA_VISIBLE_DEVICES") {
                    return Ok(cuda_devices
                        .split(',')
                        .filter_map(|s| s.trim().parse().ok())
                        .collect());
                }
            }
        }

        Ok(Vec::new())
    }

    #[allow(dead_code)]
    fn try_parse_training_metrics(
        &self,
        _pid: u32,
        _env_vars: &HashMap<String, String>,
    ) -> Result<Option<TrainingMetrics>> {
        // Try to find tensorboard logs, checkpoints, or other indicators
        // This is a simplified version - real implementation would need to:
        // 1. Check for TensorBoard event files
        // 2. Parse checkpoint files
        // 3. Monitor log files
        // 4. Use framework-specific APIs if available

        // For now, return None - would need more sophisticated monitoring
        Ok(None)
    }

    #[allow(dead_code)]
    fn try_parse_inference_metrics(&self, _pid: u32) -> Result<Option<InferenceMetrics>> {
        // Try to detect inference metrics
        // Would need to monitor network traffic, log files, or use framework APIs
        Ok(None)
    }
}

impl Default for AiWorkloadMonitor {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            workloads: Vec::new(),
            update_interval: 5,
            last_update: std::time::Instant::now(),
        })
    }
}

#[cfg(test)]
mod tpu_absence_tests {
    use super::*;

    fn detector() -> AiWorkloadMonitor {
        AiWorkloadMonitor::default()
    }

    #[test]
    fn a_tpu_with_no_core_count_in_the_environment_reports_none() {
        // `TPU_NAME` is what makes this a TPU host; the rest may be unset.
        let mut env = HashMap::new();
        env.insert("TPU_NAME".to_string(), "node-1".to_string());

        let cfg = detector()
            .detect_tpu_config(&env)
            .expect("TPU_NAME identifies a TPU host");
        assert_eq!(
            cfg.num_cores, None,
            "an unset TPU_NUM_CORES must not read as a v2-8"
        );
        assert_eq!(cfg.tpu_type, None);
        assert_eq!(cfg.topology, None);
    }

    #[test]
    fn a_tpu_that_declares_itself_is_reported_as_declared() {
        let mut env = HashMap::new();
        env.insert("TPU_NAME".to_string(), "node-1".to_string());
        env.insert("TPU_NUM_CORES".to_string(), "32".to_string());
        env.insert("TPU_TYPE".to_string(), "v3".to_string());
        env.insert("TPU_TOPOLOGY".to_string(), "4x4".to_string());

        let cfg = detector().detect_tpu_config(&env).unwrap();
        assert_eq!(cfg.num_cores, Some(32));
        assert_eq!(cfg.tpu_type.as_deref(), Some("v3"));
        assert_eq!(cfg.topology.as_deref(), Some("4x4"));
    }

    #[test]
    fn an_unparseable_core_count_is_absent_rather_than_defaulted() {
        let mut env = HashMap::new();
        env.insert("TPU_NAME".to_string(), "node-1".to_string());
        env.insert("TPU_NUM_CORES".to_string(), "many".to_string());

        assert_eq!(detector().detect_tpu_config(&env).unwrap().num_cores, None);
    }

    #[test]
    fn a_host_with_no_tpu_name_is_not_a_tpu_host() {
        assert!(detector().detect_tpu_config(&HashMap::new()).is_none());
    }
}
