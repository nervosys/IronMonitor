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
        ironmonlib::input::InputMonitor,
        |m: &ironmonlib::input::InputMonitor| m.devices().len()
    );
    probe!(
        "services",
        ironmonlib::services::ServiceMonitor,
        |m: &ironmonlib::services::ServiceMonitor| m.services().len()
    );
    probe!(
        "storage_controller",
        ironmonlib::storage_controller::StorageControllerMonitor,
        |m: &ironmonlib::storage_controller::StorageControllerMonitor| m.controllers().len()
    );
    probe!(
        "iommu",
        ironmonlib::iommu::IommuMonitor,
        |m: &ironmonlib::iommu::IommuMonitor| m.groups().len()
    );
    probe!(
        "interrupt_map",
        ironmonlib::interrupt_map::InterruptMapMonitor,
        |m: &ironmonlib::interrupt_map::InterruptMapMonitor| m.interrupts().len()
    );
    probe!(
        "io_scheduler",
        ironmonlib::io_scheduler::IoSchedulerMonitor,
        |m: &ironmonlib::io_scheduler::IoSchedulerMonitor| m.devices().len()
    );
    probe!(
        "dma_engine",
        ironmonlib::dma_engine::DmaEngineMonitor,
        |m: &ironmonlib::dma_engine::DmaEngineMonitor| m.controllers().len()
    );
    probe!(
        "gpu_topology",
        ironmonlib::gpu_topology::GpuTopologyMonitor,
        |m: &ironmonlib::gpu_topology::GpuTopologyMonitor| m.gpus().len()
    );
    probe!(
        "power_profile",
        ironmonlib::power_profile::PowerProfileMonitor,
        |m: &ironmonlib::power_profile::PowerProfileMonitor| m.power_plans().len()
    );
    probe!(
        "thermal_zone",
        ironmonlib::thermal_zone::ThermalZoneMonitor,
        |m: &ironmonlib::thermal_zone::ThermalZoneMonitor| m.zones().len()
    );
    probe!(
        "voltage_regulator",
        ironmonlib::voltage_regulator::VoltageRegulatorMonitor,
        |m: &ironmonlib::voltage_regulator::VoltageRegulatorMonitor| m.regulators().len()
    );
    probe!(
        "watchdog",
        ironmonlib::watchdog::WatchdogMonitor,
        |m: &ironmonlib::watchdog::WatchdogMonitor| m.devices().len()
    );
    probe!(
        "audio",
        ironmonlib::audio::AudioMonitor,
        |m: &ironmonlib::audio::AudioMonitor| m.devices().len()
    );
    probe!(
        "bluetooth",
        ironmonlib::bluetooth::BluetoothMonitor,
        |m: &ironmonlib::bluetooth::BluetoothMonitor| m.adapters().len()
    );
    probe!(
        "camera",
        ironmonlib::camera::CameraMonitor,
        |m: &ironmonlib::camera::CameraMonitor| m.cameras().len()
    );
    probe!(
        "codec",
        ironmonlib::codec::CodecMonitor,
        |m: &ironmonlib::codec::CodecMonitor| m.capabilities().len()
    );
    probe!(
        "printer",
        ironmonlib::printer::PrinterMonitor,
        |m: &ironmonlib::printer::PrinterMonitor| m.printers().len()
    );

    // Singleton reports rather than collections: one item when they construct.
    probe!(
        "kernel_params",
        ironmonlib::kernel_params::KernelParamsMonitor,
        // Was a hardcoded `1`, which counted nothing and reported "answers here"
        // for a reader that returns nothing on this machine. The table exists to
        // replace guesses about which readers answer; a constant in it is the
        // one thing it must not contain.
        |m: &ironmonlib::kernel_params::KernelParamsMonitor| m.report().params.len()
    );
    probe!(
        "memory_bandwidth",
        ironmonlib::memory_bandwidth::MemoryBandwidthMonitor,
        // 1 only when the memory generation was identified. Everything this
        // reader produces rests on that, and without it the estimator falls back
        // to 3200 MT/s and a 0.75 efficiency factor -- so a constant here would
        // report "answers" for a machine where nothing was read.
        |m: &ironmonlib::memory_bandwidth::MemoryBandwidthMonitor| {
            usize::from(
                m.estimate().generation != ironmonlib::memory_bandwidth::MemoryGeneration::Unknown,
            )
        }
    );
    probe!(
        "memory_topology",
        ironmonlib::memory_topology::MemoryTopologyMonitor,
        |m: &ironmonlib::memory_topology::MemoryTopologyMonitor| m.populated_dimms().len()
    );
    probe!(
        "cpu_microarch",
        ironmonlib::cpu_microarch::CpuMicroarchMonitor,
        |m: &ironmonlib::cpu_microarch::CpuMicroarchMonitor| m.supported_extensions().len()
    );
    probe!(
        "crypto_accel",
        ironmonlib::crypto_accel::CryptoAccelMonitor,
        // The features and RNG sources are the facts this reader holds; the
        // score beside them is a table lookup and is not published anywhere.
        |m: &ironmonlib::crypto_accel::CryptoAccelMonitor| {
            m.report().features.len() + m.report().rng_sources.len()
        }
    );
    probe!(
        "interconnect",
        ironmonlib::interconnect::InterconnectMonitor,
        |m: &ironmonlib::interconnect::InterconnectMonitor| m.inter_socket_links().len()
    );
    probe!(
        "security_mitigations",
        ironmonlib::security_mitigations::SecurityMitigationsMonitor,
        |m: &ironmonlib::security_mitigations::SecurityMitigationsMonitor| m.unmitigated().len()
    );
}
