//! Hardware monitoring example using the native hwmon module
//!
//! This example demonstrates how to use the hwmon module to read hardware sensors
//! without relying on external tools like LibreHardwareMonitor.
//!
//! Run with:
//!   cargo run --release --features nvidia --example hwmon

use ironmonlib::hwmon::{HardwareMonitor, HwSensorType};

fn main() {
    println!("=== IronMonitor - Hardware Monitor Example ===\n");

    // Create a hardware monitor instance
    println!("Initializing hardware monitor...\n");
    let monitor = HardwareMonitor::new();

    // Get all sensors
    let all_sensors = monitor.all_sensors();
    println!("Found {} total sensors\n", all_sensors.len());

    // Group sensors by hardware type
    println!("=== CPU Sensors ===");
    for sensor in monitor.cpu_sensors() {
        print_sensor(sensor);
    }

    println!("\n=== GPU Sensors ===");
    for sensor in monitor.gpu_sensors() {
        print_sensor(sensor);
    }

    println!("\n=== Storage Sensors ===");
    for sensor in monitor.storage_sensors() {
        print_sensor(sensor);
    }

    println!("\n=== Motherboard Sensors ===");
    for sensor in monitor.motherboard_sensors() {
        print_sensor(sensor);
    }

    // Show sensors by type
    println!("\n=== All Temperatures ===");
    for sensor in monitor.temperatures() {
        println!(
            "  {:40} {:6.1}°C [{:?}]",
            sensor.name, sensor.value, sensor.hardware_type
        );
    }

    println!("\n=== All Fans ===");
    for sensor in monitor.fans() {
        let unit = if sensor.value > 200.0 { "RPM" } else { "%" };
        println!(
            "  {:40} {:6.0} {} [{:?}]",
            sensor.name, sensor.value, unit, sensor.hardware_type
        );
    }

    println!("\n=== All Power ===");
    for sensor in monitor.power_sensors() {
        println!(
            "  {:40} {:6.1} W [{:?}]",
            sensor.name, sensor.value, sensor.hardware_type
        );
    }

    println!("\n=== All Clocks ===");
    for sensor in monitor.clocks() {
        println!(
            "  {:40} {:6.0} MHz [{:?}]",
            sensor.name, sensor.value, sensor.hardware_type
        );
    }

    println!("\n=== All Utilization/Load ===");
    for sensor in monitor.loads() {
        println!(
            "  {:40} {:6.1}% [{:?}]",
            sensor.name, sensor.value, sensor.hardware_type
        );
    }

    if all_sensors.is_empty() {
        println!("\n⚠️  No sensors found.");
        println!("   Note: Some sensors require elevated privileges (run as Administrator)");
        println!("   - CPU temperature: May require kernel driver for MSR access");
        println!("   - Storage temperature: Requires access to PhysicalDrive devices");
        println!("   - Motherboard sensors: Requires Super I/O chip access (kernel driver)");
    }
}

fn print_sensor(sensor: &ironmonlib::hwmon::HwSensor) {
    let unit = match sensor.sensor_type {
        HwSensorType::Temperature => "°C",
        HwSensorType::Voltage => "V",
        HwSensorType::Fan => {
            if sensor.value > 200.0 {
                "RPM"
            } else {
                "%"
            }
        }
        HwSensorType::Power => "W",
        HwSensorType::Clock => "MHz",
        HwSensorType::Load => "%",
        HwSensorType::Energy => "J",
        _ => "",
    };

    let range = match (sensor.min, sensor.max) {
        (Some(min), Some(max)) => format!(" (min: {:.1}, max: {:.1})", min, max),
        (None, Some(max)) => format!(" (max: {:.1})", max),
        (Some(min), None) => format!(" (min: {:.1})", min),
        _ => String::new(),
    };

    println!(
        "  {:40} {:>8.1} {}{}",
        sensor.name, sensor.value, unit, range
    );
}
