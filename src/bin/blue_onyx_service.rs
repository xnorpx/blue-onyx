//! Blue Onyx service.
//!
//! This service loads configuration from blue_onyx_config_service.json
//! If the config file doesn't exist, it creates one with default values.
//!
//! Install the service with proper GPU access:
//!
//! Increase service timeout to 10 minutes for model loading:
//! `reg add "HKLM\SYSTEM\CurrentControlSet\Control" /v ServicesPipeTimeout /t REG_DWORD /d 600000 /f`
//!
//! First, create the event log source (run as Administrator):
//! `New-EventLog -LogName Application -Source BlueOnyxService`
//!
//! Then install the service with LocalSystem for full access:
//! `sc.exe create BlueOnyxService binPath= "<path>\blue_onyx_service.exe" start= auto displayname= "Blue Onyx Service" obj= LocalSystem`
//! `sc.exe config BlueOnyxService type= own`
//!
//! Start the service: `net start BlueOnyxService`
//!
//! Stop the service: `net stop BlueOnyxService`
//!
//! Uninstall the service: `sc.exe delete BlueOnyxService`
//!
//! Configuration is managed via the blue_onyx_config_service.json file in the same directory as the executable.

#[cfg(windows)]
fn main() -> windows_service::Result<()> {
    blue_onyx_service::run()
}

#[cfg(not(windows))]
fn main() {
    panic!("This program is only intended to run on Windows.");
}

#[cfg(windows)]
mod blue_onyx_service {
    use blue_onyx::{
        ServiceResult, blue_onyx_service, cli::Cli, init_service_logging, update_service_log_level,
    };
    use std::{ffi::OsString, future::Future, time::Duration};
    use tokio_util::sync::CancellationToken;
    use tracing::{error, info, warn};
    use windows_service::{
        Result, define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };

    const SERVICE_NAME: &str = "BlueOnyxService";
    const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

    pub fn run() -> Result<()> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)
    }
    define_windows_service!(ffi_service_main, my_service_main);
    pub fn my_service_main(service_name: Vec<OsString>) {
        // Load initial configuration from service config file
        let mut current_args = match Cli::for_service() {
            Ok(args) => args,
            Err(err) => {
                eprintln!("Failed to load service configuration: {err}");
                return;
            }
        };
        // Initialize service logging once
        if let Err(e) = init_service_logging(current_args.log_level) {
            eprintln!("Failed to initialize logging: {e}");
            return;
        }

        // Preload required DLLs for faster startup
        preload_service_dlls();

        // Validate GPU environment for DirectML only if not forcing CPU
        if !current_args.force_cpu {
            validate_gpu_environment();
        } else {
            info!("GPU validation skipped - force_cpu mode is enabled");
        }

        info!(
            "Starting {} service with config from blue_onyx_config_service.json",
            service_name.join(&OsString::from(" ")).to_string_lossy()
        );

        // Print the initial configuration being used
        current_args.print_config();

        // Main service loop with restart support
        loop {
            // Reload configuration on each restart to pick up changes
            if let Ok(updated_args) = Cli::for_service() {
                if updated_args.log_level != current_args.log_level {
                    info!(
                        old_level = ?current_args.log_level,
                        new_level = ?updated_args.log_level,
                        "Log level change detected, applying dynamically"
                    );

                    // Apply the new log level dynamically
                    if let Err(e) = update_service_log_level(updated_args.log_level) {
                        error!("Failed to update log level dynamically: {}", e);
                    }
                }
                current_args = updated_args;
                current_args.print_config();
            } else {
                info!("Using previous configuration (failed to reload config)");
            }
            let (blue_onyx_service, cancellation_token, restart_token) =
                match blue_onyx_service(current_args.clone()) {
                    Ok(v) => v,
                    Err(err) => {
                        error!(
                            ?err,
                            "Failed to init blue onyx service, will retry after delay"
                        );
                        std::thread::sleep(Duration::from_secs(5));
                        continue;
                    }
                };
            let (should_restart, status_handle) =
                match run_service(blue_onyx_service, cancellation_token, restart_token.clone()) {
                    Ok((restart, handle)) => (restart, Some(handle)),
                    Err(err) => {
                        error!(?err, "Blue onyx service failed, will retry after delay");
                        std::thread::sleep(Duration::from_secs(5));
                        (true, None) // Force restart after error
                    }
                };
            if should_restart {
                info!("Restarting Blue Onyx service...");
                // Small delay before restart to avoid rapid restart loops
                std::thread::sleep(Duration::from_secs(1));
            } else {
                info!("Blue Onyx service stopped normally");
                // Set final service status to stopped
                if let Some(handle) = status_handle {
                    let _ = handle.set_service_status(ServiceStatus {
                        service_type: SERVICE_TYPE,
                        current_state: ServiceState::Stopped,
                        controls_accepted: ServiceControlAccept::empty(),
                        exit_code: ServiceExitCode::Win32(0),
                        checkpoint: 0,
                        wait_hint: Duration::default(),
                        process_id: None,
                    });
                }
                break;
            }
        }
    }
    pub fn run_service(
        blue_onyx_service: impl Future<Output = ServiceResult>,
        cancellation_token: CancellationToken,
        restart_token: CancellationToken,
    ) -> anyhow::Result<(
        bool,
        windows_service::service_control_handler::ServiceStatusHandle,
    )> {
        let restart_token_clone = restart_token.clone();
        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                ServiceControl::Stop => {
                    cancellation_token.cancel();
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Shutdown => {
                    cancellation_token.cancel();
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::UserEvent(code) => {
                    match code.to_raw() {
                        130 => {
                            // Stop signal
                            cancellation_token.cancel();
                        }
                        131 => {
                            // Restart signal
                            restart_token_clone.cancel();
                        }
                        _ => {}
                    }
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };
        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

        // Report that we're starting up and give Windows 60 seconds timeout
        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::StartPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(600), // Tell Windows we need up to 60 seconds to start
            process_id: None,
        })?;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        // Now report that we're fully running
        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        let should_restart = rt.block_on(async {
            tokio::select! {
                result = blue_onyx_service => {
                    match result {
                        Ok((restart_requested, worker_handle)) => {
                            // Wait for worker thread to complete if available
                            if let Some(handle) = worker_handle {
                                info!("Waiting for worker thread to complete...");
                                if let Err(e) = handle.join() {
                                    error!("Worker thread panicked: {:?}", e);
                                }
                            }
                            restart_requested
                        },
                        Err(err) => {
                            error!(?err, "Blue onyx service encountered an error");
                            false // Don't restart on error
                        }
                    }
                }
                _ = restart_token.cancelled() => {
                    info!("Restart signal received");
                    true // Restart requested
                }
            }
        }); // Only set service status to Stopped if we're not restarting
        if !should_restart {
            status_handle.set_service_status(ServiceStatus {
                service_type: SERVICE_TYPE,
                current_state: ServiceState::Stopped,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            })?;
        }

        Ok((should_restart, status_handle))
    }
    /// Validate GPU environment for DirectML access in service context
    fn validate_gpu_environment() {
        // Check session information
        info!("Validating GPU environment for service context");

        // Set environment variables for better GPU access
        unsafe { std::env::set_var("DIRECTML_DEBUG", "0") };
        unsafe { std::env::set_var("D3D12_EXPERIMENTAL_SHADER_MODELS", "1") };

        // Validate DirectX 12 availability
        validate_directx12_support();
    }
    /// Validate DirectX 12 support
    fn validate_directx12_support() {
        use windows::Win32::Graphics::Dxgi::*;

        // First try with debug flag which provides more information
        let factory_result =
            unsafe { CreateDXGIFactory2::<IDXGIFactory4>(DXGI_CREATE_FACTORY_DEBUG) };

        // If debug fails, try without debug
        let factory = match factory_result {
            Ok(f) => f,
            Err(e) => {
                info!(
                    "DirectX 12 debug factory creation failed: {:?}. Trying without debug flag.",
                    e
                );
                match unsafe {
                    CreateDXGIFactory2::<IDXGIFactory4>(
                        windows::Win32::Graphics::Dxgi::DXGI_CREATE_FACTORY_FLAGS(0),
                    )
                } {
                    Ok(f) => f,
                    Err(e) => {
                        error!(
                            "DirectX 12 factory creation failed: {:?}. GPU acceleration will not be available.",
                            e
                        );
                        error!(
                            "Possible causes: outdated graphics driver, no compatible GPU, or running in a remote desktop session."
                        );
                        error!("DirectML will fall back to CPU execution.");
                        return;
                    }
                }
            }
        };

        // Check for adapters
        let mut adapter_count = 0;
        let mut has_compatible_adapter = false;

        for i in 0..8 {
            match unsafe { factory.EnumAdapters1(i) } {
                Ok(adapter) => {
                    adapter_count += 1;
                    match unsafe { adapter.GetDesc1() } {
                        Ok(desc) => {
                            let desc_string = String::from_utf16_lossy(&desc.Description);
                            let adapter_name = desc_string.trim_end_matches('\0');
                            let dedicated_video_memory_mb =
                                desc.DedicatedVideoMemory / (1024 * 1024);

                            info!(
                                "GPU Adapter {}: {} (VRAM: {} MB)",
                                i, adapter_name, dedicated_video_memory_mb
                            );

                            // Check if this is likely a compatible adapter
                            // DirectML typically works well with dedicated GPUs with sufficient VRAM
                            if dedicated_video_memory_mb >= 1024 {
                                has_compatible_adapter = true;
                            }
                        }
                        Err(e) => {
                            info!("GPU Adapter {}: Description unavailable - {:?}", i, e);
                        }
                    }
                }
                Err(_) => break, // No more adapters
            }
        }

        if adapter_count == 0 {
            error!("No GPU adapters found. DirectML will fall back to CPU execution.");
        } else if !has_compatible_adapter {
            warn!(
                "Found {} GPU adapter(s) but none seem to have sufficient VRAM (>=1GB).",
                adapter_count
            );
            warn!("DirectML may still work but performance could be limited.");
        } else {
            info!(
                "Found {} compatible GPU adapter(s) - DirectX 12 support available",
                adapter_count
            );
        }
    }
    /// Preload required DLLs for faster service startup
    fn preload_service_dlls() {
        use windows::Win32::System::LibraryLoader::LoadLibraryA;
        use windows::core::PCSTR;

        info!("Preloading service DLLs for optimized startup");

        // List of DLLs to preload
        let dlls_to_preload = ["DirectML.dll", "onnxruntime.dll"];
        let mut directml_available = false;

        for dll_name in &dlls_to_preload {
            let dll_cstr = format!("{dll_name}\0");
            match unsafe { LoadLibraryA(PCSTR(dll_cstr.as_ptr())) } {
                Ok(handle) => {
                    if !handle.is_invalid() {
                        info!("Successfully preloaded: {}", dll_name);
                        if dll_name == &"DirectML.dll" {
                            directml_available = true;
                        }
                    } else if dll_name == &"DirectML.dll" {
                        warn!(
                            "Failed to preload DirectML.dll - GPU acceleration will not be available"
                        );
                    } else {
                        warn!(
                            "Failed to preload: {} (library not found or invalid)",
                            dll_name
                        );
                    }
                }
                Err(e) => {
                    if dll_name == &"DirectML.dll" {
                        warn!(
                            "Failed to preload DirectML.dll: {:?} - GPU acceleration will not be available",
                            e
                        );
                    } else {
                        warn!("Failed to preload {}: {:?}", dll_name, e);
                    }
                }
            }
        }

        if !directml_available {
            info!("DirectML is not available - CPU inference will be used");
        }

        // Set DLL search optimization
        unsafe {
            std::env::set_var(
                "PATH",
                format!(
                    "{};{}",
                    std::env::current_exe()
                        .unwrap_or_default()
                        .parent()
                        .unwrap_or(std::path::Path::new("."))
                        .display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
        };
    }
}
