// S.M.A.R.T. (Self-Monitoring, Analysis and Reporting Technology) for storage temps
//
// This module reads temperature from HDDs and SSDs using the SMART protocol
// via DeviceIoControl on Windows or /sys/class/block on Linux

use super::{HwSensor, HwSensorType, HwType};

#[cfg(target_os = "windows")]
use windows::core::PCWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Ioctl::IOCTL_STORAGE_QUERY_PROPERTY;
#[cfg(target_os = "windows")]
use windows::Win32::System::IO::DeviceIoControl;

/// SMART attribute IDs
#[allow(dead_code)]
const SMART_ATTR_TEMPERATURE: u8 = 194; // Temperature (most drives)
#[allow(dead_code)]
const SMART_ATTR_AIRFLOW_TEMP: u8 = 190; // Airflow Temperature
#[allow(dead_code)]
const SMART_ATTR_TEMPERATURE_2: u8 = 231; // Temperature (SSD alternate)
#[allow(dead_code)]
const SMART_ATTR_DRIVE_TEMP: u8 = 0xBE; // Drive Temperature (190)

/// ATA SMART IOCTL codes
#[cfg(target_os = "windows")]
const SMART_RCV_DRIVE_DATA: u32 = 0x0007C088;
#[cfg(target_os = "windows")]
#[allow(dead_code)]
const IOCTL_ATA_PASS_THROUGH: u32 = 0x0004D02C;

/// NVMe IOCTL codes  
#[cfg(target_os = "windows")]
#[allow(dead_code)]
const IOCTL_STORAGE_QUERY_PROPERTY_NVME: u32 = 0x002D1400;
#[cfg(target_os = "windows")]
#[allow(dead_code)]
const IOCTL_SCSI_MINIPORT: u32 = 0x0004D008;

/// Read temperatures from all storage devices
pub fn read_storage_temperatures() -> Vec<HwSensor> {
    let mut sensors = Vec::new();

    #[cfg(target_os = "windows")]
    {
        sensors.extend(read_windows_storage_temps());
    }

    #[cfg(target_os = "linux")]
    {
        sensors.extend(read_linux_storage_temps());
    }

    sensors
}

#[cfg(target_os = "windows")]
fn read_windows_storage_temps() -> Vec<HwSensor> {
    let mut sensors = Vec::new();

    // Try physical drives 0-15
    for drive_num in 0..16 {
        let drive_path = format!("\\\\.\\PhysicalDrive{}", drive_num);

        if let Some(temp) = read_smart_temperature(&drive_path, drive_num) {
            sensors.push(temp);
        }
    }

    // Also try NVMe drives
    for drive_num in 0..8 {
        if let Some(temp) = read_nvme_temperature(drive_num) {
            // Only add if we don't already have this drive
            let _name = format!("NVMe {}", drive_num);
            if !sensors
                .iter()
                .any(|s| s.name.contains(&format!("Drive {}", drive_num)))
            {
                sensors.push(temp);
            }
        }
    }

    sensors
}

#[cfg(target_os = "windows")]
fn read_smart_temperature(drive_path: &str, drive_num: u32) -> Option<HwSensor> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    unsafe {
        // Open the drive
        let path: Vec<u16> = OsStr::new(drive_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let handle = CreateFileW(
            PCWSTR::from_raw(path.as_ptr()),
            0x80000000 | 0x40000000, // GENERIC_READ | GENERIC_WRITE
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        );

        let handle = match handle {
            Ok(h) if h != INVALID_HANDLE_VALUE => h,
            _ => return None,
        };

        // SMART READ DATA command structure
        #[repr(C, packed)]
        #[allow(non_snake_case)]
        struct SendCmdInParams {
            buffer_size: u32,
            irDriveRegs: IdeRegs,
            drive_number: u8,
            reserved: [u8; 3],
            reserved2: [u32; 4],
            buffer: [u8; 1],
        }

        #[repr(C, packed)]
        struct IdeRegs {
            features: u8,
            sector_count: u8,
            sector_number: u8,
            cyl_low: u8,
            cyl_high: u8,
            drive_head: u8,
            command: u8,
            reserved: u8,
        }

        // Prepare SMART READ DATA command
        let mut cmd_in = vec![0u8; 32 + 512];
        let cmd = cmd_in.as_mut_ptr() as *mut SendCmdInParams;
        (*cmd).buffer_size = 512;
        (*cmd).irDriveRegs.features = 0xD0; // SMART READ DATA
        (*cmd).irDriveRegs.sector_count = 1;
        (*cmd).irDriveRegs.sector_number = 1;
        (*cmd).irDriveRegs.cyl_low = 0x4F;
        (*cmd).irDriveRegs.cyl_high = 0xC2;
        (*cmd).irDriveRegs.drive_head = 0xA0 | ((drive_num as u8 & 1) << 4);
        (*cmd).irDriveRegs.command = 0xB0; // SMART command
        (*cmd).drive_number = drive_num as u8;

        let mut out_buffer = vec![0u8; 32 + 512];
        let mut bytes_returned: u32 = 0;

        let result = DeviceIoControl(
            handle,
            SMART_RCV_DRIVE_DATA,
            Some(cmd_in.as_ptr() as *const _),
            cmd_in.len() as u32,
            Some(out_buffer.as_mut_ptr() as *mut _),
            out_buffer.len() as u32,
            Some(&mut bytes_returned),
            None,
        );

        let _close_result: std::result::Result<(), _> = CloseHandle(handle);

        if result.is_err() || bytes_returned < 48 {
            return None;
        }

        // `SMART_RCV_DRIVE_DATA` replies with `SENDCMDOUTPARAMS`, whose payload
        // begins after `cBufferSize` and a `DRIVERSTATUS` --- sixteen bytes, not
        // the thirty-two of the *input* struct. The attribute walk below used to
        // start at `32 + 2`, borrowing the input layout for the output, so every
        // attribute id it read came from sixteen bytes into the neighbouring
        // entry. Nothing caught it because this machine has no SATA drive and
        // the path never runs here.
        //
        // The offset is taken from a declared struct rather than written as a
        // number, so the compiler checks it against the layout instead of a
        // reader checking it against my arithmetic.
        #[repr(C)]
        struct DriverStatus {
            driver_error: u8,
            ide_error: u8,
            reserved: [u8; 2],
            reserved2: [u32; 2],
        }

        #[repr(C)]
        struct SendCmdOutParams {
            buffer_size: u32,
            driver_status: DriverStatus,
            buffer: [u8; 1],
        }

        const PAYLOAD_OFFSET: usize = std::mem::offset_of!(SendCmdOutParams, buffer);

        // Sixteen is what ntdddisk.h gives: a `ULONG` then a twelve-byte
        // `DRIVERSTATUS`. This pins the struct above against that number, so a
        // later edit to a field cannot shift the payload without failing to
        // compile. It does not confirm my reading of the header --- nothing here
        // can, since no SATA drive on this machine reaches this code. What it
        // buys is that the offset stops being a literal somebody has to re-derive.
        const _: () = assert!(
            PAYLOAD_OFFSET == 16,
            "SENDCMDOUTPARAMS payload follows cBufferSize and DRIVERSTATUS"
        );

        let payload = out_buffer
            .get(PAYLOAD_OFFSET..PAYLOAD_OFFSET + crate::disk::ata_smart::STRUCTURE_LEN)?;

        // Parsed by `disk::ata_smart`, which walks the same table this used to
        // walk by hand --- but from ACS-4 offsets, and refusing a structure whose
        // checksum does not validate. Two parsers for one 512-byte layout is how
        // one of them stays wrong; the hand-rolled one is gone.
        let smart = crate::disk::ata_smart::AtaSmartData::parse(payload)?;
        let celsius = smart.temperature_celsius()?;

        let drive_name =
            get_drive_model(drive_path).unwrap_or_else(|| format!("Drive {}", drive_num));

        Some(HwSensor {
            name: drive_name,
            value: celsius as f32,
            min: None,
            max: None,
            sensor_type: HwSensorType::Temperature,
            hardware_type: HwType::Storage,
        })
    }
}

#[cfg(target_os = "windows")]
fn read_nvme_temperature(drive_num: u32) -> Option<HwSensor> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    // NVMe drives expose temperature via STORAGE_PROPERTY_QUERY
    // with StorageAdapterProtocolSpecificProperty

    let drive_path = format!("\\\\.\\PhysicalDrive{}", drive_num);

    unsafe {
        let path: Vec<u16> = OsStr::new(&drive_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let handle = CreateFileW(
            PCWSTR::from_raw(path.as_ptr()),
            0x80000000, // GENERIC_READ
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        );

        let handle = match handle {
            Ok(h) if h != INVALID_HANDLE_VALUE => h,
            _ => return None,
        };

        // Query for NVMe SMART/Health information
        // StorageDeviceProtocolSpecificProperty = 49
        #[repr(C)]
        #[allow(dead_code)]
        struct StorageProtocolSpecificData {
            protocol_type: u32,
            data_type: u32,
            protocol_data_offset: u32,
            protocol_data_length: u32,
            fixed_protocol_data: [u8; 4],
        }

        #[repr(C)]
        #[allow(dead_code)]
        struct NvmeHealthInfoLog {
            critical_warning: u8,
            temperature: [u8; 2], // Composite temperature in Kelvin
            available_spare: u8,
            available_spare_threshold: u8,
            percentage_used: u8,
            // ... more fields
        }

        // For now, try to get temperature from Storage Property Query
        #[repr(C)]
        struct StoragePropertyQuery {
            property_id: u32,
            query_type: u32,
            additional_parameters: [u8; 1],
        }

        let query = StoragePropertyQuery {
            property_id: 0, // StorageDeviceProperty
            query_type: 0,  // PropertyStandardQuery
            additional_parameters: [0],
        };

        let mut buffer = vec![0u8; 4096];
        let mut bytes_returned: u32 = 0;

        let _result = DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const _ as *const _),
            std::mem::size_of::<StoragePropertyQuery>() as u32,
            Some(buffer.as_mut_ptr() as *mut _),
            buffer.len() as u32,
            Some(&mut bytes_returned),
            None,
        );

        let _close_result: std::result::Result<(), _> = CloseHandle(handle);

        // NVMe temperature would need protocol-specific query
        // This basic query doesn't return temp, but helps identify NVMe drives

        None
    }
}

#[cfg(target_os = "windows")]
fn get_drive_model(drive_path: &str) -> Option<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    unsafe {
        let path: Vec<u16> = OsStr::new(drive_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let handle = CreateFileW(
            PCWSTR::from_raw(path.as_ptr()),
            0, // No access needed for query
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        );

        let handle = match handle {
            Ok(h) if h != INVALID_HANDLE_VALUE => h,
            _ => return None,
        };

        #[repr(C)]
        struct StoragePropertyQuery {
            property_id: u32,
            query_type: u32,
            additional_parameters: [u8; 1],
        }

        let query = StoragePropertyQuery {
            property_id: 0, // StorageDeviceProperty
            query_type: 0,  // PropertyStandardQuery
            additional_parameters: [0],
        };

        let mut buffer = vec![0u8; 1024];
        let mut bytes_returned: u32 = 0;

        let result = DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const _ as *const _),
            std::mem::size_of::<StoragePropertyQuery>() as u32,
            Some(buffer.as_mut_ptr() as *mut _),
            buffer.len() as u32,
            Some(&mut bytes_returned),
            None,
        );

        let _close_result: std::result::Result<(), _> = CloseHandle(handle);

        if result.is_err() {
            return None;
        }

        // Parse STORAGE_DEVICE_DESCRIPTOR
        // Version (4) + Size (4) + DeviceType (1) + ... + ProductIdOffset (4)
        if bytes_returned >= 40 {
            let product_id_offset =
                u32::from_le_bytes([buffer[36], buffer[37], buffer[38], buffer[39]]) as usize;

            if product_id_offset > 0 && product_id_offset < buffer.len() {
                // Find null terminator
                let end = buffer[product_id_offset..]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(40);

                let model =
                    String::from_utf8_lossy(&buffer[product_id_offset..product_id_offset + end])
                        .trim()
                        .to_string();

                if !model.is_empty() {
                    return Some(model);
                }
            }
        }

        None
    }
}

/// Label for a drive's temperature sensor.
///
/// The model is the label when it was read. When it was not, the label says so
/// and names the block device, which identifies the drive without claiming to
/// be its model. The block device name used to stand in for the model
/// unannounced, so it read like a model string.
#[cfg(any(target_os = "linux", test))]
fn storage_sensor_label(model: Option<&str>, device_node: &str) -> String {
    match model.map(str::trim).filter(|m| !m.is_empty()) {
        Some(model) => model.to_string(),
        None => format!("{device_node} (model not read)"),
    }
}

#[cfg(target_os = "linux")]
fn read_linux_storage_temps() -> Vec<HwSensor> {
    use std::fs;
    use std::path::Path;

    let mut sensors = Vec::new();

    // Method 1: Try hwmon (some NVMe drives expose temp here)
    let hwmon_path = Path::new("/sys/class/hwmon");
    if let Ok(entries) = fs::read_dir(hwmon_path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let name = fs::read_to_string(path.join("name"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();

            if name.contains("nvme") || name.contains("drivetemp") {
                if let Ok(temp_str) = fs::read_to_string(path.join("temp1_input")) {
                    if let Ok(temp_millicelsius) = temp_str.trim().parse::<i32>() {
                        sensors.push(HwSensor {
                            name: name.clone(),
                            value: temp_millicelsius as f32 / 1000.0,
                            min: None,
                            max: None,
                            sensor_type: HwSensorType::Temperature,
                            hardware_type: HwType::Storage,
                        });
                    }
                }
            }
        }
    }

    // Method 2: Try drivetemp kernel module paths
    let block_path = Path::new("/sys/class/block");
    if let Ok(entries) = fs::read_dir(block_path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();

            // Only check full disks (sda, nvme0n1), not partitions
            if name.starts_with("sd") && name.len() == 3
                || name.starts_with("nvme") && name.contains("n1") && !name.contains("p")
            {
                // Check for hwmon device.
                //
                // This used `glob::glob`, but `glob` is not a dependency of this
                // crate and never has been — the call could not compile. The
                // pattern is a single directory level with a fixed prefix, which
                // `read_dir` expresses directly and without adding a dependency for
                // one call site.
                let hwmon_dir = entry.path().join("device/hwmon");
                let first_hwmon = std::fs::read_dir(&hwmon_dir).ok().and_then(|mut dir| {
                    dir.find_map(|e| {
                        let e = e.ok()?;
                        e.file_name()
                            .to_string_lossy()
                            .starts_with("hwmon")
                            .then(|| e.path())
                    })
                });
                {
                    if let Some(hwmon_path) = first_hwmon {
                        if let Ok(temp_str) = fs::read_to_string(hwmon_path.join("temp1_input")) {
                            if let Ok(temp_mc) = temp_str.trim().parse::<i32>() {
                                let model =
                                    fs::read_to_string(entry.path().join("device/model")).ok();

                                sensors.push(HwSensor {
                                    name: storage_sensor_label(model.as_deref(), &name),
                                    value: temp_mc as f32 / 1000.0,
                                    min: None,
                                    max: None,
                                    sensor_type: HwSensorType::Temperature,
                                    hardware_type: HwType::Storage,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    sensors
}

#[cfg(test)]
mod tests {
    use super::storage_sensor_label;

    #[test]
    fn a_read_model_is_the_label() {
        assert_eq!(
            storage_sensor_label(Some("Samsung SSD 990 PRO 2TB\n"), "nvme0n1"),
            "Samsung SSD 990 PRO 2TB"
        );
    }

    #[test]
    fn an_unread_model_is_announced_not_replaced_by_the_device_node() {
        assert_eq!(
            storage_sensor_label(None, "nvme0n1"),
            "nvme0n1 (model not read)"
        );
    }

    #[test]
    fn a_blank_model_counts_as_unread() {
        assert_eq!(
            storage_sensor_label(Some("  \n"), "sda"),
            "sda (model not read)"
        );
    }
}
