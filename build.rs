fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    
    // Detect the target OS
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| "unknown".to_string());
    
    if target_os == "windows" {
        println!("cargo:rustc-cfg=platform=\"windows\"");
        // Windows-specific build configuration
        println!("cargo:rustc-link-lib=user32");
        println!("cargo:rustc-link-lib=shell32");
    } else if target_os == "linux" {
        println!("cargo:rustc-cfg=platform=\"linux\"");
        // Linux-specific build configuration
        // Add any Linux-specific linker flags if needed
    } else if target_os == "macos" {
        println!("cargo:rustc-cfg=platform=\"macos\"");
        // macOS-specific build configuration
    } else {
        println!("cargo:warning=Building for unsupported platform: {}", target_os);
    }
    
    // Add USB HID device support (platform agnostic)
    #[cfg(target_os = "linux")]
    {
        // On Linux, we might need to link against libusb or libudev
        println!("cargo:rustc-link-lib=udev");
    }
    
    // Print information about the build
    println!("cargo:warning=Building Hercules for {}", target_os);
}