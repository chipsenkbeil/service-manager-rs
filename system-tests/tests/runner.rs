use assert_cmd::{crate_name, Command};
use service_manager::*;
use std::{
    ffi::OsString,
    net::{SocketAddr, TcpListener},
    thread,
    time::Duration,
};

/// Time to wait from starting a service to communicating with it
const WAIT_PERIOD: Duration = Duration::from_secs(1);

fn wait() {
    eprintln!("Waiting {}s before continuing", WAIT_PERIOD.as_secs_f32());
    thread::sleep(WAIT_PERIOD);
}

pub fn run_test_n(manager: impl Into<TypedServiceManager>, n: usize) {
    let manager = manager.into();
    for i in 0..n {
        eprintln!("[[Test iteration {i}]]");
        run_test(&manager)
    }
}

fn find_ephemeral_port() -> u16 {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    TcpListener::bind(addr)
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Run test with given service manager
pub fn run_test(manager: &TypedServiceManager) {
    let service_label: ServiceLabel = "com.example.echo".parse().unwrap();
    let port = find_ephemeral_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    eprintln!("Identified echo server address: {addr}");

    // Ensure service manager is available
    eprintln!("Checking if service available");
    assert!(manager.available().unwrap(), "Service not available");

    // Install the service
    eprintln!("Installing service");
    manager
        .install(ServiceInstallCtx {
            label: service_label.clone(),
            program: assert_cmd::cargo::cargo_bin(crate_name!()),
            args: vec![
                OsString::from("listen"),
                OsString::from(addr.to_string()),
                OsString::from("--log-file"),
                std::env::temp_dir()
                    .join(format!("{service_label}.log"))
                    .into_os_string(),
            ],
        })
        .unwrap();

    // Wait for service to be installed
    wait();

    // Start the service
    eprintln!("Starting service");
    manager
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
    wait();

    // Stop the service
    eprintln!("Stopping service");
    if manager.is_openrc() && std::env::var("CI").as_deref() == Ok("true") {
        let res = manager.stop(ServiceStopCtx {
            label: service_label.clone(),
        });
        if res.is_err() {
            eprintln!(
                "OpenRC stop is bugged in CI test, so skipping: {}",
                res.unwrap_err()
            );
        }
    } else {
        manager
            .stop(ServiceStopCtx {
                label: service_label.clone(),
            })
            .unwrap();
    }

    // Wait for the service to stop
    wait();

    // Uninstall the service
    eprintln!("Uninstalling service");
    manager
        .uninstall(ServiceUninstallCtx {
            label: service_label,
        })
        .unwrap();
    wait();
}
