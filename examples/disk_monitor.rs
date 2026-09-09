//! Disk Monitoring Example
//!
//! Demonstrates the disk monitoring interface for SSDs, HDDs, and NVMe devices.
//! Run with: cargo run --example disk_monitor

use ironmonlib::disk;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== IronMonitor: Disk Monitoring Example ===\n");

    // Enumerate all disks
    let disks = match disk::enumerate_disks() {
        Ok(disks) => disks,
        Err(e) => {
            eprintln!("WARNING: Disk monitoring not available on this platform.");
            eprintln!("Error: {}", e);
            eprintln!("\nDisk monitoring requires:");
            eprintln!("  - Linux: Root/sudo access to /dev/nvme*, /dev/sd*, /sys/block/");
            eprintln!("  - Windows: Administrator privileges (not yet fully implemented)");
            eprintln!("  - macOS: Disk Utility framework access (not yet implemented)");
            eprintln!("\nPlease run with elevated privileges or on a supported platform.");
            std::process::exit(0);
        }
    };

    println!("Found {} disk device(s)\n", disks.len());

    for disk in &disks {
        print_disk_info(disk.as_ref())?;
    }

    Ok(())
}

fn print_disk_info(disk: &dyn disk::DiskDevice) -> Result<(), Box<dyn std::error::Error>> {
    println!("===============================================");
    println!("Disk: {} ({:?})", disk.name(), disk.disk_type());
    println!("===============================================\n");

    // Basic Info
    match disk.info() {
        Ok(info) => {
            println!("[MB] Device Information:");
            println!("  Model:          {}", info.model);
            if let Some(vendor) = &info.vendor {
                println!("  Vendor:         {}", vendor);
            }
            if let Some(serial) = &info.serial {
                println!("  Serial:         {}", serial);
            }
            if let Some(firmware) = &info.firmware {
                println!("  Firmware:       {}", firmware);
            }
            println!(
                "  Capacity:       {:.2} GB ({} bytes)",
                info.capacity as f64 / 1_000_000_000.0,
                info.capacity
            );
            match info.block_size {
                Some(b) => println!("  Block Size:     {b} bytes"),
                None => println!("  Block Size:     not read"),
            }
            if let Some(phys) = info.physical_sector_size {
                println!("  Physical Sector: {} bytes", phys);
            }
            if let Some(logical) = info.logical_sector_size {
                println!("  Logical Sector:  {} bytes", logical);
            }
            if let Some(rpm) = info.rotation_rate {
                println!("  Rotation Rate:  {} RPM", rpm);
            }
            println!("  Device Path:    {}", disk.device_path().display());
        }
        Err(e) => println!("  Failed to get device info: {}", e),
    }

    // I/O Statistics
    println!("\n[INFO] I/O Statistics:");
    match disk.io_stats() {
        Ok(stats) => {
            println!(
                "  Read:           {:.2} GB ({} ops)",
                stats.read_bytes as f64 / 1_000_000_000.0,
                stats.read_ops
            );
            println!(
                "  Write:          {:.2} GB ({} ops)",
                stats.write_bytes as f64 / 1_000_000_000.0,
                stats.write_ops
            );
            println!(
                "  Total:          {:.2} GB ({} ops)",
                stats.total_bytes() as f64 / 1_000_000_000.0,
                stats.total_ops()
            );
            if let Some(read_time) = stats.read_time_ms {
                println!("  Read Time:      {} ms", read_time);
            }
            if let Some(write_time) = stats.write_time_ms {
                println!("  Write Time:     {} ms", write_time);
            }
            if let Some(queue) = stats.queue_depth {
                println!("  Queue Depth:    {}", queue);
            }
        }
        Err(e) => println!("  Failed to get I/O stats: {}", e),
    }

    // Temperature
    if let Ok(Some(temp)) = disk.temperature() {
        println!("\n[TEMP]  Temperature:    {:.1}°C", temp);
    }

    // Health
    match disk.health() {
        Ok(health) => {
            println!("\n❤️  Health Status:  {:?}", health);
        }
        Err(e) => println!("\n❤️  Health Status:  Failed to determine ({})", e),
    }

    // Filesystems
    if let Ok(filesystems) = disk.filesystem_info() {
        if !filesystems.is_empty() {
            println!("\n💾 Filesystems:");
            for fs in filesystems {
                println!("  Mount Point:    {}", fs.mount_point.display());
                println!("  Type:           {}", fs.fs_type);
                println!(
                    "  Total:          {:.2} GB",
                    fs.total_size as f64 / 1_000_000_000.0
                );
                println!(
                    "  Used:           {:.2} GB ({:.1}%)",
                    fs.used_size as f64 / 1_000_000_000.0,
                    fs.usage_percent()
                );
                println!(
                    "  Available:      {:.2} GB",
                    fs.available_size as f64 / 1_000_000_000.0
                );
                if let Some(total_inodes) = fs.total_inodes {
                    println!("  Total Inodes:   {}", total_inodes);
                }
                if let Some(used_inodes) = fs.used_inodes {
                    println!("  Used Inodes:    {}", used_inodes);
                }
                println!("  Read Only:      {}", fs.read_only);
                println!();
            }
        }
    }

    // SMART Info
    if let Ok(smart) = disk.smart_info() {
        println!("[SCAN] SMART Information:");
        // The drive's own verdict, or nothing. "false" would read as a
        // failing drive, which is not what an unobtainable verdict means.
        match smart.passed {
            Some(true) => println!("  Health Passed:  yes"),
            Some(false) => println!("  Health Passed:  NO"),
            None => println!("  Health Passed:  the drive did not give a verdict"),
        }
        if let Some(temp) = smart.temperature {
            println!("  Temperature:    {:.1}°C", temp);
        }
        if let Some(hours) = smart.power_on_hours {
            println!("  Power-On Hours: {}", hours);
        }
        if let Some(cycles) = smart.power_cycle_count {
            println!("  Power Cycles:   {}", cycles);
        }
        if let Some(reallocated) = smart.reallocated_sectors {
            println!("  Reallocated:    {} sectors", reallocated);
        }
        if let Some(pending) = smart.pending_sectors {
            println!("  Pending:        {} sectors", pending);
        }
        if let Some(uncorrectable) = smart.uncorrectable_sectors {
            println!("  Uncorrectable:  {} sectors", uncorrectable);
        }
    }

    // NVMe Info
    if let Ok(nvme) = disk.nvme_info() {
        println!("\n[VOLT] NVMe Information:");
        println!("  Model:          {}", nvme.model);
        println!("  Serial:         {}", nvme.serial);
        println!("  Firmware:       {}", nvme.firmware);
        // These come from Identify Controller, which needs elevation on Windows
        // and root for the ioctl on Linux. Say "unknown" rather than printing a
        // zero that reads as a real controller id or a namespace count of none.
        fn or_unknown<T: std::fmt::Display>(value: Option<T>) -> String {
            value.map_or_else(
                || "unknown (needs elevation)".to_string(),
                |v| v.to_string(),
            )
        }

        println!("  NVMe Version:   {}", or_unknown(nvme.nvme_version));
        println!(
            "  Total Capacity: {:.2} GB",
            nvme.total_capacity as f64 / 1_000_000_000.0
        );
        println!("  Controller ID:  {}", or_unknown(nvme.controller_id));
        println!("  Namespaces:     {}", or_unknown(nvme.num_namespaces));
        println!("  Power State:    {}", or_unknown(nvme.power_state));

        if !nvme.temperature_sensors.is_empty() {
            print!("  Temperatures:   ");
            for (i, temp) in nvme.temperature_sensors.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print!("{:.1}°C", temp);
            }
            println!();
        }

        if let Some(pct) = nvme.percentage_used {
            println!("  Wear Level:     {}%", pct);
        }

        if let Some(data_read) = nvme.data_units_read {
            println!(
                "  Data Read:      {:.2} TB",
                (data_read * 512) as f64 / 1_000_000_000_000.0
            );
        }

        if let Some(data_written) = nvme.data_units_written {
            println!(
                "  Data Written:   {:.2} TB",
                (data_written * 512) as f64 / 1_000_000_000_000.0
            );
        }
    }

    println!();
    Ok(())
}
