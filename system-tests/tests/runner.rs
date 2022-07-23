use assert_cmd::{crate_name, Command};
use service_manager::*;
use std::{net::SocketAddr, thread, time::Duration};

/// Time to wait from starting a service to communicating with it
const WAIT_PERIOD: Duration = Duration::from_secs(1);

fn wait() {
    eprintln!("Waiting {}s before continuing", WAIT_PERIOD.as_secs_f32());
    thread::sleep(WAIT_PERIOD);
}

fn cleanup(service_manager: &dyn ServiceManager, service_label: &ServiceLabel) {
    eprintln!("Trying to stop service if it was running already");
    let _ = service_manager.stop(ServiceStopCtx {
        label: service_label.clone(),
    });

    wait();

    eprintln!("Trying to uninstall service if it was installed already");
    let _ = service_manager.uninstall(ServiceUninstallCtx {
        label: service_label.clone(),
    });

    wait();
}

/// Run test with given service manager
pub fn run_test(service_manager: impl Into<Box<dyn ServiceManager>>) {
    let service_manager = service_manager.into();
    let service_label: ServiceLabel = "com.example.echo".parse().unwrap();
    let addr: SocketAddr = "127.0.0.1:8088".parse().unwrap();

    // Ensure service manager is available
    eprintln!("Checking if service available");
    assert!(
        service_manager.available().unwrap(),
        "Service not available"
    );

    // Attempt to stop & uninstall the service in case it already exists from a failed test
    cleanup(service_manager.as_ref(), &service_label);

    // Install the service
    eprintln!("Installing service");
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

    // Wait for service to be installed
    wait();

    // Start the service
    eprintln!("Starting service");
    service_manager
        .start(ServiceStartCtx {
            label: service_label.clone(),
        })
        .unwrap();

    // Wait for the service to start
    wait();

    // Communicate with the service
    eprintln!("Talking to service");
    Command::cargo_bin(crate_name!())
        .unwrap()
        .arg("talk")
        .arg(addr.to_string())
        .arg("hello world")
        .assert()
        .stdout("hello world\n");

    // Stop the service
    eprintln!("Stopping service");
    service_manager
        .stop(ServiceStopCtx {
            label: service_label.clone(),
        })
        .unwrap();

    // Wait for the service to stop
    wait();

    // Uninstall the service
    eprintln!("Uninstalling service");
    service_manager
        .uninstall(ServiceUninstallCtx {
            label: service_label,
        })
        .unwrap();
}
