//! Blue Onyx service.
//!
//! Same CLI arguments as the standalone binary.
//!
//! Avoid having spaces in you binpath, keep the space after the `=`:
//! <https://stackoverflow.com/questions/3663331/when-creating-a-service-with-sc-exe-how-to-pass-in-context-parameters>
//!
//! Install the service:
//! `sc.exe create blue_onyx_service binPath= "<path>\blue_onyx_service.exe --port 12345" start= auto displayname= "Blue Onyx Service"`
//!
//! Start the service: `net start blue_onyx_service`
//!
//! Stop the service: `net stop blue_onyx_service`
//!
//! Uninstall the service: `sc.exe delete blue_onyx_service`
//!
//! You can have several services pointing to the same binary with different names and arguments.

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
    use blue_onyx::{blue_onyx_service, cli::Cli, init_logging};
    use clap::Parser;
    use std::{env, ffi::OsString, future::Future, time::Duration};
    use tokio_util::sync::CancellationToken;
    use tracing::{error, info};

    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher, Result,
    };

    const SERVICE_NAME: &str = "blue_onyx_service";
    const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

    pub fn run() -> Result<()> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)
    }
    define_windows_service!(ffi_service_main, my_service_main);

    pub fn my_service_main(service_name: Vec<OsString>) {
        let arguments: Vec<OsString> = env::args_os().collect();
        let args = Cli::try_parse_from(arguments.clone()).unwrap();

        let default_log_path = std::path::PathBuf::from(format!(
            "{}\\{}",
            std::env::var("PROGRAMDATA").unwrap_or_else(|_| "C:\\ProgramData".to_string()),
            service_name[0].to_string_lossy()
        ));

        let log_path = args
            .log_path
            .clone()
            .unwrap_or_else(|| default_log_path.clone());

        println!("Logs will be written to log path: {}", log_path.display());
        let _guard = init_logging(args.log_level, Some("c:\\git\\".into()));
        info!("Starting blue onyx service with args: {:#?}", arguments);

        let (blue_onyx_service, cancellation_token, thread_handle) = match blue_onyx_service(args) {
            Ok(v) => v,
            Err(err) => {
                error!(?err, "Failed to init blue onyx service");
                return;
            }
        };

        if let Err(err) = run_service(blue_onyx_service, cancellation_token) {
            error!(?err, "Blue onyx service failed");
        }

        thread_handle
            .join()
            .expect("Failed to join detector worker thread");
    }

    pub fn run_service(
        blue_onyx_service: impl Future<Output = anyhow::Result<()>>,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<()> {
        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                ServiceControl::Stop => {
                    cancellation_token.cancel();
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::UserEvent(code) => {
                    if code.to_raw() == 130 {
                        cancellation_token.cancel();
                    }
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };
        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        rt.block_on(async {
            blue_onyx_service
                .await
                .expect("Failed to run blue onyx service");
        });

        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        Ok(())
    }
}
