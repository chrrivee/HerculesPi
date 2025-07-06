use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use chrono::Local;
use clap::{Arg, ArgAction, Command};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use sysinfo::{CpuExt, DiskExt, NetworkExt, PidExt, ProcessExt, System, SystemExt};

mod config;
mod installer;
#[allow(dead_code)]
mod sensors;

// Configuration for resource monitoring
struct MonitorConfig {
    update_interval_ms: u64,
    show_cpu: bool,
    show_memory: bool,
    show_disk: bool,
    show_network: bool,
    show_processes: bool,
    max_processes: usize,
    continuous: bool,
    show_compact_mode: bool,
    show_installer: bool,
    show_sensors: bool,
    sensor_config: sensors::SensorConfig,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            update_interval_ms: 1000,
            show_cpu: true,
            show_memory: true,
            show_disk: true,
            show_network: true,
            show_processes: false,
            max_processes: 10,
            continuous: true,
            show_compact_mode: false,
            show_installer: false,
            show_sensors: false,
            sensor_config: sensors::SensorConfig::default(),
        }
    }
}

// System resources data container
struct SystemResources {
    system: System,
    last_net_receive: u64,
    last_net_transmit: u64,
    last_update: Instant,
    sensor_manager: Option<sensors::SensorManager>,
    last_sensor_data: sensors::SensorData,
}

impl SystemResources {
    fn new(config: &MonitorConfig) -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        let mut total_received = 0;
        let mut total_transmitted = 0;

        for (_, network) in system.networks() {
            total_received += network.received();
            total_transmitted += network.transmitted();
        }

        // Initialize sensor manager if sensors are enabled
        let sensor_manager = if config.show_sensors {
            match sensors::initialize_sensors(config.sensor_config.clone()) {
                Ok(manager) => Some(manager),
                Err(e) => {
                    eprintln!("Failed to initialize sensors: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Self {
            system,
            last_net_receive: total_received,
            last_net_transmit: total_transmitted,
            last_update: Instant::now(),
            sensor_manager,
            last_sensor_data: sensors::SensorData::default(),
        }
    }

    fn refresh(&mut self) {
        self.system.refresh_all();
        let mut total_received = 0;
        let mut total_transmitted = 0;

        for (_, network) in self.system.networks() {
            total_received += network.received();
            total_transmitted += network.transmitted();
        }

        self.last_net_receive = total_received;
        self.last_net_transmit = total_transmitted;
        self.last_update = Instant::now();

        // Update sensor data if available
        if let Some(ref manager) = self.sensor_manager {
            if let Some(result) = manager.try_receive_update() {
                match result {
                    Ok(data) => {
                        self.last_sensor_data = data;
                    }
                    Err(e) => {
                        eprintln!("Sensor error: {}", e);
                    }
                }
            }
        }
    }
}

// Main entry point
fn main() -> Result<()> {
    env_logger::init();

    // Handle special CLI commands first
    let args: Vec<String> = env::args().collect();

    // Handle configuration commands with exact syntax: "hercules conf <property> -> <new_value>"
    if args.len() >= 2 {
        match args[1].as_str() {
            "conf" => {
                if args.len() == 2 {
                    // Display current configuration
                    return config::ConfigManager::display_config();
                } else {
                    // Handle configuration change
                    return config::ConfigManager::handle_conf_command(&args[1..]);
                }
            }
            "conf-reset" => {
                return config::ConfigManager::reset_config();
            }
            // Handle shorthand commands
            "installer" => {
                installer::prompt_install();
            }
            "compact" => {
                // Run in compact mode
                let config_manager = config::ConfigManager::new()?;
                let file_config = config_manager.get_config();
                let mut config: MonitorConfig = file_config.into();
                config.show_compact_mode = true;
                config.continuous = false; // Single display for shorthand

                let resources = Arc::new(Mutex::new(SystemResources::new(&config)));
                return display_compact_mode(&resources, config.show_sensors);
            }
            "sensors" => {
                // Run with sensors enabled
                let config_manager = config::ConfigManager::new()?;
                let file_config = config_manager.get_config();
                let mut config: MonitorConfig = file_config.into();
                config.show_sensors = true;
                config.sensor_config.enabled = true;
                config.continuous = false; // Single display for shorthand

                let resources = Arc::new(Mutex::new(SystemResources::new(&config)));
                if config.show_compact_mode {
                    return display_compact_mode(&resources, true);
                } else {
                    monitor_resources(&resources, &config)?;
                    return monitor_sensors(&resources);
                }
            }
            _ => {}
        }
    }

    // Set up clap for command line argument handling
    let matches = Command::new("Hercules")
        .version("0.1.0")
        .author("Hercules Team")
        .about("System Resource Monitor")
        .arg(
            Arg::new("compact")
                .long("compact")
                .short('c')
                .help("Run in compact mode with Intel CPU ASCII art")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("installer")
                .long("installer")
                .short('i')
                .help("Run installer for intial setup, verification, or uninstall")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("sensors")
                .long("sensors")
                .short('s')
                .help("Enable gyroscope and accelerometer monitoring via USB")
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    // Check both command line arguments and direct "compact" argument
    let use_compact_mode = matches.get_flag("compact") || env::args().any(|arg| arg == "compact");

    let use_installer = matches.get_flag("installer") || env::args().any(|arg| arg == "installer");
    let use_sensors = matches.get_flag("sensors") || env::args().any(|arg| arg == "sensors");

    println!("{}", "HERCULES - System Resource Monitor".bold().green());
    println!("{}", "==================================".green());
    println!("Use 'hercules compact' or 'hercules --compact' for compact display");
    println!("Use 'hercules sensors' or 'hercules --sensors' to enable gyro/accelerometer");
    println!("Use 'hercules conf' to view configuration");
    println!("Use 'hercules conf <property> -> <value>' to change settings");
    println!();

    // Load configuration from file, then override with command line args
    let config_manager = config::ConfigManager::new()?;
    let file_config = config_manager.get_config();
    let mut config: MonitorConfig = file_config.into();

    // Override with command line arguments
    if use_compact_mode {
        config.show_compact_mode = true;
    }
    if use_installer {
        config.show_installer = true;
    }
    if use_sensors {
        config.show_sensors = true;
        config.sensor_config.enabled = true;
        config.sensor_config.update_interval_ms = config.update_interval_ms / 10;
    }

    // Create shared system resources
    let resources = Arc::new(Mutex::new(SystemResources::new(&config)));

    // If continuous monitoring, clear screen and show live stats
    if config.continuous {
        // Handle installer if requested
        if config.show_installer {
            installer::prompt_install(); // This will exit the program
        }

        // Create progress bar for visual effect
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈")
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );

        loop {
            // Clear screen and reset cursor
            print!("\x1B[2J\x1B[1;1H");
            io::stdout().flush().unwrap();

            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

            if config.show_compact_mode {
                display_compact_mode(&resources, config.show_sensors)?;
            } else {
                println!("{} {}", "HERCULES".bold().green(), timestamp.cyan());
                println!("{}", "==================================".green());

                if let Err(e) = monitor_resources(&resources, &config) {
                    eprintln!("Error monitoring resources: {}", e);
                    break;
                }

                // Display sensor data if enabled
                if config.show_sensors {
                    if let Err(e) = monitor_sensors(&resources) {
                        eprintln!("Error monitoring sensors: {}", e);
                    }
                }
            }

            pb.set_message(format!("Updated at {}", timestamp));
            pb.tick();

            thread::sleep(Duration::from_millis(config.update_interval_ms));

            // Refresh resources data
            if let Ok(mut res) = resources.lock() {
                res.refresh();
            }
        }
    } else {
        // One-time display of system information
        if config.show_installer {
            installer::prompt_install(); // This will exit the program
        }

        // One-time display of system information
        if config.show_compact_mode {
            display_compact_mode(&resources, config.show_sensors)?;
        } else {
            monitor_resources(&resources, &config)?;

            if config.show_sensors {
                monitor_sensors(&resources)?;
            }
        }
    }

    Ok(())
}

// Function to display compact mode with ASCII art
fn display_compact_mode(resources: &Arc<Mutex<SystemResources>>, show_sensors: bool) -> Result<()> {
    let res = resources
        .lock()
        .map_err(|e| anyhow!("Failed to lock resources: {}", e))?;

    // Get system info
    let hostname = res
        .system
        .host_name()
        .unwrap_or_else(|| "Unknown".to_string());
    let os_name = res.system.name().unwrap_or_else(|| "Unknown".to_string());
    let kernel_version = res
        .system
        .kernel_version()
        .unwrap_or_else(|| "Unknown".to_string());

    // CPU info
    let global_cpu_usage = res.system.global_cpu_info().cpu_usage();
    let cpu_count = res.system.cpus().len();

    // Memory info
    let total_mem = res.system.total_memory();
    let used_mem = res.system.used_memory();
    let total_gb = total_mem as f64 / 1_073_741_824.0; // Convert to GB
    let used_gb = used_mem as f64 / 1_073_741_824.0;
    let mem_percent = if total_mem > 0 {
        (used_mem as f64 / total_mem as f64) * 100.0
    } else {
        0.0
    };

    // Network info
    let elapsed = res.last_update.elapsed().as_secs_f64();

    // Calculate total network rates across all interfaces
    let mut total_received = 0;
    let mut total_transmitted = 0;

    for (_, data) in res.system.networks() {
        total_received += data.received();
        total_transmitted += data.transmitted();
    }

    // Calculate rates (bytes/sec)
    let total_recv_rate = if elapsed > 0.0 {
        (total_received - res.last_net_receive) as f64 / elapsed
    } else {
        0.0
    };

    let total_transmit_rate = if elapsed > 0.0 {
        (total_transmitted - res.last_net_transmit) as f64 / elapsed
    } else {
        0.0
    };

    // Get sensor data if enabled
    let sensor_data = res.last_sensor_data;
    let has_sensor_data = show_sensors
        && (sensor_data.acceleration[0] != 0.0
            || sensor_data.acceleration[1] != 0.0
            || sensor_data.acceleration[2] != 0.0
            || sensor_data.gyro[0] != 0.0
            || sensor_data.gyro[1] != 0.0
            || sensor_data.gyro[2] != 0.0);

    // ASCII art for CPU
    let cpu_art = [
        r"  ╔═════════════════╗  ",
        r"  ║ ┌─────────────┐ ║  ",
        r"  ║ │             │ ║  ",
        r"  ║ │    INTEL    │ ║  ",
        r"  ║ │             │ ║  ",
        r"  ║ │   CORE  i7  │ ║  ",
        r"  ║ │             │ ║  ",
        r"  ║ └─────────────┘ ║  ",
        r"  ╚═╩═╩═╩═╩═╩═╩═╩═╩═╝  ",
        r"    │ │ │ │ │ │ │ │    ",
    ];

    // Output in neofetch style
    let timestamp = Local::now().format("%H:%M:%S").to_string();
    let uptime = match res.system.uptime() {
        uptime if uptime < 60 => format!("{}s", uptime),
        uptime if uptime < 3600 => format!("{}m {}s", uptime / 60, uptime % 60),
        uptime => format!("{}h {}m", uptime / 3600, (uptime % 3600) / 60),
    };

    // Color the CPU art based on CPU usage
    let cpu_color = if global_cpu_usage < 25.0 {
        "cyan"
    } else if global_cpu_usage < 60.0 {
        "blue"
    } else if global_cpu_usage < 85.0 {
        "yellow"
    } else {
        "red"
    };

    // Draw header
    println!(
        "{}",
        "╭─────────────────────────────────────────────╮".cyan()
    );
    println!(
        "{} {} {} {}",
        "│".cyan(),
        "HERCULES".bold().green(),
        timestamp.cyan(),
        format!("(up: {})", uptime).yellow()
    );
    if show_sensors {
        println!(
            "{} {} {}",
            "│".cyan(),
            "🔬 SENSORS ENABLED".bold().bright_blue(),
            if has_sensor_data {
                "📡 ACTIVE"
            } else {
                "⚠️  NO DATA"
            }
            .yellow()
        );
    }
    println!(
        "{}",
        "╰─────────────────────────────────────────────╯".cyan()
    );

    // Memory bar (10 chars)
    let mem_bar_width = 10;
    let mem_filled = ((mem_percent as f64) / 100.0 * (mem_bar_width as f64)).round() as usize;
    let mem_bar = format!(
        "[{}{}]",
        "█".repeat(mem_filled).red(),
        "░".repeat(mem_bar_width - mem_filled).cyan()
    );

    // CPU bar (10 chars)
    let cpu_bar_width = 10;
    let cpu_filled = ((global_cpu_usage as f64) / 100.0 * (cpu_bar_width as f64)).round() as usize;
    let cpu_bar = format!(
        "[{}{}]",
        "█".repeat(cpu_filled).red(),
        "░".repeat(cpu_bar_width - cpu_filled).cyan()
    );

    // Draw main content with colored CPU art
    for (i, line) in cpu_art.iter().enumerate() {
        let colored_line = match cpu_color {
            "cyan" => line.cyan(),
            "blue" => line.blue(),
            "yellow" => line.yellow(),
            _ => line.red(),
        };

        let info = match i {
            0 => format!("{}@{}", "user".yellow(), hostname.bright_white()),
            1 => format!("{}", "─".repeat(hostname.len() + 6).cyan()),
            2 => format!("{}: {}", "OS".yellow(), os_name.bright_white()),
            3 => format!("{}: {}", "Kernel".yellow(), kernel_version.bright_white()),
            4 => format!(
                "{}: {} {}",
                "CPU".yellow(),
                cpu_count.to_string().bright_white(),
                "cores".bright_white()
            ),
            5 => format!(
                "{}: {}% {}",
                "CPU".yellow(),
                format!("{:.1}", global_cpu_usage).bright_white(),
                cpu_bar
            ),
            6 => format!("{}: {:.1}/{:.1} GB", "RAM".yellow(), used_gb, total_gb),
            7 => format!(
                "{}: {}% {}",
                "MEM".yellow(),
                format!("{:.1}", mem_percent).bright_white(),
                mem_bar
            ),
            8 => format!("{}: {:.1} KB/s", "▼".green(), total_recv_rate / 1024.0),
            9 => format!("{}: {:.1} KB/s", "▲".red(), total_transmit_rate / 1024.0),
            _ => String::new(),
        };

        println!("{}  {}", colored_line, info);
    }

    // Draw CPU core usage as a compact bar graph
    println!(
        "\n{}",
        "╭─────────────────────────────────────────────╮".cyan()
    );
    println!("{} {}", "│".cyan(), "CPU Cores:".bold().yellow());
    println!("{}", "│".cyan());

    // Display CPU core usage in a compact graphical format
    let core_bar_width = 12;
    for i in 0..res.system.cpus().len() {
        let cpu = &res.system.cpus()[i];
        let usage = cpu.cpu_usage();
        let filled = ((usage as f64) / 100.0 * (core_bar_width as f64)).round() as usize;
        let bar = format!(
            "[{}{}]",
            "█".repeat(filled).red(),
            "░".repeat(core_bar_width - filled).cyan()
        );

        if i % 2 == 0 {
            print!("│  Core {:2}: {:5.1}% {}  ", i, usage, bar);
        } else {
            println!("Core {:2}: {:5.1}% {}", i, usage, bar);
        }
    }
    // Make sure we end with a newline
    if res.system.cpus().len() % 2 != 0 {
        println!();
    }
    println!(
        "{}",
        "╰─────────────────────────────────────────────╯".cyan()
    );

    // Display sensor data in compact mode if enabled
    if show_sensors {
        println!(
            "\n{}",
            "╭─────────────────────────────────────────────╮".cyan()
        );
        println!("{} {}", "│".cyan(), "Sensor Data:".bold().bright_blue());
        println!("{}", "│".cyan());

        if has_sensor_data {
            // Compact sensor display
            println!(
                "│  🚀 Accel: X:{:6.2} Y:{:6.2} Z:{:6.2} m/s²",
                sensor_data.acceleration[0],
                sensor_data.acceleration[1],
                sensor_data.acceleration[2]
            );
            println!(
                "│  🌀 Gyro:  X:{:6.1} Y:{:6.1} Z:{:6.1} °/s",
                sensor_data.gyro[0], sensor_data.gyro[1], sensor_data.gyro[2]
            );

            if sensor_data.orientation[0] != 0.0
                || sensor_data.orientation[1] != 0.0
                || sensor_data.orientation[2] != 0.0
            {
                println!(
                    "│  📐 Orient: R:{:5.1} P:{:5.1} Y:{:5.1} °",
                    sensor_data.orientation[0],
                    sensor_data.orientation[1],
                    sensor_data.orientation[2]
                );
            }

            if sensor_data.temperature != 0.0 {
                println!("│  🌡️  Temp:  {:.1}°C", sensor_data.temperature);
            }

            // Simple orientation visualization
            let roll_char = match sensor_data.orientation[0] {
                r if r > 30.0 => "↗️",
                r if r > 10.0 => "↗",
                r if r < -30.0 => "↙️",
                r if r < -10.0 => "↙",
                _ => "→",
            };
            let pitch_char = match sensor_data.orientation[1] {
                p if p > 30.0 => "⬆️",
                p if p > 10.0 => "⬆",
                p if p < -30.0 => "⬇️",
                p if p < -10.0 => "⬇",
                _ => "➡️",
            };
            println!("│  📱 Position: {} {}", roll_char, pitch_char);
        } else {
            println!("│  ⚠️  No sensor data available");
            println!("│     Check USB connection or run with --sensors");
        }

        println!(
            "{}",
            "╰─────────────────────────────────────────────╯".cyan()
        );
    }

    Ok(())
}

// Main function for monitoring all resources
fn monitor_resources(
    resources: &Arc<Mutex<SystemResources>>,
    config: &MonitorConfig,
) -> Result<()> {
    let res = resources
        .lock()
        .map_err(|e| anyhow!("Failed to lock resources: {}", e))?;

    if config.show_cpu {
        monitor_cpu(&res)?;
    }

    if config.show_memory {
        monitor_memory(&res)?;
    }

    if config.show_disk {
        monitor_disks(&res)?;
    }

    if config.show_network {
        monitor_network(&res)?;
    }

    if config.show_processes {
        monitor_processes(&res, config.max_processes)?;
    }

    Ok(())
}

// Function to monitor and display sensor data
#[allow(dead_code)]
fn monitor_sensors(resources: &Arc<Mutex<SystemResources>>) -> Result<()> {
    if let Ok(res) = resources.lock() {
        let sensor_data = res.last_sensor_data;

        println!("{}", "\n=== Gyroscope & Accelerometer Data ===".cyan());

        // Format and display sensor readings
        println!(
            "Acceleration (m/s²): X: {:.2}, Y: {:.2}, Z: {:.2}",
            sensor_data.acceleration[0], sensor_data.acceleration[1], sensor_data.acceleration[2]
        );

        println!(
            "Gyroscope (deg/s):   X: {:.2}, Y: {:.2}, Z: {:.2}",
            sensor_data.gyro[0], sensor_data.gyro[1], sensor_data.gyro[2]
        );

        if sensor_data.orientation[0] != 0.0
            || sensor_data.orientation[1] != 0.0
            || sensor_data.orientation[2] != 0.0
        {
            println!(
                "Orientation (deg):  Roll: {:.2}, Pitch: {:.2}, Yaw: {:.2}",
                sensor_data.orientation[0], sensor_data.orientation[1], sensor_data.orientation[2]
            );
        }

        if sensor_data.temperature != 0.0 {
            println!("Temperature:        {:.1}°C", sensor_data.temperature);
        }

        // Display a visualization of the orientation
        visualize_orientation(&sensor_data);
    }

    Ok(())
}

// Function to visualize sensor orientation
#[allow(dead_code)]
fn visualize_orientation(sensor_data: &sensors::SensorData) {
    // Create a simple ASCII visualization of orientation
    let roll = sensor_data.orientation[0].to_radians();
    let pitch = sensor_data.orientation[1].to_radians();

    // Determine device orientation symbol
    let orientation_char = if pitch.abs() < 0.3 && roll.abs() < 0.3 {
        "⬜" // flat
    } else if pitch > 0.3 {
        "⬆️" // tilted forward
    } else if pitch < -0.3 {
        "⬇️" // tilted backward
    } else if roll > 0.3 {
        "➡️" // tilted right
    } else if roll < -0.3 {
        "⬅️" // tilted left
    } else {
        "⬜" // default
    };

    println!("Current orientation: {}", orientation_char);
}

// CPU monitoring function
fn monitor_cpu(res: &SystemResources) -> Result<()> {
    println!("\n{}", "CPU USAGE".bold().blue());
    println!("{}", "----------".blue());

    // Global CPU info
    let global_cpu_usage = res.system.global_cpu_info().cpu_usage();
    println!(
        "Global CPU Usage: {}%",
        format!("{:.1}", global_cpu_usage).yellow()
    );

    // Per-core CPU info
    for (i, cpu) in res.system.cpus().iter().enumerate() {
        println!(
            "  Core #{}: {}% - {} MHz",
            i,
            format!("{:.1}", cpu.cpu_usage()).yellow(),
            format!("{:.0}", cpu.frequency()).cyan()
        );
    }

    Ok(())
}

// Memory monitoring function
fn monitor_memory(res: &SystemResources) -> Result<()> {
    println!("\n{}", "MEMORY USAGE".bold().magenta());
    println!("{}", "------------".magenta());

    // Virtual memory
    let total_mem = res.system.total_memory();
    let used_mem = res.system.used_memory();
    let total_gb = total_mem as f64 / 1_073_741_824.0; // Convert to GB
    let used_gb = used_mem as f64 / 1_073_741_824.0;
    let percent = if total_mem > 0 {
        (used_mem as f64 / total_mem as f64) * 100.0
    } else {
        0.0
    };

    println!(
        "Memory: {}/{} GB ({}% used)",
        format!("{:.2}", used_gb).yellow(),
        format!("{:.2}", total_gb).green(),
        format!("{:.1}", percent).red()
    );

    // Swap memory
    let total_swap = res.system.total_swap();
    let used_swap = res.system.used_swap();
    let total_swap_gb = total_swap as f64 / 1_073_741_824.0;
    let used_swap_gb = used_swap as f64 / 1_073_741_824.0;
    let swap_percent = if total_swap > 0 {
        (used_swap as f64 / total_swap as f64) * 100.0
    } else {
        0.0
    };

    println!(
        "Swap: {}/{} GB ({}% used)",
        format!("{:.2}", used_swap_gb).yellow(),
        format!("{:.2}", total_swap_gb).green(),
        format!("{:.1}", swap_percent).red()
    );

    Ok(())
}

// Disk monitoring function
fn monitor_disks(res: &SystemResources) -> Result<()> {
    println!("\n{}", "DISK USAGE".bold().cyan());
    println!("{}", "----------".cyan());

    // Disks from sysinfo
    println!("Disks:");
    for disk in res.system.disks() {
        let total_gb = disk.total_space() as f64 / 1_073_741_824.0;
        let available_gb = disk.available_space() as f64 / 1_073_741_824.0;
        let used_gb = total_gb - available_gb;
        let percent = if total_gb > 0.0 {
            (used_gb / total_gb) * 100.0
        } else {
            0.0
        };

        println!(
            "  {}: {}/{} GB ({}% used) - Mount: {}",
            disk.name().to_string_lossy().yellow(),
            format!("{:.2}", used_gb).red(),
            format!("{:.2}", total_gb).green(),
            format!("{:.1}", percent).red(),
            disk.mount_point().to_string_lossy().cyan()
        );
    }

    Ok(())
}

// Network monitoring function
fn monitor_network(res: &SystemResources) -> Result<()> {
    println!("\n{}", "NETWORK USAGE".bold().green());
    println!("{}", "-------------".green());

    // Network interfaces from sysinfo
    println!("Network Interfaces:");

    let elapsed = res.last_update.elapsed().as_secs_f64();

    for (interface_name, data) in res.system.networks() {
        let received = data.received();
        let transmitted = data.transmitted();

        // Calculate rates (bytes/sec)
        let recv_rate = if elapsed > 0.0 {
            ((received - res.last_net_receive) as f64 / elapsed) as u64
        } else {
            0
        };

        let transmit_rate = if elapsed > 0.0 {
            ((transmitted - res.last_net_transmit) as f64 / elapsed) as u64
        } else {
            0
        };

        println!("  {}:", interface_name.yellow());
        println!(
            "    Total Received: {} bytes",
            format!("{}", received).cyan()
        );
        println!(
            "    Total Transmitted: {} bytes",
            format!("{}", transmitted).cyan()
        );
        println!(
            "    Receive Rate: {} KB/s",
            format!("{:.2}", recv_rate as f64 / 1024.0).green()
        );
        println!(
            "    Transmit Rate: {} KB/s",
            format!("{:.2}", transmit_rate as f64 / 1024.0).green()
        );
    }

    Ok(())
}

// Process monitoring function
fn monitor_processes(res: &SystemResources, max_processes: usize) -> Result<()> {
    println!("\n{}", "TOP PROCESSES".bold().yellow());
    println!("{}", "-------------".yellow());

    // Get processes from sysinfo
    let mut processes: Vec<_> = res.system.processes().iter().collect();

    // Sort by CPU usage (descending)
    processes.sort_by(|a, b| {
        b.1.cpu_usage()
            .partial_cmp(&a.1.cpu_usage())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    println!(
        "{:<6} {:<20} {:<10} {:<10} {:<10}",
        "PID", "NAME", "CPU%", "MEM MB", "STATUS"
    );

    for (i, (pid, process)) in processes.iter().enumerate() {
        if i >= max_processes {
            break;
        }

        let name = process.name();
        let cpu_usage = process.cpu_usage();
        let memory_usage = process.memory() as f64 / 1_048_576.0; // Convert to MB
        let status = format!("{:?}", process.status());

        println!(
            "{:<6} {:<20} {:<10.1} {:<10.1} {:<10}",
            pid.as_u32(),
            if name.len() > 20 { &name[0..17] } else { name },
            cpu_usage,
            memory_usage,
            status
        );
    }

    Ok(())
}
