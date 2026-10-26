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
//!     println!("{}: {} {:?} (max {:?})",
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
    /// Maximum supported resolution, or `None` where no frame class was read.
    /// The Apple rows used to hold literal 4K and 8K classes for every chip, and
    /// the VA-API rows a literal 4K, neither of which any platform query reports.
    pub max_resolution: Option<MaxResolution>,
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
    /// CUDA compute capability (e.g., "8.9") — NVIDIA only; `None` when the
    /// generation is not known
    pub cuda_version: Option<String>,
    /// OpenCL version supported. `None` where it was not read. The NVIDIA
    /// rows used to hold the literal "3.0" for every card, whatever its
    /// generation, and the Apple rows an empty string.
    pub opencl_version: Option<String>,
    /// Vulkan compute supported. `None` where it was not read: the NVIDIA rows
    /// used to hold `true` unconditionally.
    pub vulkan_compute: Option<bool>,
    /// Metal compute supported (Apple)
    pub metal_compute: bool,
    /// DirectCompute version (Windows). `None` where it was not read: the
    /// NVIDIA rows used to hold the literal "5.0" for every card.
    pub direct_compute: Option<String>,
    /// Estimated TFLOPS (FP32); `None` for a generation the table does not know
    pub estimated_tflops_fp32: Option<f32>,
    /// Estimated TFLOPS (FP16 / half precision); `None` when FP32 is unknown
    pub estimated_tflops_fp16: Option<f32>,
    /// Tensor core / AI accelerator present; `None` when not known
    pub has_tensor_cores: Option<bool>,
    /// Ray tracing hardware acceleration; `None` when not known
    pub has_ray_tracing: Option<bool>,
    /// Source of capability info
    pub source: CapabilitySource,
}

/// Whether `s` is a non-empty run of ASCII digits.
fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// NVIDIA product line. The line decides tensor cores and ray tracing: the RTX
/// line has both from Turing on, and the GTX 10 and 16 series have neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NvidiaLine {
    Gtx,
    Rtx,
}

/// NVIDIA GPU generation, as the model number places it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NvidiaGen {
    Pascal,
    Turing,
    Ampere,
    Ada,
}

/// The parts of an NVIDIA model name this module reads. "NVIDIA GeForce RTX 3090 Ti"
/// is `Rtx`, `3090`, and suffix `ti`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NvidiaModel {
    line: NvidiaLine,
    /// The model number token: `3090`, `1660`, or `a4000` for an RTX A-series part.
    number: String,
    /// The lower-case word after the number, if any: `ti`, `super`, `ada`.
    suffix: Option<String>,
}

impl NvidiaModel {
    /// The generation this model number places the part in, or `None` for a
    /// number the table does not place.
    fn generation(&self) -> Option<NvidiaGen> {
        if self.suffix.as_deref() == Some("ada") {
            return Some(NvidiaGen::Ada);
        }
        if self.line == NvidiaLine::Rtx && self.number.starts_with('a') {
            // RTX A-series workstation parts (A2000 to A6000) are Ampere.
            return Some(NvidiaGen::Ampere);
        }
        match (self.line, self.number.get(..2)?) {
            (NvidiaLine::Gtx, "10") => Some(NvidiaGen::Pascal),
            (NvidiaLine::Gtx, "16") => Some(NvidiaGen::Turing),
            (NvidiaLine::Rtx, "20") => Some(NvidiaGen::Turing),
            (NvidiaLine::Rtx, "30") => Some(NvidiaGen::Ampere),
            (NvidiaLine::Rtx, "40") => Some(NvidiaGen::Ada),
            _ => None,
        }
    }
}

/// Parse an NVIDIA model name. The model is the token after "GTX" or "RTX": a
/// GeForce number is three or four digits, and an RTX A-series number is an "A"
/// and four digits. Anything else (Tesla, "Quadro P4000", a name with no model)
/// is `None`, so a substring such as "40" can never place a part by accident.
fn parse_nvidia_model(name: &str) -> Option<NvidiaModel> {
    let lower = name.to_lowercase();
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    let (idx, line) = tokens.iter().enumerate().find_map(|(i, t)| match *t {
        "rtx" => Some((i, NvidiaLine::Rtx)),
        "gtx" => Some((i, NvidiaLine::Gtx)),
        _ => None,
    })?;
    let number = *tokens.get(idx + 1)?;
    let geforce = (3..=4).contains(&number.len()) && all_digits(number);
    let a_series = line == NvidiaLine::Rtx
        && number.len() == 5
        && number.starts_with('a')
        && all_digits(&number[1..]);
    if !geforce && !a_series {
        return None;
    }
    let suffix = tokens
        .get(idx + 2)
        .filter(|s| s.bytes().all(|b| b.is_ascii_alphabetic()))
        .map(|s| s.to_string());
    Some(NvidiaModel {
        line,
        number: number.to_string(),
        suffix,
    })
}

/// FP32 and FP16 TFLOPS for the base models listed here, matched on the exact
/// model number with no suffix. A "Ti" or "SUPER" part has its own figures, and
/// none is taken from its base model.
fn nvidia_tflops(model: &NvidiaModel) -> Option<(f32, Option<f32>)> {
    if model.suffix.is_some() {
        return None;
    }
    match (model.line, model.number.as_str()) {
        (NvidiaLine::Rtx, "4090") => Some((82.6, Some(165.2))),
        (NvidiaLine::Rtx, "4080") => Some((48.7, Some(97.5))),
        (NvidiaLine::Rtx, "4070") => Some((29.1, Some(58.3))),
        (NvidiaLine::Rtx, "3090") => Some((35.6, Some(71.2))),
        (NvidiaLine::Rtx, "3080") => Some((29.8, Some(59.6))),
        (NvidiaLine::Rtx, "2080") => Some((14.2, Some(28.4))),
        (NvidiaLine::Gtx, "1080") => Some((8.9, None)),
        _ => None,
    }
}

/// RDNA generation of an AMD Radeon RX model, as the model number places it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AmdGen {
    Rdna1,
    Rdna2,
    Rdna3,
}

/// Parse the RDNA generation from an AMD RX model name. RDNA models carry four
/// digits after "RX" (RX 5700 XT, RX 6800M, RX 7900 XTX). The three-digit RX 500
/// series is Polaris and is not placed here, and a name with no RX model number
/// (an APU's "Radeon(TM) Graphics") is not placed either.
fn parse_amd_rx_generation(name: &str) -> Option<AmdGen> {
    let lower = name.to_lowercase();
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    let idx = tokens.iter().position(|t| *t == "rx")?;
    let number = *tokens.get(idx + 1)?;
    // Mobile parts end in "M" and share their desktop family's generation.
    let digits = number.strip_suffix('m').unwrap_or(number);
    if digits.len() != 4 || !all_digits(digits) {
        return None;
    }
    match digits.as_bytes()[0] {
        b'5' => Some(AmdGen::Rdna1),
        b'6' => Some(AmdGen::Rdna2),
        b'7' => Some(AmdGen::Rdna3),
        _ => None,
    }
}

/// Monitor for codec and compute capabilities
pub struct CodecMonitor {
    codec_caps: Vec<CodecCapability>,
    compute_caps: Vec<ComputeCapability>,
    /// GPUs whose names the inference rules did not place. They get no codec rows.
    unmatched_gpus: Vec<String>,
}

impl CodecMonitor {
    pub fn new() -> Result<Self, IronError> {
        let mut monitor = Self {
            codec_caps: Vec::new(),
            compute_caps: Vec::new(),
            unmatched_gpus: Vec::new(),
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.codec_caps.clear();
        self.compute_caps.clear();
        self.unmatched_gpus.clear();

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
                let before = self.codec_caps.len();
                self.infer_from_gpu_name(name);
                if self.codec_caps.len() == before {
                    self.unmatched_gpus.push(name.clone());
                }
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

    /// GPU names the codec inference could not place. Each carries no codec rows,
    /// and the ontology reports it as unavailable instead of leaving it out.
    pub fn unmatched_gpus(&self) -> &[String] {
        &self.unmatched_gpus
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
            // A row with no frame class is `None`, and so is `Unknown`. Neither
            // may reach `max()`: `Unknown` sorts above every real class in the
            // derived ordering.
            .filter_map(|c| c.max_resolution.clone())
            .filter(|r| *r != MaxResolution::Unknown)
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
            self.infer_nvidia_codecs(gpu_name);
        }

        // AMD inference
        if lower.contains("amd") || lower.contains("radeon") || lower.contains("rx ") {
            self.infer_amd_codecs(gpu_name);
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

    /// Push one inferred row. Resolution and bit depth are `Unknown`: the
    /// per-generation figures these rows used to carry were not taken from a
    /// vendor matrix, so the reader no longer claims them.
    fn push_inferred_codec(
        &mut self,
        gpu_name: &str,
        codec: VideoCodec,
        direction: CodecDirection,
        engine: &str,
        confidence: f32,
    ) {
        self.codec_caps.push(CodecCapability {
            device: gpu_name.to_string(),
            codec,
            direction,
            max_resolution: None,
            max_bit_depth: BitDepth::Unknown,
            max_fps: None,
            engine: engine.to_string(),
            source: CapabilitySource::Inferred,
            confidence,
        });
    }

    fn infer_nvidia_codecs(&mut self, gpu_name: &str) {
        // A name the model-number rule does not place gets no rows; `refresh`
        // records it as unmatched. The rule this replaces matched substrings, so
        // "RTX A4000" was placed in the RTX 40 generation.
        let Some(generation) = parse_nvidia_model(gpu_name).and_then(|m| m.generation()) else {
            return;
        };

        // H.264, H.265, VP9, MPEG-2 and VC-1 across the generations placed above,
        // per the NVIDIA Video Encode and Decode GPU Support Matrix.
        for (codec, dir) in [
            (VideoCodec::H264, CodecDirection::Both),
            (VideoCodec::H265, CodecDirection::Both),
            (VideoCodec::VP9, CodecDirection::Decode),
            (VideoCodec::MPEG2, CodecDirection::Decode),
            (VideoCodec::VC1, CodecDirection::Decode),
        ] {
            self.push_inferred_codec(
                gpu_name,
                codec,
                dir,
                "NVENC/NVDEC",
                CONFIDENCE_NVIDIA_BASE_CODECS,
            );
        }

        // AV1: Ada decodes and encodes, Ampere decodes only, Turing has neither.
        match generation {
            NvidiaGen::Ada => self.push_inferred_codec(
                gpu_name,
                VideoCodec::AV1,
                CodecDirection::Both,
                "NVENC/NVDEC",
                CONFIDENCE_NVIDIA_AV1_RTX40,
            ),
            NvidiaGen::Ampere => self.push_inferred_codec(
                gpu_name,
                VideoCodec::AV1,
                CodecDirection::Decode,
                "NVDEC",
                CONFIDENCE_NVIDIA_AV1_RTX30,
            ),
            NvidiaGen::Pascal | NvidiaGen::Turing => {}
        }
    }

    fn infer_amd_codecs(&mut self, gpu_name: &str) {
        // Only the RDNA families are placed, by RX model number. Polaris (RX 500)
        // and older parts, and APU names with no model number, get no rows. The
        // rule this replaces read any 6 or 7 in the name as an RDNA generation.
        let Some(generation) = parse_amd_rx_generation(gpu_name) else {
            return;
        };

        // The media engine version is AMD's own naming for each family:
        // RDNA 1 = VCN 2.0, RDNA 2 = VCN 3.0, RDNA 3 = VCN 4.0.
        let engine = match generation {
            AmdGen::Rdna1 => "VCN 2.0",
            AmdGen::Rdna2 => "VCN 3.0",
            AmdGen::Rdna3 => "VCN 4.0",
        };
        for (codec, dir) in [
            (VideoCodec::H264, CodecDirection::Both),
            (VideoCodec::H265, CodecDirection::Both),
            (VideoCodec::VP9, CodecDirection::Decode),
        ] {
            self.push_inferred_codec(gpu_name, codec, dir, engine, CONFIDENCE_AMD_BASE_CODECS);
        }

        match generation {
            AmdGen::Rdna1 => {}
            AmdGen::Rdna2 => self.push_inferred_codec(
                gpu_name,
                VideoCodec::AV1,
                CodecDirection::Decode,
                engine,
                CONFIDENCE_AMD_AV1_RDNA2,
            ),
            AmdGen::Rdna3 => self.push_inferred_codec(
                gpu_name,
                VideoCodec::AV1,
                CodecDirection::Both,
                engine,
                CONFIDENCE_AMD_AV1_RDNA3,
            ),
        }
    }

    fn infer_intel_codecs(&mut self, gpu_name: &str, lower: &str) {
        // H.264 and H.265 for the Quick Sync names matched here. VP9 is not
        // emitted: its support depends on the generation, which a name such as
        // "Intel UHD Graphics" does not carry. The rule this replaces gave every
        // Intel GPU VP9 at 4K.
        for codec in [VideoCodec::H264, VideoCodec::H265] {
            self.push_inferred_codec(
                gpu_name,
                codec,
                CodecDirection::Both,
                "Quick Sync",
                CONFIDENCE_INTEL_BASE_CODECS,
            );
        }

        if lower.contains("arc") {
            self.push_inferred_codec(
                gpu_name,
                VideoCodec::AV1,
                CodecDirection::Both,
                "Xe Media Engine",
                CONFIDENCE_INTEL_ARC_AV1,
            );
        }
    }

    fn infer_apple_codecs(&mut self, gpu_name: &str) {
        // No frame class is read for these rows, so `max_resolution` is `None`.
        // The rows used to name a literal 4K or 8K class per codec, which no
        // platform query reported for this chip.
        let codecs = vec![
            (VideoCodec::H264, CodecDirection::Both, BitDepth::Bit8),
            (VideoCodec::H265, CodecDirection::Both, BitDepth::Bit10),
            (VideoCodec::VP9, CodecDirection::Decode, BitDepth::Bit10),
            (VideoCodec::ProRes, CodecDirection::Both, BitDepth::Bit10),
        ];

        for (codec, dir, depth) in codecs {
            self.codec_caps.push(CodecCapability {
                device: gpu_name.to_string(),
                codec,
                direction: dir,
                max_resolution: None,
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
                max_resolution: None,
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
            // The generation fixes the CUDA compute capability and the two feature
            // flags. The TFLOPS figures are per model, and only for the base models
            // in `nvidia_tflops`. A name the rules do not place gets no figures.
            let model = parse_nvidia_model(gpu_name);
            let generation = model.as_ref().and_then(NvidiaModel::generation);
            let cuda = generation.map(|g| match g {
                NvidiaGen::Pascal => "6.1",
                NvidiaGen::Turing => "7.5",
                NvidiaGen::Ampere => "8.6",
                NvidiaGen::Ada => "8.9",
            });
            let (tensor, rt) = match (&model, generation) {
                (Some(m), Some(_)) => {
                    let rtx = m.line == NvidiaLine::Rtx;
                    (Some(rtx), Some(rtx))
                }
                _ => (None, None),
            };
            let (tflops_32, tflops_16) = match model.as_ref().and_then(nvidia_tflops) {
                Some((fp32, fp16)) => (Some(fp32), fp16),
                None => (None, None),
            };

            self.compute_caps.push(ComputeCapability {
                device: gpu_name.to_string(),
                cuda_version: cuda.map(String::from),
                opencl_version: None,
                vulkan_compute: None,
                metal_compute: false,
                direct_compute: None,
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
                cuda_version: None,
                opencl_version: None,
                vulkan_compute: None,
                metal_compute: true,
                direct_compute: None,
                estimated_tflops_fp32: tflops,
                estimated_tflops_fp16: tflops.map(|t| t * 2.0),
                // The Neural Engine is a separate block, not tensor cores in the
                // GPU, which this used to claim. M1-M4 GPUs have no such units;
                // later or unrecognised parts are not guessed.
                has_tensor_cores: ["m1", "m2", "m3", "m4"]
                    .iter()
                    .any(|m| lower.contains(m))
                    .then_some(false),
                // Ray tracing arrived with M3; M1/M2 are known not to have it.
                has_ray_tracing: if lower.contains("m3") || lower.contains("m4") {
                    Some(true)
                } else if lower.contains("m1") || lower.contains("m2") {
                    Some(false)
                } else {
                    None
                },
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
                    // vainfo's profile and entrypoint lines carry no frame size,
                    // and this reader does not query one. It used to be the
                    // literal UHD4K on every row.
                    max_resolution: None,
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
            unmatched_gpus: Vec::new(),
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

    fn decode_row(res: Option<MaxResolution>) -> CodecCapability {
        CodecCapability {
            device: "VA-API".into(),
            codec: VideoCodec::H264,
            direction: CodecDirection::Decode,
            max_resolution: res,
            max_bit_depth: BitDepth::Bit8,
            max_fps: None,
            engine: "VA-API".into(),
            source: CapabilitySource::DirectQuery,
            confidence: 1.0,
        }
    }

    /// A VA-API row whose frame size was never read must not win the maximum,
    /// and a row with no size at all must not make the answer `Unknown` when a
    /// classified row exists.
    #[test]
    fn test_unclassified_row_does_not_win_max_decode_resolution() {
        let monitor = CodecMonitor {
            codec_caps: vec![decode_row(None), decode_row(Some(MaxResolution::FullHD))],
            compute_caps: Vec::new(),
            unmatched_gpus: Vec::new(),
        };
        assert_eq!(
            monitor.max_decode_resolution(&VideoCodec::H264),
            MaxResolution::FullHD
        );
        let only_unknown = CodecMonitor {
            codec_caps: vec![decode_row(None)],
            compute_caps: Vec::new(),
            unmatched_gpus: Vec::new(),
        };
        assert_eq!(
            only_unknown.max_decode_resolution(&VideoCodec::H264),
            MaxResolution::Unknown
        );
    }

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
            unmatched_gpus: Vec::new(),
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
            unmatched_gpus: Vec::new(),
        };
        monitor.infer_compute_from_gpu("NVIDIA GeForce RTX 4090");
        assert!(!monitor.compute_caps.is_empty());
        assert_eq!(monitor.compute_caps[0].has_tensor_cores, Some(true));
        assert_eq!(monitor.compute_caps[0].has_ray_tracing, Some(true));
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
            unmatched_gpus: Vec::new(),
        };
        monitor.infer_compute_from_gpu("NVIDIA Quadro Unknown 9000");
        assert!(!monitor.compute_caps.is_empty());
        assert_eq!(monitor.compute_caps[0].estimated_tflops_fp32, None);
        assert_eq!(monitor.compute_caps[0].estimated_tflops_fp16, None);
        assert_eq!(monitor.compute_caps[0].cuda_version, None);
        assert_eq!(monitor.compute_caps[0].has_tensor_cores, None);
        assert_eq!(monitor.compute_caps[0].has_ray_tracing, None);
        assert_eq!(monitor.total_tflops_fp32(), None);
    }

    fn empty_monitor() -> CodecMonitor {
        CodecMonitor {
            codec_caps: Vec::new(),
            compute_caps: Vec::new(),
            unmatched_gpus: Vec::new(),
        }
    }

    #[test]
    fn test_parse_nvidia_model_names() {
        let ti = parse_nvidia_model("NVIDIA GeForce RTX 3090 Ti").unwrap();
        assert_eq!(ti.line, NvidiaLine::Rtx);
        assert_eq!(ti.number, "3090");
        assert_eq!(ti.suffix.as_deref(), Some("ti"));
        assert_eq!(ti.generation(), Some(NvidiaGen::Ampere));

        let sup = parse_nvidia_model("NVIDIA GeForce RTX 4070 SUPER").unwrap();
        assert_eq!(sup.number, "4070");
        assert_eq!(sup.suffix.as_deref(), Some("super"));
        assert_eq!(sup.generation(), Some(NvidiaGen::Ada));

        let a4000 = parse_nvidia_model("NVIDIA RTX A4000").unwrap();
        assert_eq!(a4000.number, "a4000");
        assert_eq!(a4000.suffix, None);
        assert_eq!(a4000.generation(), Some(NvidiaGen::Ampere));

        let gtx = parse_nvidia_model("NVIDIA GeForce GTX 1660 Ti").unwrap();
        assert_eq!(gtx.line, NvidiaLine::Gtx);
        assert_eq!(gtx.generation(), Some(NvidiaGen::Turing));

        assert_eq!(parse_nvidia_model("AMD Radeon RX 570"), None);
        assert_eq!(parse_nvidia_model("NVIDIA Quadro Unknown 9000"), None);
    }

    #[test]
    fn test_parse_amd_rx_generation() {
        // Polaris (three digits) and an APU name are not placed.
        assert_eq!(parse_amd_rx_generation("AMD Radeon RX 570"), None);
        assert_eq!(parse_amd_rx_generation("AMD Radeon(TM) Graphics"), None);
        assert_eq!(
            parse_amd_rx_generation("AMD Radeon RX 5700 XT"),
            Some(AmdGen::Rdna1)
        );
        assert_eq!(
            parse_amd_rx_generation("AMD Radeon RX 7900 XTX"),
            Some(AmdGen::Rdna3)
        );
    }

    #[test]
    fn test_nvidia_substring_does_not_place_a_model() {
        // "RTX A4000" contains "40" and "rtx", but it is Ampere, not RTX 40.
        let mut monitor = empty_monitor();
        monitor.infer_from_gpu_name("NVIDIA RTX A4000");
        assert!(!monitor.can_encode(&VideoCodec::AV1));
        assert!(monitor.can_decode(&VideoCodec::AV1));
    }

    #[test]
    fn test_amd_polaris_and_apu_get_no_rows() {
        let mut monitor = empty_monitor();
        monitor.infer_from_gpu_name("AMD Radeon RX 570");
        monitor.infer_from_gpu_name("AMD Radeon(TM) Graphics");
        assert!(monitor.capabilities().is_empty());
    }

    #[test]
    fn test_amd_rdna_generation_from_model_number() {
        let mut rdna1 = empty_monitor();
        rdna1.infer_from_gpu_name("AMD Radeon RX 5700 XT");
        assert!(rdna1.can_encode(&VideoCodec::H264));
        assert!(!rdna1.can_decode(&VideoCodec::AV1));

        let mut rdna3 = empty_monitor();
        rdna3.infer_from_gpu_name("AMD Radeon RX 7900 XTX");
        assert!(rdna3.can_encode(&VideoCodec::AV1));
        assert!(rdna3.capabilities().iter().all(|c| c.engine == "VCN 4.0"));
    }

    #[test]
    fn test_intel_has_no_vp9_row() {
        let mut monitor = empty_monitor();
        monitor.infer_from_gpu_name("Intel(R) UHD Graphics 770");
        assert!(!monitor
            .capabilities()
            .iter()
            .any(|c| c.codec == VideoCodec::VP9));
        assert!(monitor.can_decode(&VideoCodec::H265));
    }

    #[test]
    fn test_nvidia_ampere_has_no_av1_encode() {
        let mut monitor = empty_monitor();
        monitor.infer_from_gpu_name("NVIDIA GeForce RTX 3090 Ti");
        assert!(monitor.can_decode(&VideoCodec::AV1));
        assert!(!monitor.can_encode(&VideoCodec::AV1));
        assert_eq!(
            monitor.max_decode_resolution(&VideoCodec::H264),
            MaxResolution::Unknown
        );
    }

    #[test]
    fn test_compute_suffix_model_gets_no_tflops() {
        let mut monitor = empty_monitor();
        monitor.infer_compute_from_gpu("NVIDIA GeForce RTX 3090 Ti");
        let c = &monitor.compute_caps[0];
        assert_eq!(c.cuda_version.as_deref(), Some("8.6"));
        assert_eq!(c.has_tensor_cores, Some(true));
        assert_eq!(c.estimated_tflops_fp32, None);

        monitor.infer_compute_from_gpu("NVIDIA GeForce RTX 4070 SUPER");
        assert_eq!(monitor.compute_caps[1].estimated_tflops_fp32, None);
    }

    #[test]
    fn test_compute_base_model_and_gtx16_flags() {
        let mut monitor = empty_monitor();
        monitor.infer_compute_from_gpu("NVIDIA GeForce RTX 3090");
        assert_eq!(monitor.compute_caps[0].estimated_tflops_fp32, Some(35.6));

        let mut gtx = empty_monitor();
        gtx.infer_compute_from_gpu("NVIDIA GeForce GTX 1660 Ti");
        let c = &gtx.compute_caps[0];
        assert_eq!(c.cuda_version.as_deref(), Some("7.5"));
        assert_eq!(c.has_tensor_cores, Some(false));
        assert_eq!(c.has_ray_tracing, Some(false));
    }

    #[test]
    fn test_apple_rows_name_no_frame_class() {
        // The Apple table is inferred from the chip name, which fixes the codecs
        // and not a frame size. No row may carry a resolution it did not read.
        let mut monitor = empty_monitor();
        monitor.infer_apple_codecs("Apple M3 Max");
        assert!(!monitor.codec_caps.is_empty());
        assert!(monitor
            .codec_caps
            .iter()
            .all(|c| c.max_resolution.is_none()));
        assert!(monitor
            .codec_caps
            .iter()
            .any(|c| c.codec == VideoCodec::AV1));
        assert_eq!(
            monitor.max_decode_resolution(&VideoCodec::H264),
            MaxResolution::Unknown
        );
    }

    #[test]
    fn test_serialization() {
        let cap = CodecCapability {
            device: "Test GPU".into(),
            codec: VideoCodec::AV1,
            direction: CodecDirection::Decode,
            max_resolution: Some(MaxResolution::UHD8K),
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
