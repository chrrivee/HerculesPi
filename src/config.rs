use crate::sensors::SensorConfig;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use std::fs;
use std::path::PathBuf;

// Configuration structure that matches MonitorConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HerculesConfig {
    pub update_interval_ms: u64,
    pub show_cpu: bool,
    pub show_memory: bool,
    pub show_disk: bool,
    pub show_network: bool,
    pub show_processes: bool,
    pub max_processes: usize,
    pub continuous: bool,
    pub show_compact_mode: bool,
    pub show_installer: bool,
    pub show_sensors: bool,
    pub sensor_config: SensorConfig,
}

impl Default for HerculesConfig {
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
            sensor_config: SensorConfig::default(),
        }
    }
}

// Configuration manager
pub struct ConfigManager {
    config_path: PathBuf,
    config: HerculesConfig,
}

impl ConfigManager {
    pub fn new() -> Result<Self> {
        let config_dir = Self::get_config_dir()?;
        let config_path = config_dir.join("hercules.toml");

        // Create config directory if it doesn't exist
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)?;
        }

        let config = if config_path.exists() {
            Self::load_config(&config_path)?
        } else {
            let default_config = HerculesConfig::default();
            Self::save_config(&config_path, &default_config)?;
            default_config
        };

        Ok(ConfigManager {
            config_path,
            config,
        })
    }

    pub fn get_config(&self) -> &HerculesConfig {
        &self.config
    }

    pub fn save(&self) -> Result<()> {
        Self::save_config(&self.config_path, &self.config)
    }

    fn get_config_dir() -> Result<PathBuf> {
        if cfg!(windows) {
            if let Ok(appdata) = std::env::var("APPDATA") {
                Ok(PathBuf::from(appdata).join("Hercules"))
            } else {
                Ok(PathBuf::from("C:\\ProgramData\\Hercules"))
            }
        } else {
            if let Ok(home) = std::env::var("HOME") {
                Ok(PathBuf::from(home).join(".config").join("hercules"))
            } else {
                Ok(PathBuf::from("/etc/hercules"))
            }
        }
    }

    fn load_config(path: &PathBuf) -> Result<HerculesConfig> {
        let content = fs::read_to_string(path)?;
        let config: HerculesConfig = toml::from_str(&content)?;
        Ok(config)
    }

    fn save_config(path: &PathBuf, config: &HerculesConfig) -> Result<()> {
        let toml_string = toml::to_string_pretty(config)?;
        fs::write(path, toml_string)?;
        Ok(())
    }

    // Handle the CLI configuration command with exact syntax: "hercules conf <property> -> <new value>"
    pub fn handle_conf_command(args: &[String]) -> Result<()> {
        if args.len() < 4 || args[2] != "->" {
            return Err(anyhow!(
                "Invalid syntax. Use: hercules conf <property> -> <new_value>\n\
                 Examples:\n\
                   hercules conf update_interval_ms -> 500\n\
                   hercules conf show_sensors -> true\n\
                   hercules conf show_compact_mode -> false"
            ));
        }

        let property = &args[1];
        let new_value = &args[3];

        let mut config_manager = ConfigManager::new()?;

        match Self::set_property(&mut config_manager.config, property, new_value) {
            Ok(()) => {
                config_manager.save()?;
                println!("✓ Configuration updated: {} -> {}", property, new_value);
                println!(
                    "  Config saved to: {}",
                    config_manager.config_path.display()
                );
            }
            Err(e) => {
                return Err(anyhow!("Failed to set property '{}': {}", property, e));
            }
        }

        Ok(())
    }

    // Set a property value by string
    fn set_property(config: &mut HerculesConfig, property: &str, value: &str) -> Result<()> {
        match property {
            "update_interval_ms" => {
                config.update_interval_ms = value
                    .parse::<u64>()
                    .map_err(|_| anyhow!("Invalid number format for update_interval_ms"))?;
            }
            "show_cpu" => {
                config.show_cpu = Self::parse_bool(value)?;
            }
            "show_memory" => {
                config.show_memory = Self::parse_bool(value)?;
            }
            "show_disk" => {
                config.show_disk = Self::parse_bool(value)?;
            }
            "show_network" => {
                config.show_network = Self::parse_bool(value)?;
            }
            "show_processes" => {
                config.show_processes = Self::parse_bool(value)?;
            }
            "max_processes" => {
                config.max_processes = value
                    .parse::<usize>()
                    .map_err(|_| anyhow!("Invalid number format for max_processes"))?;
            }
            "continuous" => {
                config.continuous = Self::parse_bool(value)?;
            }
            "show_compact_mode" => {
                config.show_compact_mode = Self::parse_bool(value)?;
            }
            "show_installer" => {
                config.show_installer = Self::parse_bool(value)?;
            }
            "show_sensors" => {
                config.show_sensors = Self::parse_bool(value)?;
                config.sensor_config.enabled = config.show_sensors;
            }
            "sensor_update_interval_ms" => {
                config.sensor_config.update_interval_ms = value
                    .parse::<u64>()
                    .map_err(|_| anyhow!("Invalid number format for sensor_update_interval_ms"))?;
            }
            "sensor_use_celsius" => {
                config.sensor_config.use_celsius = Self::parse_bool(value)?;
            }
            _ => {
                return Err(anyhow!(
                    "Unknown property '{}'. Available properties:\n{}",
                    property,
                    Self::list_available_properties()
                ));
            }
        }
        Ok(())
    }

    fn parse_bool(value: &str) -> Result<bool> {
        match value.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" | "enable" | "enabled" => Ok(true),
            "false" | "0" | "no" | "off" | "disable" | "disabled" => Ok(false),
            _ => Err(anyhow!(
                "Invalid boolean value '{}'. Use: true/false, 1/0, yes/no, on/off, enable/disable",
                value
            )),
        }
    }

    fn list_available_properties() -> String {
        let properties = vec![
            (
                "update_interval_ms",
                "Update interval in milliseconds (number)",
            ),
            ("show_cpu", "Show CPU information (true/false)"),
            ("show_memory", "Show memory information (true/false)"),
            ("show_disk", "Show disk information (true/false)"),
            ("show_network", "Show network information (true/false)"),
            ("show_processes", "Show process information (true/false)"),
            ("max_processes", "Maximum processes to show (number)"),
            ("continuous", "Run in continuous mode (true/false)"),
            ("show_compact_mode", "Use compact display mode (true/false)"),
            ("show_installer", "Show installer options (true/false)"),
            ("show_sensors", "Enable sensor monitoring (true/false)"),
            (
                "sensor_update_interval_ms",
                "Sensor update interval in milliseconds (number)",
            ),
            (
                "sensor_use_celsius",
                "Use Celsius for sensor temperature (true/false)",
            ),
        ];

        properties
            .iter()
            .map(|(prop, desc)| format!("  {:<25} - {}", prop, desc))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // Display current configuration
    pub fn display_config() -> Result<()> {
        let config_manager = ConfigManager::new()?;
        let config = &config_manager.config;

        println!("🔧 Hercules Configuration");
        println!("========================");
        println!("Config file: {}", config_manager.config_path.display());
        println!();

        println!("📊 Display Settings:");
        println!("  update_interval_ms      = {}", config.update_interval_ms);
        println!("  show_cpu               = {}", config.show_cpu);
        println!("  show_memory            = {}", config.show_memory);
        println!("  show_disk              = {}", config.show_disk);
        println!("  show_network           = {}", config.show_network);
        println!("  show_processes         = {}", config.show_processes);
        println!("  max_processes          = {}", config.max_processes);
        println!("  continuous             = {}", config.continuous);
        println!("  show_compact_mode      = {}", config.show_compact_mode);
        println!("  show_installer         = {}", config.show_installer);
        println!();

        println!("🔬 Sensor Settings:");
        println!("  show_sensors           = {}", config.show_sensors);
        println!(
            "  sensor_update_interval_ms = {}",
            config.sensor_config.update_interval_ms
        );
        println!(
            "  sensor_use_celsius     = {}",
            config.sensor_config.use_celsius
        );
        println!();

        println!("💡 Usage Examples:");
        println!("  hercules conf show_sensors -> true");
        println!("  hercules conf update_interval_ms -> 500");
        println!("  hercules conf show_compact_mode -> false");
        println!("  hercules conf max_processes -> 15");

        Ok(())
    }

    // Reset configuration to defaults
    pub fn reset_config() -> Result<()> {
        let mut config_manager = ConfigManager::new()?;
        config_manager.config = HerculesConfig::default();
        config_manager.save()?;
        println!("✓ Configuration reset to defaults");
        println!("  Config file: {}", config_manager.config_path.display());
        Ok(())
    }
}

// Convert HerculesConfig to MonitorConfig for backward compatibility
impl From<&HerculesConfig> for crate::MonitorConfig {
    fn from(config: &HerculesConfig) -> Self {
        crate::MonitorConfig {
            update_interval_ms: config.update_interval_ms,
            show_cpu: config.show_cpu,
            show_memory: config.show_memory,
            show_disk: config.show_disk,
            show_network: config.show_network,
            show_processes: config.show_processes,
            max_processes: config.max_processes,
            continuous: config.continuous,
            show_compact_mode: config.show_compact_mode,
            show_installer: config.show_installer,
            show_sensors: config.show_sensors,
            sensor_config: config.sensor_config.clone(),
        }
    }
}
