//! Hardware codec and media capability detection — video encode/decode, GPU compute.
//!
//! # Platform Support
//!
//! - **Linux**: Reads `/sys/class/drm/card*/device/`, `vainfo`, `vdpauinfo`
//! - **Windows**: Uses WMI (`Win32_VideoController`), DXVA queries
//! - **macOS**: Uses `VTIsHardwareDecodeSupported`, system_profiler
//!
//! ## Inference
//!
//! When direct API queries aren't available, codec support is *inferred* from
//! GPU model name and generation using a built-in capability database.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonitor::codec::CodecMonitor;
//!
//! let monitor = CodecMonitor::new().unwrap();
//! for cap in monitor.capabilities() {
//!     println!("{}: {} {:?} (max {})",
//!         cap.device, cap.codec, cap.direction, cap.max_resolution);
//! }
//! ```

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// Video codec type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VideoCodec {
    H264,
    H265,
    H266,
    VP8,
    VP9,
    AV1,
    MPEG2,
    MPEG4,
    VC1,
    JPEG,
    ProRes,
    Other(String),
}

/// Codec direction — encode, decode, or both
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodecDirection {
    Decode,
    Encode,
    Both,
}

/// Maximum supported resolution
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MaxResolution {
    SD,     // 720x480
    HD,     // 1280x720
    FullHD, // 1920x1080
    QHD,    // 2560x1440
    UHD4K,  // 3840x2160
    UHD8K,  // 7680x4320
    Unknown,
}

/// Color depth / bit depth support
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BitDepth {
    Bit8,
    Bit10,
    Bit12,
    Unknown,
}

/// Source of the capability information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilitySource {
    /// Detected via hardware/driver API
    DirectQuery,
    /// Inferred from GPU model/generation
    Inferred,
    /// User-provided override
    Manual,
}

/// Fixed confidence for a row read from `vainfo`, a direct driver query. Not a probability.
#[cfg(target_os = "linux")]
const CONFIDENCE_VAINFO_QUERY: f32 = 1.0;
/// Fixed confidence for the NVIDIA base codec table, inferred from the GPU name.
const CONFIDENCE_NVIDIA_BASE_CODECS: f32 = 0.85;
/// Fixed confidence for AV1 encode and decode on RTX 40-series, inferred from the GPU name.
const CONFIDENCE_NVIDIA_AV1_RTX40: f32 = 0.9;
/// Fixed confidence for AV1 decode on RTX 30-series, inferred from the GPU name.
const CONFIDENCE_NVIDIA_AV1_RTX30: f32 = 0.85;
/// Fixed confidence for the AMD base codec table, inferred from the GPU name.
const CONFIDENCE_AMD_BASE_CODECS: f32 = 0.8;
/// Fixed confidence for AV1 on RDNA 3 (VCN 4.0), inferred from the GPU name.
const CONFIDENCE_AMD_AV1_RDNA3: f32 = 0.85;
/// Fixed confidence for AV1 decode on RDNA 2 (VCN 3.0), inferred from the GPU name.
const CONFIDENCE_AMD_AV1_RDNA2: f32 = 0.8;
/// Fixed confidence for the Intel base codec table, inferred from the GPU name.
const CONFIDENCE_INTEL_BASE_CODECS: f32 = 0.8;
/// Fixed confidence for AV1 on Intel Arc (Xe Media Engine), inferred from the GPU name.
const CONFIDENCE_INTEL_ARC_AV1: f32 = 0.85;
/// Fixed confidence for the Apple Media Engine codec table, inferred from the chip name.
const CONFIDENCE_APPLE_BASE_CODECS: f32 = 0.9;
/// Fixed confidence for AV1 decode on M3, M4 and M2 Pro/Max/Ultra, inferred from the chip name.
const CONFIDENCE_APPLE_AV1: f32 = 0.85;

/// A single codec capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecCapability {
    /// Device name (e.g., "NVIDIA RTX 4090", "Intel UHD 770")
    pub device: String,
    /// Codec type
    pub codec: VideoCodec,
    /// Direction (encode/decode/both)
    pub direction: CodecDirection,
    /// Maximum supported resolution
    pub max_resolution: MaxResolution,
    /// Maximum bit depth
    pub max_bit_depth: BitDepth,
    /// Maximum FPS at the maximum resolution.
    ///
    /// `None` on every path this crate has, and that is the honest value. The
    /// field held a literal `60` at all twelve construction sites while the
    /// ontology declared it `Derived` *from* `codec` and `max_resolution` --
    /// arithmetic that does not exist. 60 is also not a frame rate an 8K HEVC
    /// engine reaches, so the constant was not even a safe floor.
    pub max_fps: Option<u32>,
    /// Hardware engine name (e.g., "NVENC", "NVDEC", "VCE", "QuickSync")
    pub engine: String,
    /// How this capability was determined
    pub source: CapabilitySource,
    /// Confidence (0.0-1.0) that this row is right. Not a measured probability:
    /// it is the fixed constant of the detection rule that produced the row (one
    /// of the `CONFIDENCE_*` constants in this module), chosen by the code. Only a
    /// driver query carries 1.0. Rows are built only inside a rule that matched.
    pub confidence: f32,
}

/// Compute capability for GPGPU workloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeCapability {
    /// Device name
    pub device: String,
    /// CUDA compute capability (e.g., "8.9") — NVIDIA only
    pub cuda_version: String,
    /// OpenCL version supported
    pub opencl_version: String,
    /// Vulkan compute supported
    pub vulkan_compute: bool,
    /// Metal compute supported (Apple)
    pub metal_compute: bool,
    /// DirectCompute version (Windows)
    pub direct_compute: String,
    /// Estimated TFLOPS (FP32); `None` for a generation the table does not know
    pub estimated_tflops_fp32: Option<f32>,
    /// Estimated TFLOPS (FP16 / half precision); `None` when FP32 is unknown
    pub estimated_tflops_fp16: Option<f32>,
    /// Tensor core / AI accelerator present
    pub has_tensor_cores: bool,
    /// Ray tracing hardware acceleration
    pub has_ray_tracing: bool,
    /// Source of capability info
    pub source: CapabilitySource,
}

/// Monitor for codec and compute capabilities
pub struct CodecMonitor {
    codec_caps: Vec<CodecCapability>,
    compute_caps: Vec<ComputeCapability>,
}

impl CodecMonitor {
    pub fn new() -> Result<Self, IronError> {
        let mut monitor = Self {
            codec_caps: Vec::new(),
            compute_caps: Vec::new(),
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.codec_caps.clear();
        self.compute_caps.clear();

        // Detect GPU names for inference
        let gpu_names = Self::detect_gpu_names();

        // Try direct query first, then fall back to inference
        #[cfg(target_os = "linux")]
        self.refresh_linux();

        #[cfg(target_os = "windows")]
        self.refresh_windows();

        #[cfg(target_os = "macos")]
        self.refresh_macos();

        // Inference: if no capabilities detected directly, infer from GPU names
        if self.codec_caps.is_empty() {
            for name in &gpu_names {
                self.infer_from_gpu_name(name);
            }
        }

        // Always infer compute capabilities from GPU names
        for name in &gpu_names {
            self.infer_compute_from_gpu(name);
        }

        Ok(())
    }

    pub fn capabilities(&self) -> &[CodecCapability] {
        &self.codec_caps
    }

    pub fn compute_capabilities(&self) -> &[ComputeCapability] {
        &self.compute_caps
    }

    /// Check if a specific codec is supported for decode.
    pub fn can_decode(&self, codec: &VideoCodec) -> bool {
        self.codec_caps.iter().any(|c| {
            &c.codec == codec
                && matches!(c.direction, CodecDirection::Decode | CodecDirection::Both)
        })
    }

    /// Check if a specific codec is supported for encode.
    pub fn can_encode(&self, codec: &VideoCodec) -> bool {
        self.codec_caps.iter().any(|c| {
            &c.codec == codec
                && matches!(c.direction, CodecDirection::Encode | CodecDirection::Both)
        })
    }

    /// Get max decode resolution for a codec.
    pub fn max_decode_resolution(&self, codec: &VideoCodec) -> MaxResolution {
        self.codec_caps
            .iter()
            .filter(|c| {
                &c.codec == codec
                    && matches!(c.direction, CodecDirection::Decode | CodecDirection::Both)
            })
            .map(|c| c.max_resolution.clone())
            .max()
            .unwrap_or(MaxResolution::Unknown)
    }

    /// Get estimated TFLOPS (FP32) across all devices. `None` unless every
    /// device has a figure: a partial sum would read as the machine's total.
    pub fn total_tflops_fp32(&self) -> Option<f32> {
        if self.compute_caps.is_empty() {
            return None;
        }
        self.compute_caps
            .iter()
            .map(|c| c.estimated_tflops_fp32)
            .sum()
    }

    fn detect_gpu_names() -> Vec<String> {
        let mut names = Vec::new();

        #[cfg(target_os = "linux")]
        {
            // Read from /sys/class/drm/card*/device/
            if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("card") && !name.contains('-') {
                        let label = std::fs::read_to_string(entry.path().join("device/label"))
                            .or_else(|_| {
                                std::fs::read_to_string(entry.path().join("device/product_name"))
                            })
                            .unwrap_or_default()
                            .trim()
                            .to_string();
                        if !label.is_empty() {
                            names.push(label);
                        }
                    }
                }
            }

            // Also try lspci for GPU names
            if names.is_empty() {
                if let Ok(output) = std::process::Command::new("lspci")
                    .args(["-d", "::0300", "-nn"])
                    .output()
                {
                    let text = String::from_utf8(output.stdout).unwrap_or_default();
                    for line in text.lines() {
                        if let Some(pos) = line.find(": ") {
                            names.push(line[pos + 2..].to_string());
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Ok(output) = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    "Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name",
                ])
                .output()
            {
                let text = String::from_utf8(output.stdout).unwrap_or_default();
                for line in text.lines() {
                    let n = line.trim();
                    if !n.is_empty() {
                        names.push(n.to_string());
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            if let Ok(output) = std::process::Command::new("system_profiler")
                .args(["SPDisplaysDataType"])
                .output()
            {
                let text = String::from_utf8(output.stdout).unwrap_or_default();
                for line in text.lines() {
                    let line = line.trim();
                    if line.contains("Chipset Model:") {
                        if let Some(model) = line.split(':').nth(1) {
                            names.push(model.trim().to_string());
                        }
                    }
                }
            }
        }

        names
    }

    /// Infer codec capabilities from GPU model name.
    fn infer_from_gpu_name(&mut self, gpu_name: &str) {
        let lower = gpu_name.to_lowercase();

        // NVIDIA inference
        if lower.contains("nvidia")
            || lower.contains("geforce")
            || lower.contains("quadro")
            || lower.contains("tesla")
            || lower.contains("rtx")
            || lower.contains("gtx")
        {
            self.infer_nvidia_codecs(gpu_name, &lower);
        }

        // AMD inference
        if lower.contains("amd") || lower.contains("radeon") || lower.contains("rx ") {
            self.infer_amd_codecs(gpu_name, &lower);
        }

        // Intel inference
        if lower.contains("intel")
            || lower.contains("uhd")
            || lower.contains("iris")
            || lower.contains("arc")
        {
            self.infer_intel_codecs(gpu_name, &lower);
        }

        // Apple inference
        if lower.contains("apple")
            || lower.contains("m1")
            || lower.contains("m2")
            || lower.contains("m3")
            || lower.contains("m4")
        {
            self.infer_apple_codecs(gpu_name);
        }
    }

    fn infer_nvidia_codecs(&mut self, gpu_name: &str, lower: &str) {
        let is_rtx40 = lower.contains("40") && lower.contains("rtx");
        let is_rtx30 = lower.contains("30") && lower.contains("rtx");
        let is_rtx20 = lower.contains("20") && lower.contains("rtx");
        let is_gtx16 = lower.contains("16") && lower.contains("gtx");
        let is_gtx10 = lower.contains("10") && lower.contains("gtx");

        // All modern NVIDIA: H.264, H.265 decode/encode
        let codecs = vec![
            (
                VideoCodec::H264,
                CodecDirection::Both,
                MaxResolution::UHD4K,
                BitDepth::Bit8,
            ),
            (
                VideoCodec::H265,
                CodecDirection::Both,
                MaxResolution::UHD8K,
                BitDepth::Bit10,
            ),
            (
                VideoCodec::VP9,
                CodecDirection::Decode,
                MaxResolution::UHD8K,
                BitDepth::Bit10,
            ),
            (
                VideoCodec::MPEG2,
                CodecDirection::Decode,
                MaxResolution::FullHD,
                BitDepth::Bit8,
            ),
            (
                VideoCodec::VC1,
                CodecDirection::Decode,
                MaxResolution::FullHD,
                BitDepth::Bit8,
            ),
        ];

        for (codec, dir, res, depth) in codecs {
            self.codec_caps.push(CodecCapability {
                device: gpu_name.to_string(),
                codec,
                direction: dir,
                max_resolution: res,
                max_bit_depth: depth,
                max_fps: None,
                engine: "NVENC/NVDEC".into(),
                source: CapabilitySource::Inferred,
                confidence: CONFIDENCE_NVIDIA_BASE_CODECS,
            });
        }

        // AV1: RTX 40xx decode+encode, RTX 30xx decode only
        if is_rtx40 {
            self.codec_caps.push(CodecCapability {
                device: gpu_name.to_string(),
                codec: VideoCodec::AV1,
                direction: CodecDirection::Both,
                max_resolution: MaxResolution::UHD8K,
                max_bit_depth: BitDepth::Bit10,
                max_fps: None,
                engine: "NVENC/NVDEC".into(),
                source: CapabilitySource::Inferred,
                confidence: CONFIDENCE_NVIDIA_AV1_RTX40,
            });
        } else if is_rtx30 {
            self.codec_caps.push(CodecCapability {
                device: gpu_name.to_string(),
                codec: VideoCodec::AV1,
                direction: CodecDirection::Decode,
                max_resolution: MaxResolution::UHD8K,
                max_bit_depth: BitDepth::Bit10,
                max_fps: None,
                engine: "NVDEC".into(),
                source: CapabilitySource::Inferred,
                confidence: CONFIDENCE_NVIDIA_AV1_RTX30,
            });
        }

        let _ = (is_rtx20, is_gtx16, is_gtx10); // used for generation match above
    }

    fn infer_amd_codecs(&mut self, gpu_name: &str, lower: &str) {
        let is_rdna3 = lower.contains("7") && (lower.contains("rx ") || lower.contains("radeon"));
        let is_rdna2 = lower.contains("6") && (lower.contains("rx ") || lower.contains("radeon"));

        let codecs = vec![
            (
                VideoCodec::H264,
                CodecDirection::Both,
                MaxResolution::UHD4K,
                BitDepth::Bit8,
            ),
            (
                VideoCodec::H265,
                CodecDirection::Both,
                MaxResolution::UHD4K,
                BitDepth::Bit10,
            ),
            (
                VideoCodec::VP9,
                CodecDirection::Decode,
                MaxResolution::UHD4K,
                BitDepth::Bit10,
            ),
        ];

        for (codec, dir, res, depth) in codecs {
            self.codec_caps.push(CodecCapability {
                device: gpu_name.to_string(),
                codec,
                direction: dir,
                max_resolution: res,
                max_bit_depth: depth,
                max_fps: None,
                engine: "VCN".into(),
                source: CapabilitySource::Inferred,
                confidence: CONFIDENCE_AMD_BASE_CODECS,
            });
        }

        if is_rdna3 {
            self.codec_caps.push(CodecCapability {
                device: gpu_name.to_string(),
                codec: VideoCodec::AV1,
                direction: CodecDirection::Both,
                max_resolution: MaxResolution::UHD8K,
                max_bit_depth: BitDepth::Bit10,
                max_fps: None,
                engine: "VCN 4.0".into(),
                source: CapabilitySource::Inferred,
                confidence: CONFIDENCE_AMD_AV1_RDNA3,
            });
        } else if is_rdna2 {
            self.codec_caps.push(CodecCapability {
                device: gpu_name.to_string(),
                codec: VideoCodec::AV1,
                direction: CodecDirection::Decode,
                max_resolution: MaxResolution::UHD8K,
                max_bit_depth: BitDepth::Bit10,
                max_fps: None,
                engine: "VCN 3.0".into(),
                source: CapabilitySource::Inferred,
                confidence: CONFIDENCE_AMD_AV1_RDNA2,
            });
        }
    }

    fn infer_intel_codecs(&mut self, gpu_name: &str, lower: &str) {
        let is_arc = lower.contains("arc");

        let codecs = vec![
            (
                VideoCodec::H264,
                CodecDirection::Both,
                MaxResolution::UHD4K,
                BitDepth::Bit8,
            ),
            (
                VideoCodec::H265,
                CodecDirection::Both,
                MaxResolution::UHD4K,
                BitDepth::Bit10,
            ),
            (
                VideoCodec::VP9,
                CodecDirection::Both,
                MaxResolution::UHD4K,
                BitDepth::Bit10,
            ),
        ];

        for (codec, dir, res, depth) in codecs {
            self.codec_caps.push(CodecCapability {
                device: gpu_name.to_string(),
                codec,
                direction: dir,
                max_resolution: res,
                max_bit_depth: depth,
                max_fps: None,
                engine: "Quick Sync".into(),
                source: CapabilitySource::Inferred,
                confidence: CONFIDENCE_INTEL_BASE_CODECS,
            });
        }

        if is_arc {
            self.codec_caps.push(CodecCapability {
                device: gpu_name.to_string(),
                codec: VideoCodec::AV1,
                direction: CodecDirection::Both,
                max_resolution: MaxResolution::UHD8K,
                max_bit_depth: BitDepth::Bit10,
                max_fps: None,
                engine: "Xe Media Engine".into(),
                source: CapabilitySource::Inferred,
                confidence: CONFIDENCE_INTEL_ARC_AV1,
            });
        }
    }

    fn infer_apple_codecs(&mut self, gpu_name: &str) {
        let codecs = vec![
            (
                VideoCodec::H264,
                CodecDirection::Both,
                MaxResolution::UHD4K,
                BitDepth::Bit8,
            ),
            (
                VideoCodec::H265,
                CodecDirection::Both,
                MaxResolution::UHD8K,
                BitDepth::Bit10,
            ),
            (
                VideoCodec::VP9,
                CodecDirection::Decode,
                MaxResolution::UHD4K,
                BitDepth::Bit10,
            ),
            (
                VideoCodec::ProRes,
                CodecDirection::Both,
                MaxResolution::UHD8K,
                BitDepth::Bit10,
            ),
        ];

        for (codec, dir, res, depth) in codecs {
            self.codec_caps.push(CodecCapability {
                device: gpu_name.to_string(),
                codec,
                direction: dir,
                max_resolution: res,
                max_bit_depth: depth,
                max_fps: None,
                engine: "Apple Media Engine".into(),
                source: CapabilitySource::Inferred,
                confidence: CONFIDENCE_APPLE_BASE_CODECS,
            });
        }

        // M3+ and M2 Pro/Max/Ultra have AV1 decode
        let lower = gpu_name.to_lowercase();
        if lower.contains("m3")
            || lower.contains("m4")
            || (lower.contains("m2")
                && (lower.contains("pro") || lower.contains("max") || lower.contains("ultra")))
        {
            self.codec_caps.push(CodecCapability {
                device: gpu_name.to_string(),
                codec: VideoCodec::AV1,
                direction: CodecDirection::Decode,
                max_resolution: MaxResolution::UHD8K,
                max_bit_depth: BitDepth::Bit10,
                max_fps: None,
                engine: "Apple Media Engine".into(),
                source: CapabilitySource::Inferred,
                confidence: CONFIDENCE_APPLE_AV1,
            });
        }
    }

    /// Infer compute capabilities from GPU name.
    fn infer_compute_from_gpu(&mut self, gpu_name: &str) {
        let lower = gpu_name.to_lowercase();

        if lower.contains("nvidia")
            || lower.contains("geforce")
            || lower.contains("rtx")
            || lower.contains("gtx")
            || lower.contains("quadro")
            || lower.contains("tesla")
        {
            // A generation the table does not know gets no figures: a guessed
            // TFLOPS reads the same as a spec-sheet value once it is printed.
            let (cuda, tflops_32, tflops_16, tensor, rt) =
                if lower.contains("rtx 40") || lower.contains("rtx 4090") {
                    ("8.9", Some(82.6_f32), Some(165.2_f32), true, true)
                } else if lower.contains("rtx 4080") {
                    ("8.9", Some(48.7), Some(97.5), true, true)
                } else if lower.contains("rtx 4070") {
                    ("8.9", Some(29.1), Some(58.3), true, true)
                } else if lower.contains("rtx 30") || lower.contains("rtx 3090") {
                    ("8.6", Some(35.6), Some(71.2), true, true)
                } else if lower.contains("rtx 3080") {
                    ("8.6", Some(29.8), Some(59.6), true, true)
                } else if lower.contains("rtx 20") || lower.contains("rtx 2080") {
                    ("7.5", Some(14.2), Some(28.4), true, true)
                } else if lower.contains("gtx 1080") {
                    ("6.1", Some(8.9), Some(0.0), false, false)
                } else if lower.contains("gtx 10") {
                    ("6.1", Some(6.5), Some(0.0), false, false)
                } else {
                    ("6.0", None, None, false, false)
                };

            self.compute_caps.push(ComputeCapability {
                device: gpu_name.to_string(),
                cuda_version: cuda.to_string(),
                opencl_version: "3.0".into(),
                vulkan_compute: true,
                metal_compute: false,
                direct_compute: "5.0".into(),
                estimated_tflops_fp32: tflops_32,
                estimated_tflops_fp16: tflops_16,
                has_tensor_cores: tensor,
                has_ray_tracing: rt,
                source: CapabilitySource::Inferred,
            });
        }

        if lower.contains("apple")
            || lower.contains("m1")
            || lower.contains("m2")
            || lower.contains("m3")
            || lower.contains("m4")
        {
            let tflops = if lower.contains("m4") || lower.contains("ultra") {
                Some(27.0_f32)
            } else if lower.contains("m3") || lower.contains("max") {
                Some(14.2)
            } else if lower.contains("m2") || lower.contains("pro") {
                Some(6.8)
            } else if lower.contains("m1") {
                Some(2.6)
            } else {
                None
            };

            self.compute_caps.push(ComputeCapability {
                device: gpu_name.to_string(),
                cuda_version: String::new(),
                opencl_version: String::new(),
                vulkan_compute: false,
                metal_compute: true,
                direct_compute: String::new(),
                estimated_tflops_fp32: tflops,
                estimated_tflops_fp16: tflops.map(|t| t * 2.0),
                has_tensor_cores: true, // Apple Neural Engine
                has_ray_tracing: lower.contains("m3") || lower.contains("m4"),
                source: CapabilitySource::Inferred,
            });
        }
    }

    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) {
        // Try vainfo for VA-API capabilities
        if let Ok(output) = std::process::Command::new("vainfo")
            .args(["--display", "drm"])
            .output()
        {
            let text = String::from_utf8(output.stdout).unwrap_or_default();
            let device = "VA-API";
            for line in text.lines() {
                let line = line.trim();
                if !line.starts_with("VAProfile") {
                    continue;
                }
                // "VAProfileH264Main            : VAEntrypointVLD"
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() < 2 {
                    continue;
                }
                let profile = parts[0].trim();
                let entrypoint = parts[1].trim();

                let codec = if profile.contains("H264") {
                    VideoCodec::H264
                } else if profile.contains("HEVC") || profile.contains("H265") {
                    VideoCodec::H265
                } else if profile.contains("VP9") {
                    VideoCodec::VP9
                } else if profile.contains("VP8") {
                    VideoCodec::VP8
                } else if profile.contains("AV1") {
                    VideoCodec::AV1
                } else if profile.contains("MPEG2") {
                    VideoCodec::MPEG2
                } else if profile.contains("JPEG") {
                    VideoCodec::JPEG
                } else {
                    continue;
                };

                let direction = if entrypoint.contains("VLD") {
                    CodecDirection::Decode
                } else if entrypoint.contains("Enc") {
                    CodecDirection::Encode
                } else {
                    continue;
                };

                self.codec_caps.push(CodecCapability {
                    device: device.to_string(),
                    codec,
                    direction,
                    max_resolution: MaxResolution::UHD4K,
                    max_bit_depth: if profile.contains("10") {
                        BitDepth::Bit10
                    } else {
                        BitDepth::Bit8
                    },
                    max_fps: None,
                    engine: "VA-API".into(),
                    source: CapabilitySource::DirectQuery,
                    confidence: CONFIDENCE_VAINFO_QUERY,
                });
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) {
        // Query DXVA capabilities is complex; we rely on inference primarily
        // But we can check for basic GPU info
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) {
        // macOS: VideoToolbox capabilities are best inferred from chip name
    }
}

impl Default for CodecMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            codec_caps: Vec::new(),
            compute_caps: Vec::new(),
        })
    }
}

impl std::fmt::Display for VideoCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::H264 => write!(f, "H.264/AVC"),
            Self::H265 => write!(f, "H.265/HEVC"),
            Self::H266 => write!(f, "H.266/VVC"),
            Self::VP8 => write!(f, "VP8"),
            Self::VP9 => write!(f, "VP9"),
            Self::AV1 => write!(f, "AV1"),
            Self::MPEG2 => write!(f, "MPEG-2"),
            Self::MPEG4 => write!(f, "MPEG-4"),
            Self::VC1 => write!(f, "VC-1"),
            Self::JPEG => write!(f, "JPEG"),
            Self::ProRes => write!(f, "ProRes"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::fmt::Display for MaxResolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SD => write!(f, "SD (480p)"),
            Self::HD => write!(f, "HD (720p)"),
            Self::FullHD => write!(f, "Full HD (1080p)"),
            Self::QHD => write!(f, "QHD (1440p)"),
            Self::UHD4K => write!(f, "4K UHD"),
            Self::UHD8K => write!(f, "8K UHD"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codec_monitor_creation() {
        let monitor = CodecMonitor::new();
        assert!(monitor.is_ok());
    }

    #[test]
    fn test_codec_monitor_default() {
        let monitor = CodecMonitor::default();
        let _ = monitor.capabilities();
        let _ = monitor.compute_capabilities();
        let _ = monitor.total_tflops_fp32();
    }

    #[test]
    fn test_nvidia_inference() {
        let mut monitor = CodecMonitor {
            codec_caps: Vec::new(),
            compute_caps: Vec::new(),
        };
        monitor.infer_from_gpu_name("NVIDIA GeForce RTX 4090");
        assert!(!monitor.codec_caps.is_empty());
        assert!(monitor.can_decode(&VideoCodec::H265));
        assert!(monitor.can_encode(&VideoCodec::H264));
        assert!(monitor.can_decode(&VideoCodec::AV1));
        assert!(monitor.can_encode(&VideoCodec::AV1)); // RTX 40xx
    }

    #[test]
    fn test_compute_inference() {
        let mut monitor = CodecMonitor {
            codec_caps: Vec::new(),
            compute_caps: Vec::new(),
        };
        monitor.infer_compute_from_gpu("NVIDIA GeForce RTX 4090");
        assert!(!monitor.compute_caps.is_empty());
        assert!(monitor.compute_caps[0].has_tensor_cores);
        assert!(monitor.compute_caps[0].has_ray_tracing);
        assert!(matches!(
            monitor.compute_caps[0].estimated_tflops_fp32,
            Some(t) if t > 50.0
        ));
    }

    #[test]
    fn test_compute_unknown_generation_has_no_figures() {
        let mut monitor = CodecMonitor {
            codec_caps: Vec::new(),
            compute_caps: Vec::new(),
        };
        monitor.infer_compute_from_gpu("NVIDIA Quadro Unknown 9000");
        assert!(!monitor.compute_caps.is_empty());
        assert_eq!(monitor.compute_caps[0].estimated_tflops_fp32, None);
        assert_eq!(monitor.compute_caps[0].estimated_tflops_fp16, None);
        assert_eq!(monitor.total_tflops_fp32(), None);
    }

    #[test]
    fn test_serialization() {
        let cap = CodecCapability {
            device: "Test GPU".into(),
            codec: VideoCodec::AV1,
            direction: CodecDirection::Decode,
            max_resolution: MaxResolution::UHD8K,
            max_bit_depth: BitDepth::Bit10,
            max_fps: Some(60),
            engine: "NVDEC".into(),
            source: CapabilitySource::Inferred,
            confidence: 0.85,
        };
        let json = serde_json::to_string(&cap).unwrap();
        assert!(json.contains("AV1"));
        let _: CodecCapability = serde_json::from_str(&json).unwrap();
    }
}
