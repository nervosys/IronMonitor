//! Print what the DRM monitor reports for each adapter's VRAM.
//!
//! `Win32_VideoController.AdapterRAM` is 32 bits, so a 24GB card reports
//! 4293918720 through it. This example exists to check the figure against
//! `nvidia-smi` on a machine with a card larger than 4GB.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let monitor = ironmonlib::DrmMonitor::new()?;
    for device in monitor.devices() {
        let vram = match device.vram_total_bytes {
            Some(b) => format!("{:.1} GiB", b as f64 / 1024.0 / 1024.0 / 1024.0),
            None => "not read".to_string(),
        };
        println!("{:<12} {:<28} {}", device.card_name, device.driver, vram);
    }
    Ok(())
}
