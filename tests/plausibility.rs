//! Physical plausibility of reported hardware readings.
//!
//! # Why this exists
//!
//! The unit tests assert that collectors do not panic and that types round-trip.
//! Almost none assert that the *numbers* make physical sense, and that is precisely
//! where this codebase's defects have clustered. Every one of the following shipped
//! and passed a green suite:
//!
//! - A GPU at 80 °C rendered as `0.1 °C`, with warning thresholds that could never
//!   fire, because a degrees value was divided by 1000.
//! - `cli cpu` reported 0% usage / 100% idle on every one-shot invocation, printed
//!   directly above per-core rows showing 23–53%.
//! - Per-core CPU was the system-wide average replicated across all 24 cores.
//! - Memory readings of "32 GB total, 16 GB used" were invented as a fallback.
//! - A display named "Primary Display" was fabricated whenever detection failed.
//! - Windows fan RPM was hardcoded to 1000.
//!
//! Each is invisible to a type checker and to a test that only asks "did it return
//! Ok?". They are visible the moment you ask whether the value could be true of real
//! hardware.
//!
//! # Design rule: conditional strictness
//!
//! These run on developer machines with real hardware and on CI containers with
//! almost none. So the rule throughout is **"if a subsystem reports something, that
//! something must be physically possible"** — never "the subsystem must report
//! something". Absent hardware is not a failure; impossible readings are.
//!
//! A consequence worth stating: these tests cannot prove a reading is *correct*, only
//! that it is not absurd. They are a floor, not a guarantee.

use std::time::{Duration, Instant};

use ironmonlib::pipeline::{Collector, CollectorConfig};

/// Longest a collector may take to publish a fully-populated snapshot.
///
/// Generous because GPU driver enumeration dominates cold start and CI runners are
/// slow; this bounds the test, it is not a performance assertion.
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(90);

/// Whether this platform has readers that produce live hardware values.
///
/// This gates assertions that a *snapshot* carries readings. macOS gained CPU,
/// memory and uptime readers in 3.1.0, but `IronMonitor::snapshot` requires every reader
/// to succeed and GPU, power and temperature are still unimplemented there, so a
/// snapshot never populates. The macOS readers that do exist are asserted directly
/// in `tests/macos_readers.rs` instead.
///
/// This names the gap once rather than repeating a cfg at each site.
fn platform_has_hardware_readers() -> bool {
    cfg!(any(target_os = "linux", target_os = "windows"))
}

/// Spawn a collector and wait for a snapshot with real data.
///
/// Returns `None` when nothing populated in time, so callers can skip rather than
/// fail on a machine where collection is unavailable.
fn populated_snapshot() -> Option<std::sync::Arc<ironmonlib::pipeline::Snapshot>> {
    let collector = Collector::spawn(CollectorConfig {
        interval: Duration::from_millis(250),
        ..Default::default()
    });
    let handle = collector.handle();

    let deadline = Instant::now() + SNAPSHOT_TIMEOUT;
    while Instant::now() < deadline {
        let snap = handle.latest();
        // Generation 0 is the not-ready placeholder; generation 1 is the warm-up
        // publish which deliberately has no GPU or process data yet.
        if snap.generation > 1 && snap.cpu.is_some() {
            return Some(snap);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    None
}

#[test]
fn cpu_readings_are_physically_possible() {
    let Some(snap) = populated_snapshot() else {
        eprintln!("skipping: no snapshot available");
        return;
    };
    let Some(cpu) = snap.cpu.as_ref() else {
        eprintln!("skipping: no CPU data");
        return;
    };

    let total = &cpu.total;
    for (label, value) in [
        ("user", total.user),
        ("system", total.system),
        ("idle", total.idle),
    ] {
        assert!(
            (0.0..=100.0).contains(&value),
            "aggregate {label} is {value}%, outside 0-100"
        );
        assert!(!value.is_nan(), "aggregate {label} is NaN");
    }

    // These three are shares of one total, so together they cannot exceed it —
    // that is the invariant worth asserting, and it catches double-counting or a
    // wrong denominator on any platform.
    //
    // They are *not* required to reach 100. This used to demand ~100 and passed
    // only because it had never run anywhere but Windows, where user/system/idle
    // are the whole story. Linux divides by the sum of every field in
    // `/proc/stat`, so nice, iowait, irq, softirq and steal are real time that
    // belongs to none of these three. A virtualized CI runner has enough steal and
    // iowait to put the total at 94%, which is an accurate reading, not a fault.
    let sum = total.user + total.system + total.idle;
    assert!(
        sum <= 100.5,
        "aggregate user+system+idle = {sum}%, which exceeds the whole"
    );
    // A reading of exactly zero across all three means nothing was measured, which
    // is the honest answer where no CPU reader exists.
    if platform_has_hardware_readers() {
        assert!(
            sum > 0.0,
            "aggregate user+system+idle is 0% — no CPU time was accounted for at all"
        );
    }

    // Per-core figures must be per-core, not one number replicated. Identical
    // cumulative values across every core is the signature of the system aggregate
    // being copied into each slot.
    let busy: Vec<f32> = cpu
        .cores
        .iter()
        .filter_map(|c| c.idle.map(|idle| 100.0 - idle))
        .collect();

    for (i, value) in busy.iter().enumerate() {
        assert!(
            (0.0..=100.0).contains(value),
            "core {i} busy is {value}%, outside 0-100"
        );
    }

    // The aggregate must not claim the machine is idle while cores report load.
    if !busy.is_empty() {
        let mean = busy.iter().sum::<f32>() / busy.len() as f32;
        let aggregate_busy = 100.0 - total.idle;
        assert!(
            !(aggregate_busy < 1.0 && mean > 10.0),
            "aggregate reports {aggregate_busy:.1}% busy while cores average \
             {mean:.1}% — the aggregate is a placeholder, not a measurement"
        );
    }
}

#[test]
fn memory_readings_are_self_consistent() {
    let Some(snap) = populated_snapshot() else {
        eprintln!("skipping: no snapshot available");
        return;
    };
    let Some(mem) = snap.memory.as_ref() else {
        eprintln!("skipping: no memory data");
        return;
    };

    if platform_has_hardware_readers() {
        assert!(mem.ram.total > 0, "reported zero total RAM");
    }
    assert!(
        mem.ram.used <= mem.ram.total,
        "used RAM {} exceeds total {}",
        mem.ram.used,
        mem.ram.total
    );
    // Only comparable when the platform reported both. `None` is not a small
    // number: a machine that reported no swap figures at all has nothing here to
    // be implausible about, and asserting over `unwrap_or(0)` would have made
    // this pass for the wrong reason.
    if let (Some(used), Some(total)) = (mem.swap.used, mem.swap.total) {
        assert!(
            used <= total || total == 0,
            "used swap {used} exceeds total {total}"
        );
    }

    // Guard against the specific fabricated fallback that used to ship: exactly
    // 32 GiB total with exactly 16 GiB used, in kilobytes.
    let ram_kb_32gib = 32u64 * 1024 * 1024;
    let ram_kb_16gib = 16u64 * 1024 * 1024;
    assert!(
        !(mem.ram.total == ram_kb_32gib && mem.ram.used == ram_kb_16gib),
        "memory reads exactly 32GiB/16GiB — this is the synthetic fallback that was \
         removed, not a measurement"
    );

    let usage = mem.ram_usage_percent();
    assert!(
        (0.0..=100.0).contains(&usage),
        "RAM usage {usage}% outside 0-100"
    );
}

#[test]
fn gpu_readings_are_physically_possible() {
    let Some(snap) = populated_snapshot() else {
        eprintln!("skipping: no snapshot available");
        return;
    };
    if snap.gpu_dynamic.is_empty() {
        eprintln!("skipping: no GPUs detected");
        return;
    }

    for (i, gpu) in snap.gpu_dynamic.iter().enumerate() {
        let Some(gpu) = gpu.as_ref() else { continue };

        // Checked only where the device reports a counter. A percentage that
        // was never read cannot be out of range.
        if let Some(util) = gpu.utilization {
            assert!(util <= 100, "GPU {i} utilization {util}% exceeds 100");
        }
        // Checked only where both figures exist. The `|| total == 0` escape
        // used to be how this test tolerated an adapter that reported no
        // memory, which is to say the absence had to be smuggled through as a
        // zero here too.
        if let (Some(used), Some(total)) = (gpu.memory.used, gpu.memory.total) {
            assert!(
                used <= total,
                "GPU {i} memory used {used} exceeds total {total}"
            );
        }

        if let Some(temp) = gpu.thermal.temperature {
            // A powered GPU sits between roughly ambient and its thermal shutdown
            // point. Values far outside that indicate a unit error, which is exactly
            // how the degrees/millidegrees bug presented: 80 °C displayed as 0.1.
            assert!(
                (5..=125).contains(&temp),
                "GPU {i} temperature {temp}°C is not a plausible silicon temperature \
                 — suspect a unit conversion (degrees vs millidegrees)"
            );
        }

        if let Some(draw_mw) = gpu.power.draw {
            let watts = draw_mw as f32 / 1000.0;
            assert!(
                watts <= 2000.0,
                "GPU {i} draws {watts:.1}W, beyond any single-card power budget — \
                 suspect a unit error"
            );

            // A card pulling real power cannot also be at ambient temperature. This
            // is the cross-check that would have caught the 0.1 °C bug: 423 W was
            // reported alongside a temperature near zero.
            if watts > 50.0 {
                if let Some(temp) = gpu.thermal.temperature {
                    assert!(
                        temp > 15,
                        "GPU {i} draws {watts:.1}W but reports {temp}°C — a loaded \
                         card cannot be at ambient; suspect a scaling bug"
                    );
                }
            }
        }

        if let Some(fan_pct) = gpu.thermal.fan_speed {
            assert!(fan_pct <= 100, "GPU {i} fan {fan_pct}% exceeds 100");
        }
    }
}

#[test]
fn process_table_is_plausible() {
    let Some(snap) = populated_snapshot() else {
        eprintln!("skipping: no snapshot available");
        return;
    };
    if snap.processes.is_empty() {
        eprintln!("skipping: no process data");
        return;
    }

    // This test binary is running, so the OS has at least a handful of processes. A
    // listing in single digits means most were silently dropped — the failure mode
    // where every SYSTEM process was omitted because OpenProcess was denied.
    assert!(
        snap.processes.len() >= 10,
        "only {} processes reported; a live system has far more, so entries are \
         being dropped rather than reported",
        snap.processes.len()
    );

    for proc in &snap.processes {
        assert!(!proc.name.is_empty(), "process {} has no name", proc.pid);
        assert!(
            proc.cpu_percent >= 0.0 && proc.cpu_percent < 10_000.0,
            "process {} ({}) reports {}% CPU",
            proc.pid,
            proc.name,
            proc.cpu_percent
        );
    }

    // PIDs must be unique; duplicates mean the enumeration emitted a process twice,
    // which would double-count its resources in any aggregate.
    let mut pids: Vec<u32> = snap.processes.iter().map(|p| p.pid).collect();
    let before = pids.len();
    pids.sort_unstable();
    pids.dedup();
    assert_eq!(
        before,
        pids.len(),
        "the process table contains duplicate PIDs"
    );

    // State must be a code we actually mean, including 'U' for "this platform does
    // not tell us".
    //
    // The reader passes through whatever character `/proc/[pid]/stat` reports, so
    // this list has to be the set the kernel documents rather than the subset that
    // happens to show up. It was written on Windows and omitted `I` — idle kernel
    // thread, which Linux has reported since 3.13 and which every `kworker/R-*`
    // carries. The first Linux CI run that ever reached this test failed on PID 4.
    for proc in &snap.processes {
        assert!(
            matches!(
                proc.state,
                'R' | 'S' | 'D' | 'Z' | 'T' | 't' | 'W' | 'X' | 'x' | 'K' | 'P' | 'I' | 'U'
            ),
            "process {} ({}) reports unknown state {:?}",
            proc.pid,
            proc.name,
            proc.state
        );
    }

    // Windows has no per-process scheduling state — it lives on threads — and the
    // enumeration used to stamp 'R' on every entry, so the UI claimed several hundred
    // processes were running and none asleep on an idle desktop.
    #[cfg(windows)]
    assert!(
        !snap.processes.iter().all(|p| p.state == 'R'),
        "every one of the {} processes is reported as running, which is a default \
         rather than a reading",
        snap.processes.len()
    );
}

#[test]
fn disk_readings_are_self_consistent() {
    let Some(snap) = populated_snapshot() else {
        eprintln!("skipping: no snapshot available");
        return;
    };

    for disk in &snap.disks {
        assert!(
            !disk.name.is_empty(),
            "a disk was reported with no identifier"
        );
        assert!(
            disk.used <= disk.total || disk.total == 0,
            "disk {} used {} exceeds total {}",
            disk.name,
            disk.used,
            disk.total
        );
        assert!(
            disk.read_rate.unwrap_or(0.0) >= 0.0 && disk.write_rate.unwrap_or(0.0) >= 0.0,
            "disk {} reports a negative transfer rate",
            disk.name
        );
    }
}

#[test]
fn network_rates_are_non_negative() {
    let Some(snap) = populated_snapshot() else {
        eprintln!("skipping: no snapshot available");
        return;
    };

    for iface in &snap.network {
        assert!(!iface.name.is_empty(), "an interface has no name");
        // A rate is absent until a second sample establishes it; what must
        // never happen is a negative one.
        assert!(
            iface.rx_rate.unwrap_or(0.0) >= 0.0 && iface.tx_rate.unwrap_or(0.0) >= 0.0,
            "interface {} reports a negative rate (rx {:?}, tx {:?})",
            iface.name,
            iface.rx_rate,
            iface.tx_rate
        );
        assert!(
            iface.rx_rate.map(f64::is_finite).unwrap_or(true)
                && iface.tx_rate.map(f64::is_finite).unwrap_or(true),
            "interface {} reports a non-finite rate",
            iface.name
        );
    }
}

/// The same rule as the display test below, for the two readers found doing it.
///
/// That test exists because *displays* were where the pattern was first caught,
/// and it was never generalised. Two more readers were still inventing entries:
///
/// - `usb` pushed an Intel root hub — vendor `0x8086`, product `0x0001`, high
///   speed — when the sysfs walk found nothing.
/// - `audio` pushed a "Default Audio Output", active, enabled, unmuted, at 100%
///   volume, once per platform, when enumeration found nothing.
///
/// In both cases a machine whose hardware could not be enumerated reported one
/// working device, indistinguishable from a machine that has exactly one. An
/// empty list is the honest answer and it is what both return now.
///
/// This asserts the *signatures* rather than emptiness, because a real machine
/// legitimately has audio devices and USB devices; what it must never have is
/// one of these.
#[test]
fn readers_that_find_nothing_invent_nothing() {
    use ironmonlib::audio::AudioMonitor;
    use ironmonlib::usb::UsbMonitor;

    if let Ok(monitor) = UsbMonitor::new() {
        for device in monitor.devices() {
            let invented = device.vendor_id == Some(0x8086)
                && device.product_id == Some(0x0001)
                && device.product.as_deref() == Some("USB Root Hub");
            assert!(
                !invented,
                concat!(
                    "the synthetic Intel root hub is back: a USB walk that ",
                    "finds nothing must report nothing"
                )
            );
        }
    }

    if let Ok(monitor) = AudioMonitor::new() {
        for device in monitor.devices() {
            let invented = matches!(device.id.as_str(), "default_output" | "default")
                && matches!(
                    device.name.as_str(),
                    "Default Audio Output" | "Default Audio Device"
                );
            assert!(
                !invented,
                concat!(
                    "the synthetic audio endpoint is back ({}): enumeration ",
                    "that finds nothing must report nothing"
                ),
                device.name
            );

            // A volume that is read is a percentage. This does not catch the
            // original defect on its own -- 100 is in range -- which is what
            // the check after the loop is for.
            if let Some(volume) = device.volume {
                assert!(
                    volume <= 100,
                    "{}: volume {volume} is not a percentage",
                    device.name
                );
            }
        }

        // The original defect had a shape, and this is it: the default endpoint
        // was handed `Some(100)` while every other device got `None`, so the one
        // device a user actually looks at was the one carrying an invented
        // figure. Either a platform reads the mixer for the endpoints it
        // enumerates, or it reads none of them -- "only the default has a
        // volume, and it is exactly full" is not a state any mixer produces.
        let with_volume: Vec<&str> = monitor
            .devices()
            .iter()
            .filter(|d| d.volume.is_some())
            .map(|d| d.name.as_str())
            .collect();
        let defaults: Vec<&str> = monitor
            .devices()
            .iter()
            .filter(|d| d.is_default == Some(true))
            .map(|d| d.name.as_str())
            .collect();
        if !with_volume.is_empty() && with_volume == defaults {
            assert!(
                monitor
                    .devices()
                    .iter()
                    .any(|d| d.volume.is_some_and(|v| v != 100)),
                concat!(
                    "only the default endpoints report a volume and all of them ",
                    "read 100%: {:?}. That is the constructor default returning, ",
                    "not a mixer reading"
                ),
                with_volume
            );
        }
    }
}

/// Detection failures must report nothing, never a synthetic stand-in.
///
/// Collectors used to invent a plausible-looking entry when their probe came back
/// empty — a "Primary Display" with zero dimensions, a fan at exactly 1000 RPM. On
/// screen these are indistinguishable from measurements.
#[test]
fn absent_hardware_is_reported_as_absent_not_invented() {
    use ironmonlib::display::DisplayMonitor;

    let Ok(monitor) = DisplayMonitor::new() else {
        eprintln!("skipping: display monitor unavailable");
        return;
    };

    for display in monitor.displays() {
        assert!(!display.id.is_empty(), "a display was reported with no id");

        // Zero dimensions alone do *not* indicate fabrication: a genuinely detected
        // sink can report no mode. An AV receiver on DVI, or a monitor whose EDID
        // carries no timing block, shows up as a real device at 0x0.
        //
        // What identifies the removed placeholders is their whole signature — the
        // synthetic id `display0` paired with a generic name and no measurements at
        // all. Real entries carry an EDID-derived name ("RX-A740", "LG ULTRAWIDE")
        // even when modeless.
        let name = display.name.as_deref().unwrap_or_default();
        let generic_name = matches!(name, "Primary Display" | "Unknown Display" | "Display");
        // These were `== 0` when the fields were `u32`/`f32` and a zero was
        // how an unreadable mode was spelled. They are `Option` now, so the
        // signature is the absence itself.
        let no_measurements = display.width.is_none_or(|w| w == 0)
            && display.height.is_none_or(|h| h == 0)
            && display.refresh_rate.is_none_or(|r| r == 0.0)
            && display.physical_width_mm.is_none();

        assert!(
            !(display.id == "display0" && generic_name && no_measurements),
            "display {name:?} matches the synthetic placeholder signature (id \
             `display0`, generic name, no measurements) — detection failures must \
             report nothing rather than invent an entry"
        );

        // Whatever is reported must at least be identifiable as a specific device.
        assert!(
            !name.is_empty() || !no_measurements,
            "display {} has neither a name nor any measurement, so it identifies \
             nothing",
            display.id
        );
    }
}

/// `wmic` was removed from Windows in 11 24H2. Any reader that shells out to it
/// cannot fail loudly — the spawn error is indistinguishable from "no such data" —
/// so it silently degrades into reporting zeros and empty strings as measurements.
///
/// Seven call sites did exactly that: CPU model and clock, baseboard make and model,
/// system make and model, boot device, and pagefile paths. All were replaced with
/// registry or Win32 reads. This keeps them replaced.
#[test]
fn no_reader_depends_on_the_wmic_tool_windows_removed() {
    fn walk(dir: &std::path::Path, found: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for (n, line) in text.lines().enumerate() {
                    // Prose may name the tool to explain why it is gone; only an
                    // actual invocation is a defect.
                    if line.contains("Command::new(\"wmic\")") {
                        found.push(format!("{}:{}", path.display(), n + 1));
                    }
                }
            }
        }
    }

    let mut found = Vec::new();
    walk(std::path::Path::new("src"), &mut found);
    assert!(
        found.is_empty(),
        "these sites spawn `wmic`, which does not exist on Windows 11 24H2 or later, \
         so they report absent data as if measured: {found:?}"
    );
}

/// The CPU reader must produce a real model and a real clock, not the placeholders
/// it fell back to when its `wmic` subprocess failed to spawn.
#[test]
#[cfg(windows)]
fn windows_cpu_reader_reports_measured_identity_and_clock() {
    let Ok(stats) = ironmonlib::platform::windows::read_cpu_stats() else {
        return; // No CPU data at all is a separate failure, covered elsewhere.
    };
    let Some(core) = stats.cores.first() else {
        return;
    };

    assert_ne!(
        core.model, "Unknown CPU",
        "CPU model fell back to its placeholder; every Windows machine publishes \
         ProcessorNameString in the registry, so this means the reader failed"
    );

    if let Some(freq) = &core.frequency {
        // This said `current > 0 || max > 0` and warned, correctly, that
        // "absence must be `None`, not zero". The `||` let a zero `current`
        // through whenever `max` was known — which is exactly the sentinel the
        // Windows nominal-clock fix introduced, and why `ironmon cli cpu` printed
        // "Clock: 0 MHz" for a session before anyone read that line. The fields
        // are `Option` now, so the rule can be stated per field.
        assert!(
            freq.current.is_some() || freq.max.is_some() || freq.min.is_some(),
            concat!(
                "a frequency struct was reported with every field absent; it ",
                "should have been `None` in the first place"
            )
        );
        for (what, value) in [
            ("current", freq.current),
            ("min", freq.min),
            ("max", freq.max),
        ] {
            assert_ne!(
                value,
                Some(0),
                concat!(
                    "{} frequency reads Some(0), which is not a clock any ",
                    "running processor has — absence must be `None`, not zero"
                ),
                what
            );
        }
    }
}

/// Secure Boot must be read from `UEFISecureBootEnabled`, not from whether the key
/// holding it exists.
///
/// `HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\State` is present on every UEFI
/// machine regardless of enforcement, so testing for the key reported Secure Boot as
/// on for every UEFI system. The two fields also have to agree with each other: the
/// old code could only ever upgrade `boot_type` to `SecureBoot`, never downgrade it,
/// so it shipped `boot_type: SecureBoot` alongside `secure_boot: false`.
#[test]
#[cfg(windows)]
fn secure_boot_claim_matches_the_firmware_flag() {
    use ironmonlib::boot_config::{BootMonitor, BootType};

    let Ok(monitor) = BootMonitor::new() else {
        return;
    };
    let info = &monitor.boot_info;

    assert_eq!(
        info.boot_type == BootType::SecureBoot,
        info.secure_boot == Some(true),
        "boot_type {:?} and secure_boot {:?} disagree; one of them was derived from \
         something other than UEFISecureBootEnabled",
        info.boot_type,
        info.secure_boot
    );

    // Legacy BIOS cannot have Secure Boot at all.
    assert!(
        !(info.boot_type == BootType::Legacy && info.secure_boot == Some(true)),
        "reported a legacy BIOS boot with Secure Boot enabled, which is not a state \
         that exists"
    );

    // And an unread flag is not a disabled one. `secure_boot` is `Option<bool>`
    // precisely so that `UEFISecureBootEnabled` being unreadable cannot be
    // published as Secure Boot being off, which is the finding a posture check
    // would act on.
    if info.secure_boot.is_none() {
        assert_ne!(
            info.boot_type,
            BootType::SecureBoot,
            "claimed a Secure Boot type while the flag itself was never read"
        );
    }
}

/// The Windows OS reader must produce a real build, not the empty defaults it fell
/// back to whenever a subprocess failed to spawn.
#[test]
#[cfg(windows)]
fn windows_os_info_reports_a_real_build() {
    let Ok(monitor) = ironmonlib::os_info::OsInfoMonitor::new() else {
        return;
    };
    let info = monitor.info();

    assert!(
        !info.os_name.is_empty(),
        "OS name is empty; the registry publishes ProductName on every Windows install"
    );
    assert!(
        !info.kernel_version.is_empty(),
        "kernel version is empty, so the build number was never read"
    );
    // `ProductName` still says "Windows 10" on Windows 11; the build number is what
    // distinguishes them, and the reader is supposed to apply that correction.
    let build: u32 = info.os_build.parse().unwrap_or(0);
    if build >= 22000 {
        assert!(
            !info.os_name.contains("Windows 10"),
            "build {build} is Windows 11, but the name still reads {:?} — the \
             registry's stale ProductName was passed through uncorrected",
            info.os_name
        );
    }
    assert!(
        info.uptime_seconds > 0,
        "uptime reads zero, which would mean the machine booted this instant"
    );
}
