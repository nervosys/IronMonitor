//! Platform detection for Linux

use crate::core::platform_info::{BoardInfo, HardwareInfo, LibraryVersions, PlatformInfo};
use crate::error::Result;
use std::collections::HashMap;
use std::fs;

/// Detect platform information
pub fn detect_platform() -> Result<BoardInfo> {
    let mut info = BoardInfo::new()?;

    info.platform = detect_platform_info()?;
    info.hardware = detect_hardware_info()?;
    info.libraries = detect_library_versions()?;

    Ok(info)
}

fn detect_platform_info() -> Result<PlatformInfo> {
    let machine = std::env::consts::ARCH.to_string();
    let system = std::env::consts::OS.to_string();

    // Read OS release info
    let distribution = read_distribution();
    let release = read_kernel_release();

    Ok(PlatformInfo {
        machine,
        system,
        distribution,
        release,
    })
}

fn read_distribution() -> Option<String> {
    // Try /etc/os-release
    if let Ok(content) = fs::read_to_string("/etc/os-release") {
        for line in content.lines() {
            if line.starts_with("PRETTY_NAME=") {
                let name = line.trim_start_matches("PRETTY_NAME=").trim_matches('"');
                return Some(name.to_string());
            }
        }
    }

    // Try /etc/lsb-release
    if let Ok(content) = fs::read_to_string("/etc/lsb-release") {
        for line in content.lines() {
            if line.starts_with("DISTRIB_DESCRIPTION=") {
                let name = line
                    .trim_start_matches("DISTRIB_DESCRIPTION=")
                    .trim_matches('"');
                return Some(name.to_string());
            }
        }
    }

    None
}

fn read_kernel_release() -> String {
    fs::read_to_string("/proc/version")
        .ok()
        .and_then(|content| content.split_whitespace().nth(2).map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

fn detect_hardware_info() -> Result<HardwareInfo> {
    use super::jetson::{read_jetson_model, read_l4t_version, read_serial_number};

    // Empty, not "Unknown": the resolver publishes an empty model as unavailable.
    let model = read_jetson_model().unwrap_or_default();
    let serial_number = read_serial_number();
    let l4t = read_l4t_version();

    // Parse P-Number from device tree or EEPROM
    let p_number = read_p_number();

    // Read SoC info from device tree compatible string
    let soc = read_soc_info();

    // Only nvcc can report the architecture. The SoC-name table used to fill it
    // in when nvcc was absent; that value was not read here.
    let cuda_arch = detect_cuda_arch();

    // Looked up from the P-Number, which was read. The model-name fallback used to
    // guess a module from a product name, and was dropped.
    let module = derive_module_from_p_number(&p_number);

    // No codename is read. This used to be derived from a substring of the model
    // name ("orin" -> "Orin"), which is a guess from a product name.
    let codename = None;

    // Map L4T version to JetPack version
    let jetpack = l4t.as_ref().and_then(|v| map_l4t_to_jetpack(v));

    Ok(HardwareInfo {
        model,
        p_number,
        module,
        soc,
        cuda_arch,
        codename,
        serial_number,
        l4t,
        jetpack,
    })
}

/// Read P-Number from device tree or EEPROM
fn read_p_number() -> Option<String> {
    // Try reading from /proc/device-tree/nvidia,p-number (Orin+)
    if let Ok(pnum) = fs::read_to_string("/proc/device-tree/nvidia,p-number") {
        let pnum = pnum.trim_end_matches('\0').trim();
        if !pnum.is_empty() {
            return Some(pnum.to_string());
        }
    }

    // Try reading from EEPROM via tegra-boardspec
    if let Ok(content) = fs::read_to_string("/sys/firmware/devicetree/base/nvidia,boardids") {
        let content = content.trim_end_matches('\0').trim();
        if !content.is_empty() {
            // Format is typically "2888-0400-0004-300-0-001-E"
            if let Some(p_num) = content.split('-').next() {
                return Some(format!("P{}", p_num));
            }
        }
    }

    None
}

/// Read SoC information from device tree compatible string
fn read_soc_info() -> Option<String> {
    if let Ok(compatible) = fs::read_to_string("/proc/device-tree/compatible") {
        let compatible = compatible.trim_end_matches('\0');
        // Parse compatible string to extract SoC
        for compat in compatible.split('\0') {
            let compat = compat.trim();
            if compat.starts_with("nvidia,") {
                // Map compatible strings to SoC names
                if compat.contains("tegra234") {
                    return Some("Tegra234 (Orin)".to_string());
                } else if compat.contains("tegra194") {
                    return Some("Tegra194 (Xavier)".to_string());
                } else if compat.contains("tegra186") {
                    return Some("Tegra186 (TX2)".to_string());
                } else if compat.contains("tegra210") {
                    return Some("Tegra210 (TX1/Nano)".to_string());
                } else if compat.contains("tegra132") {
                    return Some("Tegra K1".to_string());
                }
            }
        }
    }

    // No fallback to the model name. It used to match "orin", "xavier" and so on
    // in the board name, which is a guess from a product name.
    None
}

/// Detect CUDA architecture from nvcc. Without nvcc there is no reading.
fn detect_cuda_arch() -> Option<String> {
    // Try to get from nvcc first
    if let Ok(output) = std::process::Command::new("nvcc")
        .args(["--list-gpu-arch"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Find the highest compute capability
            for line in stdout.lines().rev() {
                if line.starts_with("compute_") || line.starts_with("sm_") {
                    return Some(line.trim().to_string());
                }
            }
        }
    }

    None
}

/// Derive module name from P-Number
fn derive_module_from_p_number(p_number: &Option<String>) -> Option<String> {
    // P-Number to module mapping
    if let Some(pnum) = p_number {
        let pnum_lower = pnum.to_lowercase();
        // Orin series
        if pnum_lower.contains("3737") {
            return Some("Jetson AGX Orin".to_string());
        } else if pnum_lower.contains("3767") {
            return Some("Jetson Orin NX".to_string());
        } else if pnum_lower.contains("3768") {
            return Some("Jetson Orin Nano".to_string());
        }
        // Xavier series
        else if pnum_lower.contains("2888") {
            return Some("Jetson AGX Xavier".to_string());
        } else if pnum_lower.contains("3668") {
            return Some("Jetson Xavier NX".to_string());
        }
        // TX2 series
        else if pnum_lower.contains("3310") {
            return Some("Jetson TX2".to_string());
        } else if pnum_lower.contains("3489") {
            return Some("Jetson TX2 NX".to_string());
        } else if pnum_lower.contains("3636") {
            return Some("Jetson TX2i".to_string());
        }
        // TX1/Nano
        else if pnum_lower.contains("2180") {
            return Some("Jetson TX1".to_string());
        } else if pnum_lower.contains("3448") {
            return Some("Jetson Nano".to_string());
        }
    }

    // The model-name fallback used to guess a module from a product name, and was
    // dropped. Only the P-Number, which was read, is looked up.
    None
}

/// Parse `(major, revision)` from an L4T line such as
/// `# R36 (release), REVISION: 4.0, GCID: ...`.
///
/// Both parts must be present. A line with no `REVISION:` used to fall back to
/// revision 0, and an unrecognised one to a `JetPack 4.x` label.
fn parse_l4t(l4t: &str) -> Option<(u32, u32)> {
    let lower = l4t.to_lowercase();
    let major: u32 = lower
        .split_whitespace()
        .find_map(|word| word.strip_prefix('r'))?
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()?;
    let revision: u32 = lower
        .split_once("revision:")?
        .1
        .trim_start()
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some((major, revision))
}

/// Map L4T version to JetPack version
fn map_l4t_to_jetpack(l4t: &str) -> Option<String> {
    let (major, minor) = parse_l4t(l4t)?;

    // L4T to JetPack mapping
    match (major, minor) {
        // JetPack 6.x (Orin)
        (36, m) if m >= 4 => Some("JetPack 6.1".to_string()),
        (36, 3) => Some("JetPack 6.0 GA".to_string()),
        (36, 2) => Some("JetPack 6.0 DP".to_string()),
        // JetPack 5.x (Orin, Xavier)
        (35, m) if m >= 5 => Some("JetPack 5.1.3".to_string()),
        (35, 4) => Some("JetPack 5.1.2".to_string()),
        (35, 3) => Some("JetPack 5.1.1".to_string()),
        (35, 2) => Some("JetPack 5.1".to_string()),
        (35, 1) => Some("JetPack 5.0.2".to_string()),
        (35, 0) => Some("JetPack 5.0.1".to_string()),
        (34, _) => Some("JetPack 5.0 DP".to_string()),
        // JetPack 4.x (TX1, TX2, Nano, Xavier)
        (32, 7) => Some("JetPack 4.6.4".to_string()),
        (32, 6) => Some("JetPack 4.6.1".to_string()),
        (32, 5) => Some("JetPack 4.5.1".to_string()),
        (32, 4) => Some("JetPack 4.4.1".to_string()),
        (32, 3) => Some("JetPack 4.3".to_string()),
        // Any other revision is unmapped. This was labelled "JetPack 4.x".
        _ => None,
    }
}

fn detect_library_versions() -> Result<LibraryVersions> {
    let other = HashMap::new();

    // Try to detect CUDA version
    let cuda = detect_cuda_version();

    // Try to detect cuDNN
    let cudnn = detect_cudnn_version();

    // Try to detect TensorRT
    let tensorrt = detect_tensorrt_version();

    Ok(LibraryVersions {
        cuda,
        cudnn,
        tensorrt,
        other,
    })
}

fn detect_cuda_version() -> Option<String> {
    // Try reading from nvcc
    if let Ok(output) = std::process::Command::new("nvcc").arg("--version").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("release") {
                    return Some(line.to_string());
                }
            }
        }
    }

    // Try reading version.txt
    if let Ok(version) = fs::read_to_string("/usr/local/cuda/version.txt") {
        return Some(version.trim().to_string());
    }

    None
}

/// The version of the first installed (`ii`) package whose name starts with
/// `package_prefix`, from `dpkg -l` output.
///
/// A row is `ii  name  version  arch  description`. The row used to be stored
/// whole, so the "version" was the entire line.
fn dpkg_installed_version(dpkg_output: &str, package_prefix: &str) -> Option<String> {
    dpkg_output.lines().find_map(|line| {
        let mut columns = line.split_whitespace();
        let status = columns.next()?;
        let name = columns.next()?;
        let version = columns.next()?;
        (status == "ii" && name.starts_with(package_prefix)).then(|| version.to_string())
    })
}

fn dpkg_version(glob: &str, package_prefix: &str) -> Option<String> {
    let output = std::process::Command::new("dpkg")
        .args(["-l", glob])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    dpkg_installed_version(&String::from_utf8_lossy(&output.stdout), package_prefix)
}

fn detect_cudnn_version() -> Option<String> {
    dpkg_version("libcudnn*", "libcudnn")
}

fn detect_tensorrt_version() -> Option<String> {
    dpkg_version("libnvinfer*", "libnvinfer")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpkg_row_yields_the_version_column() {
        let output = concat!(
            "Desired=Unknown/Install/Remove/Purge/Hold\n",
            "| Status=Not/Inst/Conf-files/Unpacked/halF-conf/Half-inst/trig-aWait/Trig-pend\n",
            "||/ Name           Version              Architecture Description\n",
            "+++-==============-====================-============-=================================\n",
            "ii  libcudnn8      8.6.0.163-1+cuda11.8 arm64        cuDNN runtime libraries\n",
            "ii  libnvinfer8    8.5.2-1+cuda11.4     arm64        TensorRT runtime libraries\n",
        );
        assert_eq!(
            dpkg_installed_version(output, "libcudnn").as_deref(),
            Some("8.6.0.163-1+cuda11.8")
        );
        assert_eq!(
            dpkg_installed_version(output, "libnvinfer").as_deref(),
            Some("8.5.2-1+cuda11.4")
        );
        assert_eq!(dpkg_installed_version(output, "libcublas"), None);
    }

    #[test]
    fn removed_packages_are_not_installed() {
        let output = "rc  libcudnn8  8.4.0 arm64  cuDNN runtime libraries\n";
        assert_eq!(dpkg_installed_version(output, "libcudnn"), None);
    }

    #[test]
    fn l4t_line_maps_to_jetpack_only_when_fully_parsed() {
        let release = "# R35 (release), REVISION: 4.1, GCID: 33958178, BOARD: t186ref";
        assert_eq!(
            map_l4t_to_jetpack(release).as_deref(),
            Some("JetPack 5.1.2")
        );
        assert_eq!(
            map_l4t_to_jetpack("# R32 (release), REVISION: 7.4, GCID: 1").as_deref(),
            Some("JetPack 4.6.4")
        );
        // No revision, and an unknown release: neither is labelled.
        assert_eq!(map_l4t_to_jetpack("# R32 (release)"), None);
        assert_eq!(map_l4t_to_jetpack("# R32 (release), REVISION: 9.0"), None);
        assert_eq!(map_l4t_to_jetpack("# R99 (release), REVISION: 1.0"), None);
    }
}
