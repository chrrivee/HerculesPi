# HERCULES - System Resource Monitor


A powerful system resource monitor built in Rust with a compact mode that resembles neofetch. Supports both Windows and Linux.

## Features

- Real-time monitoring of system resources
- Detailed CPU usage statistics (overall and per-core)
- Memory usage tracking
- Disk usage information
- Network transfer rates
- Process monitoring
- Gyroscope and accelerometer monitoring via USB
- Beautiful compact display mode with Intel CPU ASCII art
- Cross-platform support (Windows and Linux)

## Installation

### Build from Source

Clone the repository and build with Cargo:

```bash
git clone https://github.com/yourusername/hercules.git
cd hercules
cargo build --release
```

The built executable will be available at `target/release/hercules`.

### Installer

You can also use the built-in installer:

#### Windows
```bash
hercules --installer
```

This will install Hercules to `C:\Program Files\hercules` and create a desktop shortcut.

#### Linux
```bash
sudo hercules --installer
```

This will install Hercules to `/usr/local/bin/hercules` and create a desktop entry.

## Usage

### Standard Mode

Run HERCULES in standard mode to see detailed system information:

```bash
hercules
```

Or if running from source:

```bash
cargo run
```

### Compact Mode

Run HERCULES in compact mode to see a neofetch-like display with Intel CPU ASCII art:

```bash
hercules compact
```

Or if running from source:

```bash
cargo run -- compact
```

The compact mode provides a visually appealing display with:
- Intel CPU ASCII art that changes color based on system load
- System information (OS, kernel, hostname)
- CPU information with usage bars
- Memory usage statistics
- Network transfer rates
- Individual CPU core usage displayed graphically

### Sensor Mode

Enable gyroscope and accelerometer monitoring:

```bash
hercules --sensors
```

Or if running from source:

```bash
cargo run -- --sensors
```

This mode will attempt to detect and read data from USB-connected gyroscopes and accelerometers.

## Configuration

Configuration is done through command-line arguments. More customization options will be available in future releases.

## Requirements

### Windows
- Rust 1.56.0 or higher
- Standard system libraries for retrieving system information
- Optional: USB-connected gyroscope/accelerometer for sensor monitoring

### Linux
- Rust 1.56.0 or higher
- libudev-dev package for USB device access
- libusb-1.0-0-dev for USB communication
- Optional: USB-connected gyroscope/accelerometer for sensor monitoring

On Debian/Ubuntu systems, install dependencies with:
```bash
sudo apt install libudev-dev libusb-1.0-0-dev
```

On Fedora/RHEL systems:
```bash
sudo dnf install systemd-devel libusb1-devel
```

## Supported Sensor Devices

Hercules supports various USB-connected gyroscope and accelerometer devices, including:

- MPU-6050 based adapters
- Arduino Leonardo with IMU shields
- SparkFun 9DoF sensors
- Many gaming controllers with gyro (like DualShock 4, Nintendo Switch Pro)
- Other HID devices that identify as gyroscopes or accelerometers

## License

MIT License

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
