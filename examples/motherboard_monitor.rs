// Example: Motherboard and System Monitoring
//
// Demonstrates motherboard sensor monitoring and system information retrieval

use ironmonlib::motherboard;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== IronMonitor: Motherboard & System Information ===\n");

    // System Information
    println!("=================================================");
    println!("System Information");
    println!("=================================================\n");

    match motherboard::get_system_info() {
        Ok(info) => {
            println!("[OS] Operating System:");
            println!("  Name:         {}", info.os_name);
            println!("  Version:      {}", info.os_version);
            if let Some(kernel) = &info.kernel_version {
                println!("  Kernel:       {}", kernel);
            }
            println!("  Architecture: {}", info.architecture);
            if let Some(hostname) = &info.hostname {
                println!("  Hostname:     {}", hostname);
            }

            println!("\n[BIOS] BIOS/UEFI:");
            println!("  Type:         {:?}", info.bios.firmware_type);
            if let Some(vendor) = &info.bios.vendor {
                println!("  Vendor:       {}", vendor);
            }
            if let Some(version) = &info.bios.version {
                println!("  Version:      {}", version);
            }
            if let Some(date) = &info.bios.release_date {
                println!("  Release Date: {}", date);
            }
            if let Some(secure_boot) = info.bios.secure_boot {
                println!(
                    "  Secure Boot:  {}",
                    if secure_boot { "Enabled" } else { "Disabled" }
                );
            }

            println!("\n[HW]  Hardware:");
            if let Some(manufacturer) = &info.manufacturer {
                println!("  Manufacturer: {}", manufacturer);
            }
            if let Some(product) = &info.product_name {
                println!("  Model:        {}", product);
            }
            if let Some(serial) = &info.serial_number {
                println!("  Serial:       {}", serial);
            }
            if let Some(uuid) = &info.uuid {
                println!("  UUID:         {}", uuid);
            }

            println!("\n[MB] Motherboard:");
            if let Some(vendor) = &info.board_vendor {
                println!("  Vendor:       {}", vendor);
            }
            if let Some(name) = &info.board_name {
                println!("  Model:        {}", name);
            }
            if let Some(version) = &info.board_version {
                println!("  Version:      {}", version);
            }

            println!("\n[CPU] CPU:");
            if let Some(cpu_name) = &info.cpu_name {
                println!("  Model:        {}", cpu_name);
            }
            if let Some(cores) = info.cpu_cores {
                println!("  Cores:        {}", cores);
            }
            if let Some(threads) = info.cpu_threads {
                println!("  Threads:      {}", threads);
            }
        }
        Err(e) => {
            println!("[ERROR] Failed to get system information: {}", e);
        }
    }

    // Driver Versions
    println!("\n===============================================");
    println!("Driver Versions");
    println!("===============================================\n");

    match motherboard::get_driver_versions() {
        Ok(drivers) => {
            if drivers.is_empty() {
                println!("No driver information available (may require elevated privileges)");
            } else {
                for driver in drivers {
                    println!("[PKG] {} ({}):", driver.name, driver.driver_type);
                    println!("  Version:     {}", driver.version);
                    if let Some(desc) = &driver.description {
                        println!("  Description: {}", desc);
                    }
                    if let Some(vendor) = &driver.vendor {
                        println!("  Vendor:      {}", vendor);
                    }
                    if let Some(date) = &driver.date {
                        println!("  Date:        {}", date);
                    }
                    println!();
                }
            }
        }
        Err(e) => {
            println!("[ERROR] Failed to get driver information: {}", e);
        }
    }

    // Motherboard Sensors
    println!("===============================================");
    println!("Motherboard Sensors");
    println!("===============================================\n");

    match motherboard::enumerate_sensors() {
        Ok(sensors) => {
            if sensors.is_empty() {
                println!(
                    "No sensors found (may require lm-sensors on Linux or elevated privileges)"
                );
            } else {
                println!("Found {} sensor chip(s)\n", sensors.len());

                for (i, sensor) in sensors.iter().enumerate() {
                    println!("=== Chip {}: {} ===", i + 1, sensor.name());
                    if let Some(path) = sensor.device_path() {
                        println!("Path: {}", path);
                    }
                    println!();

                    // Temperature sensors
                    match sensor.temperature_sensors() {
                        Ok(temps) => {
                            if !temps.is_empty() {
                                println!("[TEMP]  Temperatures:");
                                for temp in temps {
                                    print!("  {:<20} {:>6.1}°C", temp.label, temp.temperature);
                                    if let Some(max) = temp.max {
                                        print!("  (max: {:.1}°C)", max);
                                    }
                                    if let Some(crit) = temp.critical {
                                        print!("  (crit: {:.1}°C)", crit);
                                    }
                                    println!("  [{:?}]", temp.sensor_type);
                                }
                                println!();
                            }
                        }
                        Err(e) => println!("  Error reading temperatures: {}\n", e),
                    }

                    // Voltage rails
                    match sensor.voltage_rails() {
                        Ok(voltages) => {
                            if !voltages.is_empty() {
                                println!("[VOLT] Voltages:");
                                for volt in voltages {
                                    print!("  {:<20} {:>7.3}V", volt.label, volt.voltage);
                                    if volt.min.is_some() || volt.max.is_some() {
                                        print!("  (");
                                        if let Some(min) = volt.min {
                                            print!("min: {:.3}V", min);
                                        }
                                        if volt.min.is_some() && volt.max.is_some() {
                                            print!(", ");
                                        }
                                        if let Some(max) = volt.max {
                                            print!("max: {:.3}V", max);
                                        }
                                        print!(")");
                                    }
                                    println!();
                                }
                                println!();
                            }
                        }
                        Err(e) => println!("  Error reading voltages: {}\n", e),
                    }

                    // Fans
                    match sensor.fans() {
                        Ok(fans) => {
                            if !fans.is_empty() {
                                println!("[FAN] Fans:");
                                for fan in fans {
                                    print!("  {:<20}", fan.label);
                                    if let Some(rpm) = fan.rpm {
                                        print!(" {:>5} RPM", rpm);
                                    } else {
                                        print!("     0 RPM (stopped)");
                                    }
                                    if let Some(pwm) = fan.pwm {
                                        let percent = (pwm as f32 / 255.0 * 100.0) as u8;
                                        print!("  (PWM: {}%)", percent);
                                    }
                                    if fan.controllable {
                                        print!("  [Controllable]");
                                    }
                                    println!();
                                }
                                println!();
                            }
                        }
                        Err(e) => println!("  Error reading fans: {}\n", e),
                    }
                }
            }
        }
        Err(e) => {
            println!("[ERROR] Failed to enumerate sensors: {}", e);
            println!("Note: On Linux, you may need to:");
            println!("  1. Install lm-sensors: sudo apt install lm-sensors");
            println!("  2. Run sensors-detect: sudo sensors-detect");
            println!("  3. Load kernel modules: sudo modprobe <module_name>");
        }
    }

    Ok(())
}
