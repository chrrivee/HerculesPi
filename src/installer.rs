use std::fs;
use std::io::{self, Write};
use std::error::Error;
use std::path::Path;
use std::process;
use std::env;
use std::fs::File;

#[cfg(target_os = "windows")]
use std::ffi::OsString;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
#[cfg(target_os = "windows")]
use std::ptr::null_mut;
#[cfg(target_os = "windows")]
use winapi::um::shellapi::ShellExecuteW;
#[cfg(target_os = "windows")]
use winapi::um::winuser::{SW_SHOW, MB_OK, MB_ICONINFORMATION, MessageBoxW};
#[cfg(target_os = "windows")]
use is_elevated::is_elevated;

#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use users::get_current_uid;
#[cfg(target_os = "linux")]
use users::os::unix::UserExt;

#[cfg(target_os = "windows")]
fn show_message_box(title: &str, message: &str, is_success: bool) {
    let title_wide: Vec<u16> = OsString::from(title)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    let message_wide: Vec<u16> = OsString::from(message)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    let icon = if is_success { MB_ICONINFORMATION } else { MB_ICONINFORMATION };
    
    unsafe {
        MessageBoxW(
            null_mut(),
            message_wide.as_ptr(),
            title_wide.as_ptr(),
            MB_OK | icon
        );
    }
    
    log_message(&format!("Displayed message box: {} - {}", title, message));
}

#[cfg(target_os = "linux")]
fn show_message_box(title: &str, message: &str, is_success: bool) {
    // On Linux, we just print to the console
    println!("\n{} {}", if is_success { "✓" } else { "!" }, title);
    println!("{}", message);
    println!("");
    
    log_message(&format!("Displayed message: {} - {}", title, message));
}

fn create_log_file(initial_message: &str) -> Result<(), Box<dyn Error>> {
    let user_profile = env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    let log_dir = Path::new(&user_profile).join("AppData").join("Local").join("Hercules");
    
    // Create log directory if it doesn't exist
    if !log_dir.exists() {
        fs::create_dir_all(&log_dir)?;
    }
    
    let log_file_path = log_dir.join("installer_log.txt");
    let mut log_file = if log_file_path.exists() {
        OpenOptions::new().append(true).open(&log_file_path)?
    } else {
        File::create(&log_file_path)?
    };
    
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    writeln!(log_file, "\n========== {} ==========", timestamp)?;
    writeln!(log_file, "{}", initial_message)?;
    
    Ok(())
}

fn log_message(message: &str) {
    let user_profile = match env::var("USERPROFILE") {
        Ok(profile) => profile,
        Err(_) => return, // Skip logging if we can't get the user profile
    };
    
    let log_dir = Path::new(&user_profile).join("AppData").join("Local").join("Hercules");
    let log_file_path = log_dir.join("installer_log.txt");
    
    // Don't try to create the directory here, as it should have been created by create_log_file
    // Just append to the file if it exists
    if let Ok(mut log_file) = fs::OpenOptions::new().append(true).open(log_file_path) {
        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
        let _ = writeln!(log_file, "[{}] {}", timestamp, message);
    }
}

use std::fs::OpenOptions;


pub fn prompt_install() -> ! {
    println!("========================================");
    println!("HERCULES SYSTEM MONITOR - INSTALLER");
    println!("========================================");
    
    // Create log file
    let _ = create_log_file("Starting Hercules installer");
    
    #[cfg(target_os = "windows")]
    if !is_elevated() {
        log_message("Not running with admin privileges. Requesting elevation...");
        println!("Administrator privileges required for installation.");
        println!("Requesting elevation...");
        
        if let Err(e) = request_elevation() {
            let error_msg = format!("Failed to elevate privileges: {}", e);
            log_message(&error_msg);
            eprintln!("{}", error_msg);
            println!("Please right-click and select 'Run as administrator' to install.");
            
            // Pause to let the user read the message
            println!("Press Enter to exit...");
            let mut input = String::new();
            let _ = io::stdin().read_line(&mut input);
            process::exit(1);
        }
        
        // If we reach here, a new elevated process has been started
        // We should exit this non-elevated process
        log_message("Elevation requested. Exiting non-elevated process.");
        process::exit(0);
    }
    
    #[cfg(target_os = "linux")]
    if get_current_uid() != 0 {
        log_message("Not running with root privileges. Requesting elevation...");
        println!("Root privileges required for installation.");
        println!("Requesting elevation using sudo...");
        
        if let Err(e) = request_elevation_linux() {
            let error_msg = format!("Failed to elevate privileges: {}", e);
            log_message(&error_msg);
            eprintln!("{}", error_msg);
            println!("Please run the installer with sudo to install.");
            
            // Pause to let the user read the message
            println!("Press Enter to exit...");
            let mut input = String::new();
            let _ = io::stdin().read_line(&mut input);
            process::exit(1);
        }
        
        // If we reach here, a new elevated process has been started
        // We should exit this non-elevated process
        log_message("Elevation requested. Exiting non-elevated process.");
        process::exit(0);
    }
    
    log_message("Running with administrator/root privileges");
    #[cfg(target_os = "windows")]
    let install_dir = "C:\\Program Files\\hercules";
    
    #[cfg(target_os = "linux")]
    let install_dir = "/usr/local/bin/hercules";
    
    if let Err(e) = run_installer(install_dir) {
        let error_msg = format!("Installation failed: {}", e);
        log_message(&error_msg);
        eprintln!("{}", error_msg);
        
        // Show error popup
        show_message_box("Hercules Installation", &format!("Installation failed: {}", e), false);
        process::exit(1);
    }
    
    log_message("Installation completed successfully!");
    println!("Installation completed successfully!");
    println!("Exiting installer.");
    
    // Show success popup
    show_message_box("Hercules Installation", "Installation completed successfully!", true);
    
    process::exit(0);
}

#[cfg(target_os = "windows")]
fn request_elevation() -> Result<(), Box<dyn Error>> {
    // Get the path to the current executable
    let current_exe = env::current_exe()?;
    let current_exe_str = current_exe.to_str().ok_or("Failed to convert path to string")?;
    
    // Convert to wide string for Windows API
    let wide_exe: Vec<u16> = OsString::from(current_exe_str)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    let wide_operation: Vec<u16> = OsString::from("runas")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    // Add --installer parameter to ensure we run the installer when elevated
    let wide_params: Vec<u16> = OsString::from("--installer")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    // Call ShellExecuteW to start a new process with elevated privileges
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            wide_operation.as_ptr(),
            wide_exe.as_ptr(),
            wide_params.as_ptr(),
            null_mut(),
            SW_SHOW,
        )
    };
    
    // Check if the elevation request was successful
    if result as usize <= 32 {
        return Err("Failed to request elevation".into());
    }
    
    Ok(())
}

#[cfg(target_os = "linux")]
fn request_elevation_linux() -> Result<(), Box<dyn Error>> {
    // Get the path to the current executable
    let current_exe = env::current_exe()?;
    
    // Use sudo to re-run the current executable with root privileges
    let status = Command::new("sudo")
        .arg(current_exe)
        .arg("--installer")
        .status()?;
    
    if !status.success() {
        return Err(format!("sudo exited with status: {}", status).into());
    }
    
    Ok(())
}

fn run_installer(install_dir: &str) -> Result<(), Box<dyn Error>> {
    println!("Checking for previous installation...");
    log_message(&format!("Checking for previous installation at: {}", install_dir));
    
    if check_previous_installation(install_dir) {
        log_message(&format!("Previous installation detected at: {}", install_dir));
        println!("Previous installation detected at: {}", install_dir);
        println!("Options: [r]epair, [u]ninstall, [c]ancel");
        
        let mut input = String::new();
        io::stdout().flush()?;
        io::stdin().read_line(&mut input)?;
        
        let choice = input.trim().to_lowercase();
        log_message(&format!("User selected: {}", choice));
        
        match choice.as_str() {
            "r" | "repair" => {
                println!("Repairing installation...");
                log_message("Starting repair process");
                uninstall(install_dir)?;
                install(install_dir)?;
                log_message("Repair process completed");
                
                // Show success popup
                show_message_box("Hercules Installation", "Repair completed successfully!\nYou can now run 'hercules' from any command prompt.", true);
            },
            "u" | "uninstall" => {
                println!("Uninstalling...");
                log_message("Starting uninstall process");
                uninstall(install_dir)?;
                log_message("Uninstallation completed successfully");
                println!("Uninstallation complete.");
                
                // Show success popup
                show_message_box("Hercules Uninstallation", "Uninstallation completed successfully!", true);
                return Ok(());
            },
            _ => {
                println!("Installation cancelled.");
                log_message("Installation cancelled by user");
                
                // Show cancellation popup
                show_message_box("Hercules Installation", "Installation cancelled by user.", false);
                return Ok(());
            }
        }
    } else {
        log_message("No previous installation found");
        println!("No previous installation found.");
        println!("Would you like to install Hercules? [y/n]");
        
        let mut input = String::new();
        io::stdout().flush()?;
        io::stdin().read_line(&mut input)?;
        
        let choice = input.trim().to_lowercase();
        log_message(&format!("User selected: {}", choice));
        
        if choice == "y" {
            log_message("Starting new installation");
            install(install_dir)?;
        } else {
            println!("Installation cancelled.");
            log_message("Installation cancelled by user");
            
            // Show cancellation popup
            show_message_box("Hercules Installation", "Installation cancelled by user.", false);
            return Ok(());
        }
    }
    
    Ok(())
}

fn check_previous_installation(directory: &str) -> bool {
    let path = Path::new(directory);
    
    if !path.exists() {
        return false;
    }
    
    match fs::read_dir(directory) {
        Ok(entries) => {
            let entries: Vec<_> = entries.filter_map(Result::ok).collect();
            !entries.is_empty()
        },
        Err(_) => false
    }
}

fn install(install_dir: &str) -> Result<(), Box<dyn Error>> {
    println!("Installing Hercules to: {}", install_dir);
    log_message(&format!("Installing Hercules to: {}", install_dir));
    
    // Create installation directory if it doesn't exist
    match fs::create_dir_all(install_dir) {
        Ok(_) => {
            println!("Created installation directory successfully");
            log_message("Created installation directory successfully");
        },
        Err(e) => {
            let error_msg = format!("Error creating installation directory: {}", e);
            println!("{}", error_msg);
            log_message(&error_msg);
            
            #[cfg(target_os = "windows")]
            if !is_elevated() {
                println!("This error may be due to insufficient permissions.");
                println!("Please run the installer as Administrator.");
                log_message("Insufficient permissions - Administrator rights required");
                return Err("Insufficient permissions".into());
            }
            
            #[cfg(target_os = "linux")]
            if get_current_uid() != 0 {
                println!("This error may be due to insufficient permissions.");
                println!("Please run the installer with sudo.");
                log_message("Insufficient permissions - Root permissions required");
                return Err("Insufficient permissions".into());
            }
            
            return Err(e.into());
        }
    }
    
    // Get path to current executable
    let current_exe = env::current_exe()?;
    println!("Current executable: {:?}", current_exe);
    log_message(&format!("Current executable: {:?}", current_exe));
    
    // Copy executable to installation directory
    let target_exe = Path::new(install_dir).join("hercules.exe");
    
    println!("Copying executable to installation directory...");
    log_message("Copying executable to installation directory...");
    
    match fs::copy(&current_exe, &target_exe) {
        Ok(_) => {
            println!("Copied executable successfully");
            log_message("Copied executable successfully");
        },
        Err(e) => {
            let error_msg = format!("Error copying executable: {}", e);
            println!("{}", error_msg);
            log_message(&error_msg);
            
            #[cfg(target_os = "windows")]
            if !is_elevated() {
                println!("This error may be due to insufficient permissions.");
                println!("Please run the installer as Administrator.");
                log_message("Insufficient permissions - Administrator rights required");
                return Err("Insufficient permissions".into());
            }
            
            #[cfg(target_os = "linux")]
            if get_current_uid() != 0 {
                println!("This error may be due to insufficient permissions.");
                println!("Please run the installer with sudo.");
                log_message("Insufficient permissions - Root permissions required");
                return Err("Insufficient permissions".into());
            }
            
            return Err(e.into());
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        // Make the executable file executable
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&target_exe)?.permissions();
        perms.set_mode(0o755); // rwxr-xr-x
        fs::set_permissions(&target_exe, perms)?;
        log_message("Set executable permissions on Linux");
    }
    
    // Create desktop shortcut
    create_desktop_shortcut(&target_exe)?;
    
    // Create uninstaller info
    create_uninstaller_info(install_dir, &target_exe)?;
    
    println!("Installation successful!");
    println!("Executable installed to: {:?}", target_exe);
    log_message("Installation completed successfully");
    
    Ok(())
}

fn uninstall(install_dir: &str) -> Result<(), Box<dyn Error>> {
    println!("Uninstalling Hercules from: {}", install_dir);
    log_message(&format!("Uninstalling Hercules from: {}", install_dir));
    
    // Remove desktop shortcut
    if let Ok(desktop_path) = env::var("USERPROFILE") {
        let shortcut_path = Path::new(&desktop_path).join("Desktop").join("Hercules System Monitor.lnk");
        if shortcut_path.exists() {
            println!("Removing desktop shortcut...");
            log_message(&format!("Removing desktop shortcut: {:?}", shortcut_path));
            match fs::remove_file(&shortcut_path) {
                Ok(_) => log_message("Desktop shortcut removed successfully"),
                Err(e) => log_message(&format!("Error removing desktop shortcut: {}", e))
            }
        }
    }
    
    // Remove installation directory and all contents
    if Path::new(install_dir).exists() {
        println!("Removing installation directory...");
        log_message(&format!("Removing installation directory: {}", install_dir));
        match fs::remove_dir_all(install_dir) {
            Ok(_) => log_message("Installation directory removed successfully"),
            Err(e) => {
                let error_msg = format!("Error removing installation directory: {}", e);
                log_message(&error_msg);
                return Err(e.into());
            }
        }
    }
    
    println!("Uninstallation successful!");
    log_message("Uninstallation completed successfully");
    
    Ok(())
}

#[cfg(target_os = "windows")]
fn create_desktop_shortcut(target_exe: &Path) -> Result<(), Box<dyn Error>> {
    println!("Creating desktop shortcut...");
    log_message("Creating desktop shortcut...");
    
    if let Ok(desktop_path) = env::var("USERPROFILE") {
        let desktop_dir = Path::new(&desktop_path).join("Desktop");
        
        // This is a simplified version since creating actual .lnk files requires Windows API
        // In a real application, you would use the Windows API or a crate like 'windows-shortcut-rs'
        let shortcut_path = desktop_dir.join("Hercules System Monitor.lnk");
        
        // For demonstration, we'll create a simple text file that points to the executable
        match File::create(&shortcut_path) {
            Ok(mut shortcut_file) => {
                write!(shortcut_file, "Target: {}", target_exe.display())?;
                println!("Desktop shortcut created at: {:?}", shortcut_path);
                log_message(&format!("Desktop shortcut created at: {:?}", shortcut_path));
            },
            Err(e) => {
                let error_msg = format!("Error creating desktop shortcut: {}", e);
                println!("{}", error_msg);
                log_message(&error_msg);
                return Err(e.into());
            }
        }
    } else {
        let msg = "Couldn't determine user profile path, skipping desktop shortcut creation.";
        println!("{}", msg);
        log_message(msg);
    }
    
    Ok(())
}

#[cfg(target_os = "linux")]
fn create_desktop_shortcut(target_exe: &Path) -> Result<(), Box<dyn Error>> {
    println!("Creating desktop shortcut...");
    log_message("Creating desktop shortcut...");
    
    // Get the home directory
    if let Some(home_dir) = dirs::home_dir() {
        let desktop_dir = home_dir.join("Desktop");
        
        // Create .desktop file in the user's desktop directory
        if desktop_dir.exists() {
            let desktop_file_path = desktop_dir.join("hercules.desktop");
            
            match File::create(&desktop_file_path) {
                Ok(mut desktop_file) => {
                    writeln!(desktop_file, "[Desktop Entry]")?;
                    writeln!(desktop_file, "Name=Hercules System Monitor")?;
                    writeln!(desktop_file, "Exec={}", target_exe.display())?;
                    writeln!(desktop_file, "Icon=utilities-system-monitor")?;
                    writeln!(desktop_file, "Terminal=false")?;
                    writeln!(desktop_file, "Type=Application")?;
                    writeln!(desktop_file, "Categories=System;Monitor;")?;
                    
                    // Make the desktop file executable
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = fs::metadata(&desktop_file_path)?.permissions();
                    perms.set_mode(0o755); // rwxr-xr-x
                    fs::set_permissions(&desktop_file_path, perms)?;
                    
                    println!("Desktop shortcut created at: {:?}", desktop_file_path);
                    log_message(&format!("Desktop shortcut created at: {:?}", desktop_file_path));
                },
                Err(e) => {
                    let error_msg = format!("Error creating desktop shortcut: {}", e);
                    println!("{}", error_msg);
                    log_message(&error_msg);
                    return Err(e.into());
                }
            }
        } else {
            let msg = "Desktop directory not found, skipping desktop shortcut creation.";
            println!("{}", msg);
            log_message(msg);
        }
    } else {
        let msg = "Couldn't determine home directory, skipping desktop shortcut creation.";
        println!("{}", msg);
        log_message(msg);
    }
    
    Ok(())
}

fn create_uninstaller_info(install_dir: &str, target_exe: &Path) -> Result<(), Box<dyn Error>> {
    println!("Creating uninstaller information...");
    log_message("Creating uninstaller information...");
    
    let uninstall_info_path = Path::new(install_dir).join("uninstall_info.txt");
    
    match File::create(&uninstall_info_path) {
        Ok(mut uninstall_file) => {
            writeln!(uninstall_file, "Hercules System Monitor")?;
            writeln!(uninstall_file, "Installation Path: {}", install_dir)?;
            writeln!(uninstall_file, "Executable Path: {}", target_exe.display())?;
            writeln!(uninstall_file, "Installation Date: {}", chrono::Local::now())?;
            
            println!("Uninstaller information created at: {:?}", uninstall_info_path);
            log_message(&format!("Uninstaller information created at: {:?}", uninstall_info_path));
        },
        Err(e) => {
            let error_msg = format!("Error creating uninstaller information: {}", e);
            println!("{}", error_msg);
            log_message(&error_msg);
            return Err(e.into());
        }
    }
    
    Ok(())
}

