use assert_cmd::{crate_name, Command};
use service_manager::*;
use std::{net::SocketAddr, thread, time::Duration};

/// Time to wait from starting a service to communicating with it
static WAIT_PERIOD_SECS: f32 = 1.5;

fn cleanup(service_manager: &dyn ServiceManager, service_label: &ServiceLabel) {
    let _ = service_manager.stop(ServiceStopCtx {
        label: service_label.clone(),
    });

    thread::sleep(Duration::from_secs_f32(WAIT_PERIOD_SECS));

    let _ = service_manager.uninstall(ServiceUninstallCtx {
        label: service_label.clone(),
    });

    thread::sleep(Duration::from_secs_f32(WAIT_PERIOD_SECS));
}

/// Run test with given service manager
pub fn run_test(service_manager: impl Into<Box<dyn ServiceManager>>) {
    let service_manager = service_manager.into();
    let service_label: ServiceLabel = "com.example.echo".parse().unwrap();
    let addr: SocketAddr = "127.0.0.1:8088".parse().unwrap();

    // Ensure service manager is available
    assert!(
        service_manager.available().unwrap(),
        "Service not available"
    );

    // Attempt to stop & uninstall the service in case it already exists from a failed test
    cleanup(service_manager.as_ref(), &service_label);

    // Install the service
    service_manager
        .install(ServiceInstallCtx {
            label: service_label.clone(),
            program: assert_cmd::cargo::cargo_bin(crate_name!())
                .to_string_lossy()
                .to_string(),
            args: vec![
                "listen".to_string(),
                addr.to_string(),
                "--log-file".to_string(),
                std::env::temp_dir()
                    .join(format!("{service_label}.log"))
                    .to_string_lossy()
                    .to_string(),
            ],
        })
        .unwrap();

    // Start the service
    service_manager
        .start(ServiceStartCtx {
            label: service_label.clone(),
        })
        .unwrap();

    // Wait for the service to start
    thread::sleep(Duration::from_secs_f32(WAIT_PERIOD_SECS));

    // Communicate with the service
    Command::cargo_bin(crate_name!())
        .unwrap()
        .arg("talk")
        .arg(addr.to_string())
        .arg("hello world")
        .assert()
        .stdout("hello world\n");

    // Stop the service
    service_manager
        .stop(ServiceStopCtx {
            label: service_label.clone(),
        })
        .unwrap();

    // Uninstall the service
    service_manager
        .uninstall(ServiceUninstallCtx {
            label: service_label,
        })
        .unwrap();
}
