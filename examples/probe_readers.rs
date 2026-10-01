//! Probe every reader that the ontology does not yet cover.
//!
//! Plan item F adds ontology clusters only for readers that demonstrably answer
//! on the machine at hand -- declaring a cluster for a reader that returns
//! nothing here would put entities in the graph that no test can confirm. This
//! example constructs each uncovered monitor and prints what it produced, so
//! the choice of which clusters to add is made from evidence rather than from
//! the module list.
//!
//! Three statuses, and the distinction between the last two is the point:
//! `ok` enumerated something, `err` failed to construct and says why, and
//! `none` constructed and found nothing. A `none` does **not** mean the machine
//! lacks the hardware -- it equally means the reader is a stub on this platform
//! that returns an empty list without saying so. Establish which before
//! concluding a cluster needs different hardware to verify.
//!
//! Run with `cargo run --example probe_readers --all-features`.

macro_rules! probe {
    ($name:literal, $ty:path, $count:expr) => {{
        match <$ty>::new() {
            Ok(m) => {
                let n: usize = ($count)(&m);
                // A reader that constructed and enumerated nothing is not a
                // reader that answered, and both arrive here as `Ok`. Printing
                // both as "ok" is how a stub returning `Ok(vec![])` on this
                // platform reads as a machine that genuinely has none of the
                // thing -- which is the defect this table exists to avoid
                // making, and the one RAPL already shipped once.
                let status = if n == 0 { "none" } else { "ok" };
                println!("{:<22} {:<7} {}", $name, status, n);
            }
            Err(e) => println!("{:<22} {:<7} {}", $name, "err", e),
        }
    }};
}

fn main() {
    println!("{:<22} {:<7} items", "reader", "status");

    probe!(
        "input",
        ironmonitor::input::InputMonitor,
        |m: &ironmonitor::input::InputMonitor| m.devices().len()
    );
    probe!(
        "services",
        ironmonitor::services::ServiceMonitor,
        |m: &ironmonitor::services::ServiceMonitor| m.services().len()
    );
    probe!(
        "storage_controller",
        ironmonitor::storage_controller::StorageControllerMonitor,
        |m: &ironmonitor::storage_controller::StorageControllerMonitor| m.controllers().len()
    );
    probe!(
        "iommu",
        ironmonitor::iommu::IommuMonitor,
        |m: &ironmonitor::iommu::IommuMonitor| m.groups().len()
    );
    probe!(
        "interrupt_map",
        ironmonitor::interrupt_map::InterruptMapMonitor,
        |m: &ironmonitor::interrupt_map::InterruptMapMonitor| m.interrupts().len()
    );
    probe!(
        "io_scheduler",
        ironmonitor::io_scheduler::IoSchedulerMonitor,
        |m: &ironmonitor::io_scheduler::IoSchedulerMonitor| m.devices().len()
    );
    probe!(
        "dma_engine",
        ironmonitor::dma_engine::DmaEngineMonitor,
        |m: &ironmonitor::dma_engine::DmaEngineMonitor| m.controllers().len()
    );
    probe!(
        "gpu_topology",
        ironmonitor::gpu_topology::GpuTopologyMonitor,
        |m: &ironmonitor::gpu_topology::GpuTopologyMonitor| m.gpus().len()
    );
    probe!(
        "power_profile",
        ironmonitor::power_profile::PowerProfileMonitor,
        |m: &ironmonitor::power_profile::PowerProfileMonitor| m.power_plans().len()
    );
    probe!(
        "thermal_zone",
        ironmonitor::thermal_zone::ThermalZoneMonitor,
        |m: &ironmonitor::thermal_zone::ThermalZoneMonitor| m.zones().len()
    );
    probe!(
        "voltage_regulator",
        ironmonitor::voltage_regulator::VoltageRegulatorMonitor,
        |m: &ironmonitor::voltage_regulator::VoltageRegulatorMonitor| m.regulators().len()
    );
    probe!(
        "watchdog",
        ironmonitor::watchdog::WatchdogMonitor,
        |m: &ironmonitor::watchdog::WatchdogMonitor| m.devices().len()
    );
    probe!(
        "audio",
        ironmonitor::audio::AudioMonitor,
        |m: &ironmonitor::audio::AudioMonitor| m.devices().len()
    );
    probe!(
        "bluetooth",
        ironmonitor::bluetooth::BluetoothMonitor,
        |m: &ironmonitor::bluetooth::BluetoothMonitor| m.adapters().len()
    );
    probe!(
        "camera",
        ironmonitor::camera::CameraMonitor,
        |m: &ironmonitor::camera::CameraMonitor| m.cameras().len()
    );
    probe!(
        "codec",
        ironmonitor::codec::CodecMonitor,
        |m: &ironmonitor::codec::CodecMonitor| m.capabilities().len()
    );
    probe!(
        "printer",
        ironmonitor::printer::PrinterMonitor,
        |m: &ironmonitor::printer::PrinterMonitor| m.printers().len()
    );

    // Singleton reports rather than collections: one item when they construct.
    probe!(
        "kernel_params",
        ironmonitor::kernel_params::KernelParamsMonitor,
        // Was a hardcoded `1`, which counted nothing and reported "answers here"
        // for a reader that returns nothing on this machine. The table exists to
        // replace guesses about which readers answer; a constant in it is the
        // one thing it must not contain.
        |m: &ironmonitor::kernel_params::KernelParamsMonitor| m.report().params.len()
    );
    probe!(
        "memory_bandwidth",
        ironmonitor::memory_bandwidth::MemoryBandwidthMonitor,
        // 1 only when the memory generation was identified. Everything this
        // reader produces rests on that, and without it the estimator falls back
        // to 3200 MT/s and a 0.75 efficiency factor -- so a constant here would
        // report "answers" for a machine where nothing was read.
        |m: &ironmonitor::memory_bandwidth::MemoryBandwidthMonitor| {
            usize::from(
                m.estimate().generation != ironmonitor::memory_bandwidth::MemoryGeneration::Unknown,
            )
        }
    );
    probe!(
        "memory_topology",
        ironmonitor::memory_topology::MemoryTopologyMonitor,
        |m: &ironmonitor::memory_topology::MemoryTopologyMonitor| m.populated_dimms().len()
    );
    probe!(
        "cpu_microarch",
        ironmonitor::cpu_microarch::CpuMicroarchMonitor,
        |m: &ironmonitor::cpu_microarch::CpuMicroarchMonitor| m.supported_extensions().len()
    );
    probe!(
        "crypto_accel",
        ironmonitor::crypto_accel::CryptoAccelMonitor,
        // The features and RNG sources are the facts this reader holds; the
        // score beside them is a table lookup and is not published anywhere.
        |m: &ironmonitor::crypto_accel::CryptoAccelMonitor| {
            m.report().features.len() + m.report().rng_sources.len()
        }
    );
    probe!(
        "interconnect",
        ironmonitor::interconnect::InterconnectMonitor,
        |m: &ironmonitor::interconnect::InterconnectMonitor| m.inter_socket_links().len()
    );
    probe!(
        "security_mitigations",
        ironmonitor::security_mitigations::SecurityMitigationsMonitor,
        // What this reader *enumerated*, not what it found wrong with it. This
        // counted `unmitigated()`, so a fully patched machine reported `none 0`
        // for a reader that had just read nineteen vulnerability files and found
        // every one of them mitigated — putting it in the silent column, which is
        // the column that drives the work plan. Third counter in this table to
        // measure the wrong thing; `kernel_params` was a literal `1`.
        |m: &ironmonitor::security_mitigations::SecurityMitigationsMonitor| {
            m.vulnerabilities.len()
        }
    );

    // The four readers HANDOFF listed as never probed. `hardware_ai` is not
    // here: every field of its report is an inference or an identifier, so it
    // has nothing a probe could confirm as a reading.
    probe!(
        "drm_monitor",
        ironmonitor::drm_monitor::DrmMonitor,
        |m: &ironmonitor::drm_monitor::DrmMonitor| m.devices().len()
    );
    probe!(
        "scheduler",
        ironmonitor::scheduler::SchedulerMonitor,
        // PSI entries enumerated, not "how many are under pressure": the table
        // counts what a reader read, which `security_mitigations` got wrong once.
        |m: &ironmonitor::scheduler::SchedulerMonitor| m.pressure().len()
    );
    {
        // `WslDetector::detect` cannot fail, so it does not fit `probe!`. Not
        // being inside WSL is a correct `none` on a Windows host.
        let w = ironmonitor::wsl::WslDetector::detect();
        let status = if w.is_wsl { "ok" } else { "none" };
        println!(
            "{:<22} {:<7} version {:?}, dxg {}, cuda {}, {} virtual adapter(s)",
            "wsl",
            status,
            w.version,
            w.dxg_available,
            w.cuda_available,
            w.virtual_adapters.len()
        );
    }
}
