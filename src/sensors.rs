use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use crossbeam_channel::{bounded, Receiver};
use hidapi::{HidApi, HidDevice};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};

// Common sensor data structure
#[derive(Debug, Clone, Copy)]
pub struct SensorData {
    pub timestamp: Instant,
    pub acceleration: [f32; 3], // x, y, z in m/s²
    pub gyro: [f32; 3],         // x, y, z in deg/s
    pub orientation: [f32; 3],  // roll, pitch, yaw in degrees
    pub temperature: f32,       // in °C
}

impl Default for SensorData {
    fn default() -> Self {
        SensorData {
            timestamp: Instant::now(),
            acceleration: [0.0; 3],
            gyro: [0.0; 3],
            orientation: [0.0; 3],
            temperature: 0.0,
        }
    }
}

// Sensor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorConfig {
    pub enabled: bool,
    pub update_interval_ms: u64,
    #[allow(dead_code)]
    pub use_celsius: bool,
}

impl Default for SensorConfig {
    fn default() -> Self {
        SensorConfig {
            enabled: false,
            update_interval_ms: 100,
            use_celsius: true,
        }
    }
}

// Sensor error type
#[derive(Debug)]
pub enum SensorError {
    NotFound,
    #[allow(dead_code)]
    ConnectionFailed(String),
    ReadError(String),
    #[allow(dead_code)]
    Disconnected,
    #[allow(dead_code)]
    InitializationFailed(String),
}

impl fmt::Display for SensorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SensorError::NotFound => write!(f, "No compatible sensor found"),
            SensorError::ConnectionFailed(s) => write!(f, "Failed to connect to sensor: {}", s),
            SensorError::ReadError(s) => write!(f, "Failed to read from sensor: {}", s),
            SensorError::Disconnected => write!(f, "Sensor disconnected"),
            SensorError::InitializationFailed(s) => {
                write!(f, "Failed to initialize sensor: {}", s)
            }
        }
    }
}

impl Error for SensorError {}

// Sensor manager to handle connection and data collection
pub struct SensorManager {
    data: Arc<Mutex<SensorData>>,
    config: SensorConfig,
    receiver: Option<Receiver<Result<SensorData, SensorError>>>,
}

impl SensorManager {
    pub fn new(config: SensorConfig) -> Self {
        SensorManager {
            data: Arc::new(Mutex::new(SensorData::default())),
            config,
            receiver: None,
        }
    }

    pub fn start(&mut self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        info!("Starting sensor monitoring");

        // Try to initialize HidApi
        let api = match HidApi::new() {
            Ok(api) => api,
            Err(e) => {
                error!("Failed to initialize HID API: {}", e);
                return Err(anyhow!("Failed to initialize HID API: {}", e));
            }
        };

        // Look for supported devices
        let device = self.find_supported_sensor(&api)?;

        // Create channel for sensor data
        let (sender, receiver) = bounded(10);
        self.receiver = Some(receiver);

        // Clone necessary data for the thread
        let update_interval = self.config.update_interval_ms;
        let data_clone = self.data.clone();

        // Spawn a thread to continuously read sensor data
        thread::spawn(move || {
            let mut last_data = SensorData::default();

            loop {
                match read_sensor_data(&device) {
                    Ok(sensor_data) => {
                        // Update the shared data
                        if let Ok(mut data) = data_clone.lock() {
                            *data = sensor_data;
                        }

                        // Send the data through the channel
                        if sender.send(Ok(sensor_data)).is_err() {
                            // Receiver dropped, exit thread
                            break;
                        }

                        last_data = sensor_data;
                    }
                    Err(e) => {
                        error!("Error reading sensor data: {}", e);

                        // Send the error through the channel
                        if sender.send(Err(e)).is_err() {
                            // Receiver dropped, exit thread
                            break;
                        }

                        // Continue with last known good data
                        if let Ok(mut data) = data_clone.lock() {
                            *data = last_data;
                        }
                    }
                }

                thread::sleep(Duration::from_millis(update_interval));
            }
        });

        Ok(())
    }

    #[allow(dead_code)]
    pub fn get_latest_data(&self) -> SensorData {
        if let Ok(data) = self.data.lock() {
            *data
        } else {
            SensorData::default()
        }
    }

    pub fn try_receive_update(&self) -> Option<Result<SensorData, SensorError>> {
        if let Some(ref receiver) = self.receiver {
            receiver.try_recv().ok()
        } else {
            None
        }
    }

    fn find_supported_sensor(&self, api: &HidApi) -> Result<HidDevice, SensorError> {
        // List of supported sensors by vendor_id, product_id, and description
        let supported_sensors = [
            // MPU-6050 based USB adapters
            (0x16c0, 0x0486, "MPU-6050"),
            // Common IMU adapters
            (0x2341, 0x8036, "Arduino Leonardo"), // Arduino with IMU shield
            (0x1b4f, 0x9206, "SparkFun 9DoF"),    // SparkFun 9DoF sensor
            // Mainstream gaming controllers with gyro (for testing)
            (0x054c, 0x09cc, "Sony DualShock 4"), // PS4 controller
            (0x057e, 0x2009, "Nintendo Switch Pro Controller"),
        ];

        // First try to find exact matches for supported sensors
        for &(vendor_id, product_id, description) in &supported_sensors {
            debug!(
                "Looking for sensor: {} ({:04x}:{:04x})",
                description, vendor_id, product_id
            );

            if let Ok(device) = api.open(vendor_id, product_id) {
                info!("Found supported sensor: {}", description);
                return Ok(device);
            }
        }

        // If no exact match found, list all available HID devices for debugging
        debug!("No exact match found, listing all available HID devices");
        for device_info in api.device_list() {
            debug!(
                "HID Device: {:04x}:{:04x} - {} [{}]",
                device_info.vendor_id(),
                device_info.product_id(),
                device_info.product_string().unwrap_or("Unknown"),
                device_info.manufacturer_string().unwrap_or("Unknown")
            );

            // Try to detect if it might be an IMU/gyro device by name
            let product = device_info.product_string().unwrap_or("").to_lowercase();
            let manufacturer = device_info
                .manufacturer_string()
                .unwrap_or("")
                .to_lowercase();

            if product.contains("gyro")
                || product.contains("accel")
                || product.contains("imu")
                || product.contains("motion")
                || manufacturer.contains("gyro")
                || manufacturer.contains("accel")
            {
                info!(
                    "Found potential IMU device: {} from {}",
                    device_info.product_string().unwrap_or("Unknown"),
                    device_info.manufacturer_string().unwrap_or("Unknown")
                );

                if let Ok(device) = api.open(device_info.vendor_id(), device_info.product_id()) {
                    return Ok(device);
                }
            }
        }

        // No supported device found
        error!("No supported sensor found");
        Err(SensorError::NotFound)
    }
}

fn read_sensor_data(device: &HidDevice) -> Result<SensorData, SensorError> {
    let mut buf = [0u8; 64]; // Common buffer size for HID devices

    // Read data from the device
    match device.read_timeout(&mut buf, 100) {
        Ok(size) if size > 0 => {
            debug!("Read {} bytes from sensor", size);

            // Parse the data based on generic IMU format
            // This is a simplified implementation - in reality, you'd need specific parsing
            // for each supported device based on its protocol
            let mut data = SensorData::default();

            // Example parsing (adjust based on actual device protocol)
            if size >= 16 {
                // Acceleration (assuming bytes 0-11 contain accel data as 3 floats)
                data.acceleration[0] = parse_float(&buf[0..4]);
                data.acceleration[1] = parse_float(&buf[4..8]);
                data.acceleration[2] = parse_float(&buf[8..12]);

                // Gyro (assuming bytes 12-23 contain gyro data as 3 floats)
                if size >= 24 {
                    data.gyro[0] = parse_float(&buf[12..16]);
                    data.gyro[1] = parse_float(&buf[16..20]);
                    data.gyro[2] = parse_float(&buf[20..24]);
                }

                // Temperature (if available)
                if size >= 28 {
                    data.temperature = parse_float(&buf[24..28]);
                }

                // Orientation (if available)
                if size >= 40 {
                    data.orientation[0] = parse_float(&buf[28..32]);
                    data.orientation[1] = parse_float(&buf[32..36]);
                    data.orientation[2] = parse_float(&buf[36..40]);
                }
            } else {
                // Simple data format fallback - try to extract at least some information
                // This is highly device-specific and may need adjustment
                if size >= 6 {
                    // Try to interpret as simple 16-bit per axis format
                    data.acceleration[0] =
                        (((buf[0] as i16) << 8) | buf[1] as i16) as f32 / 16384.0;
                    data.acceleration[1] =
                        (((buf[2] as i16) << 8) | buf[3] as i16) as f32 / 16384.0;
                    data.acceleration[2] =
                        (((buf[4] as i16) << 8) | buf[5] as i16) as f32 / 16384.0;

                    if size >= 12 {
                        data.gyro[0] = (((buf[6] as i16) << 8) | buf[7] as i16) as f32 / 131.0;
                        data.gyro[1] = (((buf[8] as i16) << 8) | buf[9] as i16) as f32 / 131.0;
                        data.gyro[2] = (((buf[10] as i16) << 8) | buf[11] as i16) as f32 / 131.0;
                    }
                }
            }

            data.timestamp = Instant::now();
            Ok(data)
        }
        Ok(_) => {
            warn!("Read 0 bytes from sensor");
            Err(SensorError::ReadError("Zero bytes read".to_string()))
        }
        Err(e) => {
            error!("Failed to read from sensor: {}", e);
            Err(SensorError::ReadError(e.to_string()))
        }
    }
}

// Helper function to convert 4 bytes to a float
fn parse_float(bytes: &[u8]) -> f32 {
    if bytes.len() < 4 {
        return 0.0;
    }

    let bits = (bytes[0] as u32)
        | ((bytes[1] as u32) << 8)
        | ((bytes[2] as u32) << 16)
        | ((bytes[3] as u32) << 24);

    f32::from_bits(bits)
}

// Cross-platform initialization of sensors
pub fn initialize_sensors(config: SensorConfig) -> Result<SensorManager> {
    let mut manager = SensorManager::new(config);

    // Only try to start if enabled
    if manager.config.enabled {
        if let Err(e) = manager.start() {
            warn!("Failed to start sensor monitoring: {}", e);
            // Return the manager anyway - it will operate in a disabled state
        }
    }

    Ok(manager)
}
