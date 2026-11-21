//! Device fingerprint and device profile, sent once per `ironmon serve` session
//! under the opt-in [`ConsentScope::DeviceProfile`] scope.
//!
//! Follows the nervosys client contract, `otel/DEVICE-FINGERPRINT.md` in
//! nervosys_v2: identity signals (hardware UUIDs and serials, machine ids, MAC
//! addresses) each under its own attribute, and a profile of the hardware, OS,
//! runtimes and user account. Values are sent raw. **This includes the user's
//! account name, full name and home directory**, which is why the scope is off
//! until the user agrees to it ([`crate::consent`]).
//!
//! Nothing is escalated: a value the process cannot read is left out, never
//! guessed. Identity values that are vendor placeholders (contract step 4) are
//! still sent: the server disqualifies them from identity matching itself, and
//! [`usable_identity`] applies the same rule here. Numeric fields are sent as
//! integers ([`is_integer_field`]), so every client stores the same JSON type.
//!
//! [`ConsentScope::DeviceProfile`]: crate::consent::ConsentScope::DeviceProfile

/// Attribute name and value, in the order collected.
pub type Attributes = Vec<(String, String)>;

/// Identity values vendors ship as constants (contract step 4), compared after
/// normalisation.
pub const PLACEHOLDERS: &[&str] = &[
    "0",
    "none",
    "null",
    "n/a",
    "na",
    "default string",
    "to be filled by o.e.m.",
    "to be filled by oem",
    "system serial number",
    "not specified",
    "not applicable",
    "unknown",
    "invalid",
    "123456789",
    "0123456789",
    "000000000",
    "00000000-0000-0000-0000-000000000000",
    "ffffffff-ffff-ffff-ffff-ffffffffffff",
];

/// Contract steps 1-3: trim, lowercase, collapse internal whitespace.
pub fn normalize(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Contract step 4: the raw value (trimmed) when it can identify a machine;
/// `None` when it normalises to nothing or to a placeholder.
pub fn usable_identity(raw: &str) -> Option<String> {
    let normalized = normalize(raw);
    if normalized.is_empty() || PLACEHOLDERS.contains(&normalized.as_str()) {
        None
    } else {
        Some(raw.trim().to_string())
    }
}

/// MAC addresses in the contract's form: lowercase, colon-separated, sorted,
/// de-duplicated and comma-joined. All-zero and malformed addresses are dropped.
pub fn mac_list(macs: impl IntoIterator<Item = String>) -> Option<String> {
    let mut out: Vec<String> = macs
        .into_iter()
        .filter_map(|m| {
            let hex: String = m.chars().filter(|c| c.is_ascii_hexdigit()).collect();
            if hex.len() != 12 || hex.chars().all(|c| c == '0') {
                return None;
            }
            let hex = hex.to_lowercase();
            Some(
                (0..6)
                    .map(|i| &hex[i * 2..i * 2 + 2])
                    .collect::<Vec<_>>()
                    .join(":"),
            )
        })
        .collect();
    out.sort();
    out.dedup();
    (!out.is_empty()).then(|| out.join(","))
}

/// Fields the contract requires as OTLP `intValue` rather than strings.
pub fn is_integer_field(key: &str) -> bool {
    const FIXED: [&str; 9] = [
        "device.cpu_cores_physical",
        "device.cpu_cores_logical",
        "device.cpu_freq_max_mhz",
        "device.ram_total_bytes",
        "device.ram_available_bytes",
        "device.ram_speed_mhz",
        "device.ram_modules",
        "device.gpu_count",
        "device.display_count",
    ];
    if FIXED.contains(&key) || key == "device.battery_capacity_wh" {
        return true;
    }
    let parts: Vec<&str> = key.split('.').collect();
    match parts.as_slice() {
        ["device", "gpu", n, "vram_bytes"]
        | ["device", "disk", n, "size_bytes" | "free_bytes"]
        | ["device", "display", n, "width" | "height" | "refresh_hz"] => n.parse::<u32>().is_ok(),
        _ => false,
    }
}

/// `0x41` -> `65`; decimal passes through. `cpu_sig` is decimal throughout.
pub fn decimal(field: &str) -> String {
    let f = field.trim();
    match f.strip_prefix("0x").or_else(|| f.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).map_or_else(|_| f.to_string(), |n| n.to_string()),
        None => f.to_string(),
    }
}

/// A year, month and day that form a plausible calendar date. Kept loose on
/// purpose: a BIOS date only has to be recognisable, not validated to the day.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn valid_ymd(y: u32, m: u32, d: u32) -> bool {
    (1970..=2100).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d)
}

/// A WMI/CIM datetime (`yyyymmddHHMMSS.ffffff±UTC`) to `YYYY-MM-DD`, or None
/// when the leading eight digits are not a plausible date.
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[allow(dead_code)] // used on one platform; compiled on both for test coverage
fn cim_date_to_iso(raw: &str) -> Option<String> {
    let d = raw.trim();
    if d.len() < 8 || !d.as_bytes()[..8].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let year: u32 = d[0..4].parse().ok()?;
    let month: u32 = d[4..6].parse().ok()?;
    let day: u32 = d[6..8].parse().ok()?;
    valid_ymd(year, month, day).then(|| format!("{year:04}-{month:02}-{day:02}"))
}

/// An SMBIOS/DMI `bios_date` (`MM/DD/YYYY`) to `YYYY-MM-DD`.
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[allow(dead_code)] // used on one platform; compiled on both for test coverage
fn mdy_date_to_iso(raw: &str) -> Option<String> {
    let mut parts = raw.trim().split('/');
    let month: u32 = parts.next()?.trim().parse().ok()?;
    let day: u32 = parts.next()?.trim().parse().ok()?;
    let year: u32 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    valid_ymd(year, month, day).then(|| format!("{year:04}-{month:02}-{day:02}"))
}

/// The value of a UEFI `SecureBoot` EFI variable: a four-byte attribute prefix
/// then a one-byte value, enabled when that byte is 1.
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[allow(dead_code)] // used on one platform; compiled on both for test coverage
fn secure_boot_byte(data: &[u8]) -> Option<bool> {
    data.get(4).map(|b| *b == 1)
}

/// Interface or adapter names that are virtual, per the contract's list.
pub fn is_virtual_adapter(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        "docker",
        "veth",
        "vmnet",
        "vboxnet",
        "virtualbox",
        "vmware",
        "hyper-v",
        "vethernet",
        "wsl",
        "tap",
        "tun",
        "virbr",
        "br-",
        "loopback",
        "teredo",
        "isatap",
        "wireguard",
        "zerotier",
        "tailscale",
        "npcap",
        "wan miniport",
        "virtual",
    ]
    .iter()
    .any(|p| n.contains(p))
        || n == "lo"
}

#[derive(Default)]
struct Builder {
    out: Attributes,
}

impl Builder {
    /// An identity signal, sent raw (trimmed) when anything was read. A
    /// placeholder is sent too; the server disqualifies it from matching.
    fn identity(&mut self, key: &str, raw: Option<String>) {
        if let Some(v) = raw.map(|r| r.trim().to_string()).filter(|r| !r.is_empty()) {
            self.out.push((key.to_string(), v));
        }
    }

    /// A profile value, dropped if empty.
    fn put(&mut self, key: impl Into<String>, value: Option<impl ToString>) {
        if let Some(v) = value {
            let v = v.to_string();
            let v = v.trim();
            if !v.is_empty() {
                self.out.push((key.into(), v.to_string()));
            }
        }
    }

    fn has(&self, key: &str) -> bool {
        self.out.iter().any(|(k, _)| k == key)
    }
}

/// Collect everything this process can read. Slow-ish (WMI on Windows, a local
/// Ollama probe everywhere), so call it once per session from a background
/// thread.
pub fn collect() -> Attributes {
    let mut b = Builder::default();
    platform::collect(&mut b);
    common(&mut b);
    b.out
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Fields read the same way on every platform, filled where the platform
/// reader left them out.
fn common(b: &mut Builder) {
    b.put("device.cpu_arch", Some(std::env::consts::ARCH));
    if !b.has("device.cpu_cores_logical") {
        b.put(
            "device.cpu_cores_logical",
            std::thread::available_parallelism().ok().map(|n| n.get()),
        );
    }
    b.put("device.cpu_features", cpu_features());
    b.put("device.os_type", Some(std::env::consts::OS));
    b.put("device.locale", crate::telemetry::locale());
    if !b.has("device.timezone") {
        b.put("device.timezone", env("TZ"));
    }
    if !b.has("device.hostname") {
        b.put(
            "device.hostname",
            env("COMPUTERNAME").or_else(|| env("HOSTNAME")),
        );
    }
    b.put("device.username", env("USERNAME").or_else(|| env("USER")));
    b.put(
        "device.home_dir",
        env("USERPROFILE").or_else(|| env("HOME")),
    );
    b.put("device.shell", env("SHELL").or_else(|| env("ComSpec")));
    let terminal = env("TERM_PROGRAM")
        .or_else(|| env("WT_SESSION").map(|_| "windows-terminal".to_string()))
        .or_else(|| env("TERM"));
    b.put("device.terminal", terminal);
    ollama(b);
}

/// Instruction-set extensions that decide inference code paths.
fn cpu_features() -> Option<String> {
    #[allow(unused_mut)]
    let mut found: Vec<&str> = Vec::new();
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        macro_rules! probe {
            ($($f:tt),*) => { $( if std::arch::is_x86_feature_detected!($f) { found.push($f); } )* };
        }
        probe!(
            "sse4.2",
            "avx",
            "avx2",
            "fma",
            "f16c",
            "aes",
            "sha",
            "avx512f",
            "avx512bw",
            "avx512vl",
            "avx512vnni",
            "avx512bf16",
            "avx512fp16"
        );
        // AMX detection is not yet stable in `is_x86_feature_detected!`, so it
        // is read from CPUID leaf 7: EDX bits 22 (bf16), 24 (tile), 25 (int8).
        #[cfg(target_arch = "x86_64")]
        {
            // `__cpuid_count` is safe on newer toolchains and unsafe on older ones.
            #[allow(unused_unsafe)]
            let edx = unsafe { std::arch::x86_64::__cpuid_count(7, 0) }.edx;
            for (bit, name) in [(24, "amx-tile"), (22, "amx-bf16"), (25, "amx-int8")] {
                if edx & (1 << bit) != 0 {
                    found.push(name);
                }
            }
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        macro_rules! probe {
            ($($f:tt),*) => { $( if std::arch::is_aarch64_feature_detected!($f) { found.push($f); } )* };
        }
        probe!("neon", "sve", "sve2", "dotprod", "fp16", "i8mm", "bf16", "sha2", "aes");
    }
    (!found.is_empty()).then(|| found.join(","))
}

/// The local Ollama's version and pulled model names (names only), from its
/// loopback API. Nothing is sent anywhere but 127.0.0.1 here.
fn ollama(b: &mut Builder) {
    #[cfg(feature = "remote-backends")]
    {
        let Ok(client) = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .redirect(reqwest::redirect::Policy::none())
            .build()
        else {
            return;
        };
        let get = |path: &str| -> Option<serde_json::Value> {
            let response = client
                .get(format!("http://127.0.0.1:11434{path}"))
                .send()
                .ok()?;
            if !response.status().is_success() {
                return None;
            }
            response.json().ok()
        };
        let Some(version) = get("/api/version") else {
            return;
        };
        b.put(
            "device.ollama_version",
            version.get("version").and_then(|v| v.as_str()),
        );
        let models = get("/api/tags").and_then(|tags| {
            let names: Vec<String> = tags
                .get("models")?
                .as_array()?
                .iter()
                .filter_map(|m| m.get("name")?.as_str().map(str::to_string))
                .collect();
            (!names.is_empty()).then(|| names.join(","))
        });
        b.put("device.ollama_models", models);
    }
    #[cfg(not(feature = "remote-backends"))]
    let _ = b;
}

// ---------------------------------------------------------------- Windows ---

#[cfg(target_os = "windows")]
mod platform {
    use super::{mac_list, Builder};
    use std::collections::HashMap;
    use wmi::{COMLibrary, Variant, WMIConnection};

    type Row = HashMap<String, Variant>;

    fn connect(namespace: Option<&str>) -> Option<WMIConnection> {
        let open = |com: COMLibrary| match namespace {
            Some(ns) => WMIConnection::with_namespace_path(ns, com).ok(),
            None => WMIConnection::new(com).ok(),
        };
        COMLibrary::new()
            .ok()
            .and_then(open)
            .or_else(|| COMLibrary::without_security().ok().and_then(open))
            .or_else(|| open(unsafe { COMLibrary::assume_initialized() }))
    }

    fn rows(conn: &WMIConnection, query: &str) -> Vec<Row> {
        conn.raw_query::<Row>(query).unwrap_or_default()
    }

    fn first(conn: &WMIConnection, query: &str) -> Row {
        rows(conn, query).into_iter().next().unwrap_or_default()
    }

    fn text(row: &Row, key: &str) -> Option<String> {
        match row.get(key)? {
            Variant::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
            other => number(other).map(|n| n.to_string()),
        }
    }

    fn number(v: &Variant) -> Option<u64> {
        match v {
            Variant::UI1(n) => Some(u64::from(*n)),
            Variant::UI2(n) => Some(u64::from(*n)),
            Variant::UI4(n) => Some(u64::from(*n)),
            Variant::UI8(n) => Some(*n),
            Variant::I1(n) => u64::try_from(*n).ok(),
            Variant::I2(n) => u64::try_from(*n).ok(),
            Variant::I4(n) => u64::try_from(*n).ok(),
            Variant::I8(n) => u64::try_from(*n).ok(),
            Variant::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    fn num(row: &Row, key: &str) -> Option<u64> {
        number(row.get(key)?)
    }

    fn flag(row: &Row, key: &str) -> Option<bool> {
        match row.get(key)? {
            Variant::Bool(b) => Some(*b),
            _ => None,
        }
    }

    fn escape(s: &str) -> String {
        s.replace('\\', "\\\\").replace('\'', "\\'")
    }

    pub(super) fn collect(b: &mut Builder) {
        let Some(conn) = connect(None) else {
            return;
        };

        // Identity.
        let product = first(&conn, "SELECT UUID FROM Win32_ComputerSystemProduct");
        b.identity("device.smbios_uuid", text(&product, "UUID"));
        b.identity("device.machine_guid", machine_guid());
        let board = first(&conn, "SELECT SerialNumber FROM Win32_BaseBoard");
        b.identity("device.board_serial", text(&board, "SerialNumber"));
        let bios = first(&conn, "SELECT SerialNumber, ReleaseDate FROM Win32_BIOS");
        b.identity("device.bios_serial", text(&bios, "SerialNumber"));
        b.identity("device.disk_serial", system_disk_serial(&conn));
        let cpus = rows(
            &conn,
            "SELECT ProcessorId, Manufacturer, Name, NumberOfCores, \
             NumberOfLogicalProcessors, MaxClockSpeed FROM Win32_Processor",
        );
        let cpu = cpus.first().cloned().unwrap_or_default();
        b.identity("device.cpu_id", text(&cpu, "ProcessorId"));
        let macs = rows(
            &conn,
            "SELECT Name, MACAddress, PNPDeviceID FROM Win32_NetworkAdapter \
             WHERE PhysicalAdapter = TRUE AND MACAddress IS NOT NULL",
        )
        .into_iter()
        // PCI-attached only: USB tethering, USB dongles and Bluetooth PAN come
        // and go, and an adapter that moves between machines poisons matching.
        .filter(|r| {
            let name = text(r, "Name").unwrap_or_default();
            let pnp = text(r, "PNPDeviceID").unwrap_or_default().to_uppercase();
            !super::is_virtual_adapter(&name) && pnp.starts_with("PCI\\")
        })
        .filter_map(|r| text(&r, "MACAddress"));
        b.put("device.mac_addrs", mac_list(macs));

        // CPU.
        b.put("device.cpu_vendor", text(&cpu, "Manufacturer"));
        b.put("device.cpu_model", text(&cpu, "Name"));
        let sum = |key: &str| -> Option<u64> {
            let values: Vec<u64> = cpus.iter().filter_map(|c| num(c, key)).collect();
            (!values.is_empty()).then(|| values.iter().sum())
        };
        b.put("device.cpu_cores_physical", sum("NumberOfCores"));
        b.put("device.cpu_cores_logical", sum("NumberOfLogicalProcessors"));
        b.put("device.cpu_freq_max_mhz", num(&cpu, "MaxClockSpeed"));

        // Memory.
        let system = first(
            &conn,
            "SELECT TotalPhysicalMemory, Manufacturer, Model, Domain, PartOfDomain, \
             Workgroup, DNSHostName FROM Win32_ComputerSystem",
        );
        b.put(
            "device.ram_total_bytes",
            num(&system, "TotalPhysicalMemory"),
        );
        let os = first(
            &conn,
            "SELECT Caption, Version, BuildNumber, FreePhysicalMemory FROM Win32_OperatingSystem",
        );
        b.put(
            "device.ram_available_bytes",
            num(&os, "FreePhysicalMemory").map(|kib| kib * 1024),
        );
        let modules = rows(
            &conn,
            "SELECT Speed, ConfiguredClockSpeed FROM Win32_PhysicalMemory",
        );
        b.put(
            "device.ram_speed_mhz",
            modules
                .iter()
                .filter_map(|m| num(m, "ConfiguredClockSpeed").or_else(|| num(m, "Speed")))
                .filter(|s| *s > 0)
                .max(),
        );
        if !modules.is_empty() {
            b.put("device.ram_modules", Some(modules.len()));
        }

        // GPUs.
        let gpus = rows(
            &conn,
            "SELECT Name, AdapterCompatibility, AdapterRAM, DriverVersion, PNPDeviceID \
             FROM Win32_VideoController",
        );
        // Zero adapters means none were visible, not that there are none.
        b.put("device.gpu_count", Some(gpus.len()).filter(|n| *n > 0));
        let pci_ids: Vec<Option<String>> = gpus
            .iter()
            .map(|g| text(g, "PNPDeviceID").as_deref().and_then(pci_id))
            .collect();
        for (i, gpu) in gpus.iter().enumerate() {
            let name = text(gpu, "Name");
            b.put(
                format!("device.gpu.{i}.vendor"),
                text(gpu, "AdapterCompatibility"),
            );
            b.put(format!("device.gpu.{i}.model"), name.clone());
            let vram = name.as_deref().and_then(registry_vram).or_else(|| {
                // AdapterRAM is 32-bit, so a saturated value means "4 GiB or more".
                num(gpu, "AdapterRAM").filter(|r| *r > 0 && *r < 0xFFF0_0000)
            });
            b.put(format!("device.gpu.{i}.vram_bytes"), vram);
            b.put(
                format!("device.gpu.{i}.driver_version"),
                text(gpu, "DriverVersion"),
            );
            b.put(format!("device.gpu.{i}.pci_id"), pci_ids[i].clone());
            if i == 0 {
                b.put("device.driver_version", text(gpu, "DriverVersion"));
            }
        }
        nvidia(b, &pci_ids);

        // Storage.
        if let Some(storage) = connect(Some("ROOT\\Microsoft\\Windows\\Storage")) {
            let disks = rows(
                &storage,
                "SELECT FriendlyName, Size, MediaType, BusType FROM MSFT_PhysicalDisk",
            );
            for (i, disk) in disks.iter().enumerate() {
                b.put(format!("device.disk.{i}.model"), text(disk, "FriendlyName"));
                b.put(format!("device.disk.{i}.size_bytes"), num(disk, "Size"));
                let kind = match (num(disk, "BusType"), num(disk, "MediaType")) {
                    (Some(17), _) => Some("nvme"),
                    (_, Some(4)) => Some("ssd"),
                    (_, Some(3)) => Some("hdd"),
                    _ => None,
                };
                b.put(format!("device.disk.{i}.kind"), kind);
            }
        }

        // Displays.
        let displays = displays();
        b.put("device.display_count", Some(displays.len()));
        for (i, (w, h, hz)) in displays.iter().enumerate() {
            b.put(format!("device.display.{i}.width"), Some(w));
            b.put(format!("device.display.{i}.height"), Some(h));
            b.put(format!("device.display.{i}.refresh_hz"), Some(hz));
        }

        // Chassis, power and virtualisation.
        let manufacturer = text(&system, "Manufacturer").unwrap_or_default();
        let model = text(&system, "Model").unwrap_or_default();
        let virt = virt_type(&manufacturer, &model);
        b.put("device.virt_type", Some(virt));
        if virt != "none" {
            b.put("device.virt_vendor", Some(manufacturer.clone()));
        }
        let enclosure = first(&conn, "SELECT ChassisTypes FROM Win32_SystemEnclosure");
        let chassis = if virt != "none" {
            Some("vm")
        } else {
            match enclosure.get("ChassisTypes") {
                Some(Variant::Array(types)) => types.iter().filter_map(number).find_map(chassis),
                _ => None,
            }
        };
        b.put("device.chassis", chassis);
        let batteries = rows(&conn, "SELECT DeviceID FROM Win32_Battery");
        b.put("device.battery_present", Some(!batteries.is_empty()));
        if !batteries.is_empty() {
            if let Some(wmi) = connect(Some("ROOT\\WMI")) {
                let mwh: u64 = rows(
                    &wmi,
                    "SELECT FullChargedCapacity FROM BatteryFullChargedCapacity",
                )
                .iter()
                .filter_map(|r| num(r, "FullChargedCapacity"))
                .sum();
                if mwh > 0 {
                    b.put(
                        "device.battery_capacity_wh",
                        Some((mwh as f64 / 1000.0).round() as u64),
                    );
                }
            }
        }

        // OS.
        let caption = text(&os, "Caption");
        let version = text(&os, "Version");
        // Version only (`10.0.26200`); the edition goes in its own field.
        b.put("device.os_version", version.clone());
        b.put(
            "device.os_edition",
            caption.map(|c| c.trim_start_matches("Microsoft ").trim().to_string()),
        );
        b.put("device.kernel_version", version);
        b.put("device.timezone", crate::os_info::windows_timezone_key());
        b.put("device.ui_language", ui_language());

        // User and environment.
        b.put("device.hostname", text(&system, "DNSHostName"));
        if flag(&system, "PartOfDomain") == Some(true) {
            b.put("device.domain", text(&system, "Domain"));
        } else {
            b.put("device.workgroup", text(&system, "Workgroup"));
        }
        if let Some(user) = super::env("USERNAME") {
            let account = first(
                &conn,
                &format!(
                    "SELECT FullName FROM Win32_UserAccount WHERE LocalAccount = TRUE \
                     AND Name = '{}'",
                    escape(&user)
                ),
            );
            b.put("device.user_full_name", text(&account, "FullName"));
        }
        b.put("device.is_admin", is_elevated());

        // Firmware and security posture.
        b.put("device.firmware_type", firmware_type());
        b.put("device.secure_boot", secure_boot());
        b.put(
            "device.bios_release_date",
            text(&bios, "ReleaseDate").and_then(|d| super::cim_date_to_iso(&d)),
        );
    }

    /// UEFI or legacy BIOS, from the firmware's own report.
    fn firmware_type() -> Option<&'static str> {
        use windows::Win32::System::SystemInformation::{GetFirmwareType, FIRMWARE_TYPE};
        let mut kind = FIRMWARE_TYPE::default();
        unsafe { GetFirmwareType(&mut kind) }.ok()?;
        match kind.0 {
            1 => Some("bios"),
            2 => Some("uefi"),
            _ => None,
        }
    }

    /// Secure Boot state from its registry value, not the key's existence: a
    /// machine can carry the key with the feature off. Absent on legacy BIOS.
    fn secure_boot() -> Option<bool> {
        let key = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
            .open_subkey("SYSTEM\\CurrentControlSet\\Control\\SecureBoot\\State")
            .ok()?;
        key.get_value::<u32, _>("UEFISecureBootEnabled")
            .ok()
            .map(|v| v == 1)
    }

    fn machine_guid() -> Option<String> {
        let key = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
            .open_subkey("SOFTWARE\\Microsoft\\Cryptography")
            .ok()?;
        key.get_value::<String, _>("MachineGuid").ok()
    }

    /// The serial of the physical disk holding the system drive, through the
    /// logical disk -> partition -> drive associations.
    fn system_disk_serial(conn: &WMIConnection) -> Option<String> {
        let drive = super::env("SystemDrive").unwrap_or_else(|| "C:".into());
        let partition = rows(
            conn,
            &format!(
                "ASSOCIATORS OF {{Win32_LogicalDisk.DeviceID='{}'}} \
                 WHERE AssocClass = Win32_LogicalDiskToPartition",
                escape(&drive)
            ),
        )
        .into_iter()
        .find_map(|r| text(&r, "DeviceID"))?;
        rows(
            conn,
            &format!(
                "ASSOCIATORS OF {{Win32_DiskPartition.DeviceID='{}'}} \
                 WHERE AssocClass = Win32_DiskDriveToDiskPartition",
                escape(&partition)
            ),
        )
        .into_iter()
        .find_map(|r| text(&r, "SerialNumber"))
    }

    /// Dedicated video memory as the display driver records it (64-bit, unlike
    /// WMI's AdapterRAM), matched to the adapter by its description.
    fn registry_vram(name: &str) -> Option<u64> {
        let class = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
            .open_subkey(
                "SYSTEM\\CurrentControlSet\\Control\\Class\\\
                 {4d36e968-e325-11ce-bfc1-08002be10318}",
            )
            .ok()?;
        class.enum_keys().flatten().find_map(|sub| {
            let key = class.open_subkey(&sub).ok()?;
            let desc: String = key.get_value("DriverDesc").ok()?;
            if desc.trim() != name.trim() {
                return None;
            }
            let raw = key.get_raw_value("HardwareInformation.qwMemorySize").ok()?;
            let bytes: [u8; 8] = raw.bytes.get(..8)?.try_into().ok()?;
            Some(u64::from_le_bytes(bytes)).filter(|v| *v > 0)
        })
    }

    /// `PCI\VEN_10DE&DEV_2684&...` -> `10de:2684`.
    fn pci_id(pnp: &str) -> Option<String> {
        let upper = pnp.to_uppercase();
        let field = |tag: &str| {
            let at = upper.find(tag)? + tag.len();
            upper.get(at..at + 4).map(str::to_lowercase)
        };
        Some(format!("{}:{}", field("VEN_")?, field("DEV_")?))
    }

    /// CUDA version, and compute capability for each listed adapter whose PCI
    /// id NVML also reports (NVML's device order is not WMI's).
    #[cfg(feature = "nvidia")]
    fn nvidia(b: &mut Builder, pci_ids: &[Option<String>]) {
        let Ok(nvml) = nvml_wrapper::Nvml::init() else {
            return;
        };
        if let Ok(v) = nvml.sys_cuda_driver_version() {
            b.put(
                "device.cuda_version",
                Some(format!("{}.{}", v / 1000, (v % 1000) / 10)),
            );
        }
        let mut capability = std::collections::HashMap::new();
        for index in 0..nvml.device_count().unwrap_or(0) {
            let Ok(device) = nvml.device_by_index(index) else {
                continue;
            };
            let (Ok(pci), Ok(cc)) = (device.pci_info(), device.cuda_compute_capability()) else {
                continue;
            };
            // pci_device_id packs the device id above the vendor id.
            let id = pci.pci_device_id;
            let key = format!("{:04x}:{:04x}", id & 0xffff, id >> 16);
            capability.insert(key, format!("{}.{}", cc.major, cc.minor));
        }
        for (i, id) in pci_ids.iter().enumerate() {
            if let Some(cc) = id.as_ref().and_then(|id| capability.get(id)) {
                b.put(format!("device.gpu.{i}.compute_capability"), Some(cc));
            }
        }
    }

    #[cfg(not(feature = "nvidia"))]
    fn nvidia(_: &mut Builder, _: &[Option<String>]) {}

    /// Active displays as (width, height, refresh Hz).
    fn displays() -> Vec<(u32, u32, u32)> {
        use windows::core::PCWSTR;
        use windows::Win32::Graphics::Gdi::{
            EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW,
            DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, ENUM_CURRENT_SETTINGS,
        };
        let mut out = Vec::new();
        for index in 0.. {
            let mut device = DISPLAY_DEVICEW {
                cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                ..Default::default()
            };
            if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0) }.as_bool() {
                break;
            }
            if device.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP == 0 {
                continue;
            }
            let mut mode = DEVMODEW {
                dmSize: std::mem::size_of::<DEVMODEW>() as u16,
                ..Default::default()
            };
            let name = PCWSTR(device.DeviceName.as_ptr());
            if unsafe { EnumDisplaySettingsW(name, ENUM_CURRENT_SETTINGS, &mut mode) }.as_bool() {
                out.push((mode.dmPelsWidth, mode.dmPelsHeight, mode.dmDisplayFrequency));
            }
        }
        out
    }

    fn ui_language() -> Option<String> {
        use windows::Win32::Globalization::{GetUserDefaultUILanguage, LCIDToLocaleName};
        let lang = unsafe { GetUserDefaultUILanguage() };
        let mut buffer = [0u16; 85];
        let len = unsafe { LCIDToLocaleName(u32::from(lang), Some(&mut buffer), 0) };
        (len > 1).then(|| String::from_utf16_lossy(&buffer[..(len as usize - 1)]))
    }

    fn is_elevated() -> Option<bool> {
        use windows::Win32::Foundation::{CloseHandle, HANDLE};
        use windows::Win32::Security::{
            GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
        };
        use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
        let mut token = HANDLE::default();
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.ok()?;
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let result = unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                Some(std::ptr::addr_of_mut!(elevation).cast()),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut len,
            )
        };
        let _ = unsafe { CloseHandle(token) };
        result.ok()?;
        Some(elevation.TokenIsElevated != 0)
    }

    fn virt_type(manufacturer: &str, model: &str) -> &'static str {
        let m = manufacturer.to_lowercase();
        let model = model.to_lowercase();
        if m.contains("microsoft") && model.contains("virtual") {
            "hyperv"
        } else if m.contains("vmware") || model.contains("vmware") {
            "vmware"
        } else if model.contains("kvm") {
            "kvm"
        } else if m.contains("qemu") || model.contains("qemu") {
            "qemu"
        } else if model.contains("virtualbox") {
            "virtualbox"
        } else {
            "none"
        }
    }

    /// SMBIOS chassis type codes.
    fn chassis(code: u64) -> Option<&'static str> {
        match code {
            3..=7 | 13 | 15 | 16 | 24 | 35 | 36 => Some("desktop"),
            8..=12 | 14 | 18 | 21 | 30..=32 => Some("laptop"),
            17 | 23 | 25 | 28 | 29 => Some("server"),
            _ => None,
        }
    }
}

// ------------------------------------------------------------------ Linux ---

#[cfg(target_os = "linux")]
mod platform {
    use super::{env, mac_list, Builder};
    use std::fs;
    use std::path::Path;

    fn read(path: impl AsRef<Path>) -> Option<String> {
        fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn dmi(field: &str) -> Option<String> {
        read(format!("/sys/class/dmi/id/{field}"))
    }

    pub(super) fn collect(b: &mut Builder) {
        // Identity. The DMI serials are root-only; unreadable ones are left out.
        b.identity("device.product_uuid", dmi("product_uuid"));
        b.identity("device.product_serial", dmi("product_serial"));
        b.identity("device.board_serial", dmi("board_serial"));
        b.identity("device.chassis_serial", dmi("chassis_serial"));
        b.identity(
            "device.machine_id",
            read("/etc/machine-id").or_else(|| read("/var/lib/dbus/machine-id")),
        );
        let cpuinfo = read("/proc/cpuinfo").unwrap_or_default();
        let field = |name: &str| {
            cpuinfo.lines().find_map(|l| {
                let (k, v) = l.split_once(':')?;
                (k.trim() == name).then(|| v.trim().to_string())
            })
        };
        // x86: vendor family model stepping. aarch64: implementer architecture
        // variant part revision. Decimal, single spaces, first processor block.
        let x86 = ["vendor_id", "cpu family", "model", "stepping"];
        let arm = [
            "CPU implementer",
            "CPU architecture",
            "CPU variant",
            "CPU part",
            "CPU revision",
        ];
        let sig = [&x86[..], &arm[..]].into_iter().find_map(|names| {
            let values: Option<Vec<String>> = names
                .iter()
                .map(|n| field(n).map(|v| super::decimal(&v)))
                .collect();
            values.map(|v| v.join(" "))
        });
        b.identity("device.cpu_sig", sig);
        let macs = fs::read_dir("/sys/class/net")
            .into_iter()
            .flatten()
            .flatten()
            // PCI-attached only, as on Windows: USB and Bluetooth adapters move.
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let on_pci = fs::read_link(e.path().join("device/subsystem"))
                    .map(|p| p.ends_with("pci"))
                    .unwrap_or(false);
                on_pci && !super::is_virtual_adapter(&name)
            })
            .filter_map(|e| read(e.path().join("address")));
        b.put("device.mac_addrs", mac_list(macs));

        // CPU.
        b.put("device.cpu_vendor", field("vendor_id"));
        b.put("device.cpu_model", field("model name"));
        let mut cores = std::collections::HashSet::new();
        let (mut phys, mut core) = (None, None);
        for line in cpuinfo.lines() {
            if let Some((k, v)) = line.split_once(':') {
                match k.trim() {
                    "physical id" => phys = Some(v.trim().to_string()),
                    "core id" => core = Some(v.trim().to_string()),
                    _ => {}
                }
            }
            if line.trim().is_empty() {
                if let (Some(p), Some(c)) = (phys.take(), core.take()) {
                    cores.insert((p, c));
                }
            }
        }
        if !cores.is_empty() {
            b.put("device.cpu_cores_physical", Some(cores.len()));
        }
        b.put(
            "device.cpu_freq_max_mhz",
            read("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq")
                .and_then(|k| k.parse::<u64>().ok())
                .map(|khz| khz / 1000),
        );

        // Memory.
        let meminfo = read("/proc/meminfo").unwrap_or_default();
        let mem = |name: &str| {
            meminfo.lines().find_map(|l| {
                let rest = l.strip_prefix(name)?.strip_prefix(':')?;
                rest.split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
                    .map(|k| k * 1024)
            })
        };
        b.put("device.ram_total_bytes", mem("MemTotal"));
        b.put("device.ram_available_bytes", mem("MemAvailable"));

        // GPUs, from DRM.
        let mut gpu = 0;
        let mut cards: Vec<_> = fs::read_dir("/sys/class/drm")
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("card") && !n.contains('-'))
            .collect();
        cards.sort();
        for card in cards {
            let dev = Path::new("/sys/class/drm").join(&card).join("device");
            let vendor = read(dev.join("vendor"));
            let device = read(dev.join("device"));
            let (Some(v), Some(d)) = (vendor, device) else {
                continue;
            };
            let v = v.trim_start_matches("0x").to_lowercase();
            let d = d.trim_start_matches("0x").to_lowercase();
            let name = match v.as_str() {
                "10de" => "NVIDIA",
                "1002" => "AMD",
                "8086" => "Intel",
                _ => "",
            };
            b.put(format!("device.gpu.{gpu}.vendor"), Some(name));
            b.put(format!("device.gpu.{gpu}.pci_id"), Some(format!("{v}:{d}")));
            b.put(
                format!("device.gpu.{gpu}.vram_bytes"),
                read(dev.join("mem_info_vram_total")),
            );
            gpu += 1;
        }
        // No DRM cards means none were visible (WSL2 exposes its GPUs through
        // /dev/dxg instead), not that there are none, so zero is not sent.
        b.put("device.gpu_count", Some(gpu).filter(|n| *n > 0));
        b.put("device.rocm_version", read("/opt/rocm/.info/version"));

        // Firmware and security posture.
        b.put(
            "device.firmware_type",
            Some(if Path::new("/sys/firmware/efi").is_dir() {
                "uefi"
            } else {
                "bios"
            }),
        );
        b.put("device.secure_boot", secure_boot());
        b.put(
            "device.bios_release_date",
            dmi("bios_date").and_then(|d| super::mdy_date_to_iso(&d)),
        );

        // Storage.
        let mut disks: Vec<_> = fs::read_dir("/sys/block")
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| !n.starts_with("loop") && !n.starts_with("ram") && !n.starts_with("zram"))
            .collect();
        disks.sort();
        for (i, name) in disks.iter().enumerate() {
            let base = Path::new("/sys/block").join(name);
            b.put(
                format!("device.disk.{i}.model"),
                read(base.join("device/model")),
            );
            b.put(
                format!("device.disk.{i}.size_bytes"),
                read(base.join("size"))
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|sectors| sectors * 512),
            );
            let kind = if name.starts_with("nvme") {
                Some("nvme")
            } else {
                match read(base.join("queue/rotational")).as_deref() {
                    Some("0") => Some("ssd"),
                    Some("1") => Some("hdd"),
                    _ => None,
                }
            };
            b.put(format!("device.disk.{i}.kind"), kind);
        }

        // Chassis, power, virtualisation.
        let virt = virt_type();
        b.put("device.virt_type", Some(virt));
        if virt != "none" {
            b.put("device.virt_vendor", dmi("sys_vendor"));
        }
        let chassis = if virt != "none" {
            Some("vm")
        } else {
            dmi("chassis_type")
                .and_then(|c| c.parse::<u64>().ok())
                .and_then(|c| match c {
                    3..=7 | 13 | 15 | 16 | 24 | 35 | 36 => Some("desktop"),
                    8..=12 | 14 | 18 | 21 | 30..=32 => Some("laptop"),
                    17 | 23 | 25 | 28 | 29 => Some("server"),
                    _ => None,
                })
        };
        b.put("device.chassis", chassis);
        let batteries: Vec<_> = fs::read_dir("/sys/class/power_supply")
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| read(e.path().join("type")).as_deref() == Some("Battery"))
            .collect();
        b.put("device.battery_present", Some(!batteries.is_empty()));
        let uwh: u64 = batteries
            .iter()
            .filter_map(|e| read(e.path().join("energy_full")))
            .filter_map(|v| v.parse::<u64>().ok())
            .sum();
        if uwh > 0 {
            b.put(
                "device.battery_capacity_wh",
                Some((uwh as f64 / 1_000_000.0).round() as u64),
            );
        }

        // OS.
        let release = read("/etc/os-release").unwrap_or_default();
        let os_field = |name: &str| {
            release.lines().find_map(|l| {
                l.strip_prefix(name)
                    .and_then(|v| v.strip_prefix('='))
                    .map(|v| v.trim_matches('"').to_string())
            })
        };
        b.put("device.os_version", os_field("VERSION_ID"));
        b.put("device.os_edition", os_field("PRETTY_NAME"));
        b.put("device.kernel_version", read("/proc/sys/kernel/osrelease"));
        b.put(
            "device.timezone",
            env("TZ").or_else(|| read("/etc/timezone")).or_else(|| {
                fs::read_link("/etc/localtime").ok().and_then(|p| {
                    let s = p.to_string_lossy().to_string();
                    s.split_once("zoneinfo/").map(|(_, z)| z.to_string())
                })
            }),
        );
        b.put(
            "device.ui_language",
            env("LANGUAGE").or_else(|| env("LANG")),
        );

        // User.
        b.put("device.hostname", read("/proc/sys/kernel/hostname"));
        b.put(
            "device.domain",
            read("/proc/sys/kernel/domainname").filter(|d| d != "(none)"),
        );
        let uid = unsafe { libc::geteuid() };
        b.put("device.is_admin", Some(uid == 0));
        let passwd = read("/etc/passwd").unwrap_or_default();
        let full_name = passwd.lines().find_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            (f.len() > 4 && f[2] == uid.to_string())
                .then(|| f[4].split(',').next().unwrap_or("").to_string())
        });
        b.put("device.user_full_name", full_name);
    }

    /// Secure Boot from the EFI variable. Its name ends in a fixed GUID. Absent
    /// on legacy BIOS, and the file is often root-only; either way it is left out.
    fn secure_boot() -> Option<bool> {
        let data = fs::read(
            "/sys/firmware/efi/efivars/\
             SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c",
        )
        .ok()?;
        super::secure_boot_byte(&data)
    }

    fn virt_type() -> &'static str {
        let version = read("/proc/version").unwrap_or_default().to_lowercase();
        if version.contains("microsoft") || version.contains("wsl") {
            return "wsl2";
        }
        if Path::new("/.dockerenv").exists() {
            return "docker";
        }
        let environ = fs::read("/proc/1/environ").unwrap_or_default();
        if String::from_utf8_lossy(&environ).contains("container=lxc") {
            return "lxc";
        }
        let vendor = format!(
            "{} {}",
            dmi("sys_vendor").unwrap_or_default(),
            dmi("product_name").unwrap_or_default()
        )
        .to_lowercase();
        if vendor.contains("vmware") {
            "vmware"
        } else if vendor.contains("microsoft") && vendor.contains("virtual") {
            "hyperv"
        } else if vendor.contains("kvm") {
            "kvm"
        } else if vendor.contains("qemu") {
            "qemu"
        } else {
            "none"
        }
    }
}

// ------------------------------------------------------------------ macOS ---

#[cfg(target_os = "macos")]
mod platform {
    use super::{mac_list, Builder};
    use std::process::Command;

    fn run(cmd: &str, args: &[&str]) -> Option<String> {
        let out = Command::new(cmd).args(args).output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn sysctl(name: &str) -> Option<String> {
        run("/usr/sbin/sysctl", &["-n", name])
    }

    /// `"Key" = "value"` or `"Key" = <"value">` from `ioreg -l` output.
    fn ioreg_value(text: &str, key: &str) -> Option<String> {
        let needle = format!("\"{key}\" = ");
        let line = text.lines().find(|l| l.contains(&needle))?;
        let value = line.split_once(&needle)?.1;
        Some(
            value
                .trim_matches(|c| c == '<' || c == '>' || c == '"' || c == ' ')
                .to_string(),
        )
    }

    pub(super) fn collect(b: &mut Builder) {
        let platform =
            run("/usr/sbin/ioreg", &["-rd1", "-c", "IOPlatformExpertDevice"]).unwrap_or_default();
        b.identity(
            "device.platform_uuid",
            ioreg_value(&platform, "IOPlatformUUID"),
        );
        b.identity(
            "device.platform_serial",
            ioreg_value(&platform, "IOPlatformSerialNumber"),
        );
        b.identity(
            "device.board_id",
            ioreg_value(&platform, "board-id").or_else(|| ioreg_value(&platform, "target-type")),
        );
        b.identity("device.hw_model", sysctl("hw.model"));
        let macs = run("/sbin/ifconfig", &[])
            .unwrap_or_default()
            .split("\n")
            .scan(String::new(), |iface, line| {
                if !line.starts_with('\t') && !line.starts_with(' ') {
                    *iface = line.split(':').next().unwrap_or("").to_string();
                }
                Some((iface.clone(), line.trim().to_string()))
            })
            .filter(|(iface, line)| iface.starts_with("en") && line.starts_with("ether "))
            .filter_map(|(_, line)| line.split_whitespace().nth(1).map(str::to_string))
            .collect::<Vec<_>>();
        b.put("device.mac_addrs", mac_list(macs));

        b.put(
            "device.cpu_vendor",
            sysctl("machdep.cpu.vendor").or(Some("Apple".into())),
        );
        b.put("device.cpu_model", sysctl("machdep.cpu.brand_string"));
        b.put("device.cpu_cores_physical", sysctl("hw.physicalcpu"));
        b.put("device.cpu_cores_logical", sysctl("hw.logicalcpu"));
        b.put(
            "device.cpu_freq_max_mhz",
            sysctl("hw.cpufrequency_max")
                .and_then(|h| h.parse::<u64>().ok())
                .map(|hz| hz / 1_000_000),
        );
        b.put("device.ram_total_bytes", sysctl("hw.memsize"));
        b.put(
            "device.chassis",
            Some(if sysctl("hw.model").unwrap_or_default().contains("Book") {
                "laptop"
            } else {
                "desktop"
            }),
        );
        let product = run("/usr/bin/sw_vers", &["-productVersion"]);
        b.put("device.os_version", product.clone());
        b.put(
            "device.os_edition",
            product.map(|v| format!("macOS {}", v.split('.').next().unwrap_or(&v))),
        );
        b.put("device.kernel_version", sysctl("kern.osrelease"));
        b.put("device.hostname", sysctl("kern.hostname"));
        b.put("device.user_full_name", run("/usr/bin/id", &["-F"]));
        b.put("device.is_admin", Some(unsafe { libc::geteuid() } == 0));
        b.put(
            "device.virt_type",
            Some(if sysctl("kern.hv_vmm_present").as_deref() == Some("1") {
                "vm"
            } else {
                "none"
            }),
        );
        b.put(
            "device.timezone",
            std::fs::read_link("/etc/localtime").ok().and_then(|p| {
                let s = p.to_string_lossy().to_string();
                s.split_once("zoneinfo/").map(|(_, z)| z.to_string())
            }),
        );
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod platform {
    pub(super) fn collect(_: &mut super::Builder) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_dropped_and_real_values_kept_raw() {
        for p in [
            "To Be Filled By O.E.M.",
            "  Default   String ",
            "00000000-0000-0000-0000-000000000000",
            "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF",
            "",
            "   ",
            "N/A",
        ] {
            assert_eq!(usable_identity(p), None, "{p:?} should be dropped");
        }
        assert_eq!(
            usable_identity("  4C4C4544-0042-3510-8051-B4C04F4B3732 "),
            Some("4C4C4544-0042-3510-8051-B4C04F4B3732".to_string())
        );
    }

    #[test]
    fn placeholders_are_still_sent() {
        let mut b = Builder::default();
        b.identity("device.bios_serial", Some(" Default string ".into()));
        b.identity("device.board_serial", Some("   ".into()));
        assert_eq!(
            b.out,
            vec![(
                "device.bios_serial".to_string(),
                "Default string".to_string()
            )]
        );
    }

    #[test]
    fn the_contract_integer_fields_are_recognised() {
        for k in [
            "device.ram_total_bytes",
            "device.gpu.2.vram_bytes",
            "device.disk.0.size_bytes",
            "device.display.1.refresh_hz",
            "device.battery_capacity_wh",
        ] {
            assert!(is_integer_field(k), "{k}");
        }
        for k in [
            "device.gpu.0.model",
            "device.gpu.x.vram_bytes",
            "device.os_version",
        ] {
            assert!(!is_integer_field(k), "{k}");
        }
    }

    #[test]
    fn cpu_sig_fields_are_decimal() {
        assert_eq!(decimal("0x41"), "65");
        assert_eq!(decimal("0xd0c"), "3340");
        assert_eq!(decimal(" 151 "), "151");
    }

    #[test]
    fn integer_fields_encode_as_int_values() {
        let body = crate::otlp::encode_with(
            &[],
            0,
            &[
                ("device.ram_total_bytes", "100547727360".to_string()),
                ("device.os_version", "10.0.26200".to_string()),
            ],
        );
        let attrs = body["resourceMetrics"][0]["resource"]["attributes"]
            .as_array()
            .unwrap();
        let get = |k: &str| attrs.iter().find(|a| a["key"] == k).unwrap()["value"].clone();
        assert_eq!(get("device.ram_total_bytes")["intValue"], "100547727360");
        assert_eq!(get("device.os_version")["stringValue"], "10.0.26200");
    }

    #[test]
    fn normalisation_matches_the_contract() {
        assert_eq!(normalize("  Default \t  STRING  "), "default string");
    }

    #[test]
    fn macs_are_sorted_lowercase_colon_joined() {
        let list = mac_list(vec![
            "AA-BB-CC-DD-EE-FF".to_string(),
            "00:11:22:33:44:55".to_string(),
            "00:00:00:00:00:00".to_string(),
            "aa:bb:cc:dd:ee:ff".to_string(),
            "bogus".to_string(),
        ]);
        assert_eq!(list.as_deref(), Some("00:11:22:33:44:55,aa:bb:cc:dd:ee:ff"));
    }

    #[test]
    fn virtual_adapters_are_recognised() {
        for v in [
            "vEthernet (WSL)",
            "docker0",
            "veth12ab",
            "VMware Network Adapter VMnet8",
            "lo",
        ] {
            assert!(is_virtual_adapter(v), "{v}");
        }
        for p in [
            "Intel(R) Ethernet Controller I225-V",
            "eth0",
            "wlp3s0",
            "en0",
        ] {
            assert!(!is_virtual_adapter(p), "{p}");
        }
    }

    #[test]
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn cim_dates_parse_to_iso() {
        assert_eq!(
            cim_date_to_iso("20231005000000.000000+000").as_deref(),
            Some("2023-10-05")
        );
        assert_eq!(cim_date_to_iso("20200229").as_deref(), Some("2020-02-29"));
        assert_eq!(cim_date_to_iso("2023130500000000"), None); // month 13
        assert_eq!(cim_date_to_iso("not-a-date"), None);
        assert_eq!(cim_date_to_iso("2023"), None); // too short
    }

    #[test]
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn mdy_dates_parse_to_iso() {
        assert_eq!(mdy_date_to_iso("10/05/2023").as_deref(), Some("2023-10-05"));
        assert_eq!(mdy_date_to_iso("1/2/2020").as_deref(), Some("2020-01-02"));
        assert_eq!(mdy_date_to_iso("2023-10-05"), None); // wrong separator
        assert_eq!(mdy_date_to_iso("10/05/2023/00"), None); // extra field
        assert_eq!(mdy_date_to_iso("13/05/2023"), None); // month 13
    }

    #[test]
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn secure_boot_byte_reads_the_value() {
        assert_eq!(secure_boot_byte(&[6, 0, 0, 0, 1]), Some(true));
        assert_eq!(secure_boot_byte(&[6, 0, 0, 0, 0]), Some(false));
        assert_eq!(secure_boot_byte(&[6, 0, 0, 0]), None); // no value byte
        assert_eq!(secure_boot_byte(&[]), None);
    }
}
