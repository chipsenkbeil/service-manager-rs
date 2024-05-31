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
const SERVICE_LABEL: &str = "com.example.echo";

pub fn is_running_in_ci() -> bool {
    std::env::var("CI").as_deref() == Ok("true")
}

fn wait() {
    eprintln!("Waiting {}s before continuing", WAIT_PERIOD.as_secs_f32());
    thread::sleep(WAIT_PERIOD);
}

#[allow(dead_code)]
pub fn run_test_n(manager: impl Into<TypedServiceManager>, n: usize) {
    let manager = manager.into();
    for i in 0..n {
        eprintln!("[[Test iteration {i}]]");
        run_test(&manager, None);
    }
}

#[allow(dead_code)]
pub fn run_test_n_as_user(
    manager: impl Into<TypedServiceManager>,
    n: usize,
    username: &str,
) -> bool {
    let manager = manager.into();
    let mut is_user_specified = false;
    for i in 0..n {
        eprintln!("[[Test iteration {i}]]");
        is_user_specified = run_test(&manager, Some(username.to_string())).unwrap();
    }
    is_user_specified
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
pub fn run_test(manager: &TypedServiceManager, username: Option<String>) -> Option<bool> {
    let service_label: ServiceLabel = if username.is_some() {
        format!("{}-user", SERVICE_LABEL).parse().unwrap()
    } else {
        SERVICE_LABEL.parse().unwrap()
    };
    let port = find_ephemeral_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    eprintln!("Identified echo server address: {addr}");

    // Copy the service binary to a location where it can be accessed by a different user account
    // if need be.
    let temp_dir = std::env::temp_dir();
    let bin_path = assert_cmd::cargo::cargo_bin(crate_name!());
    let temp_bin_path = temp_dir.join(bin_path.file_name().unwrap());
    if temp_bin_path.exists() {
        std::fs::remove_file(temp_bin_path.clone()).unwrap();
    }
    std::fs::copy(&bin_path, &temp_bin_path).unwrap();

    // Ensure service manager is available
    eprintln!("Checking if service available");
    assert!(manager.available().unwrap(), "Service not available");

    let mut args = vec![
        OsString::from("listen"),
        OsString::from(addr.to_string()),
        OsString::from("--log-file"),
        std::env::temp_dir()
            .join(format!("{service_label}.log"))
            .into_os_string(),
    ];
    if manager.is_sc() {
        args.push(OsString::from("--run-as-windows-service"));
    }

    // Install the service
    eprintln!("Installing service");
    manager
        .install(ServiceInstallCtx {
            label: service_label.clone(),
            program: temp_bin_path,
            args,
            contents: None,
            username: username.clone(),
            working_directory: None,
            environment: None,
            autostart: true,
        })
        .unwrap();

    // Wait for service to be installed
    wait();

    let is_user_specified =
        username.map(|user| is_service_using_the_specified_user(&user, service_label.clone()));

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
    if manager.is_openrc() && is_running_in_ci() {
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

    is_user_specified
}

#[cfg(target_os = "linux")]
fn is_service_using_the_specified_user(username: &str, service_label: ServiceLabel) -> bool {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    // Check for the file at either the global or the user location, and if neither exist, bail out.
    // It has to be the case that one of them exist: something has went wrong if they don't.
    let path = [
        systemd_global_dir_path().join(format!("{}.service", service_label.to_script_name())),
        systemd_user_dir_path()
            .unwrap()
            .join(format!("{}.service", service_label.to_script_name())),
    ]
    .iter()
    .find(|p| p.exists())
    .cloned()
    .unwrap_or_else(|| panic!("Service file not located at either system-wide or user-wide paths"));

    let string_to_find = format!("User={username}");
    let file = File::open(&path).unwrap();
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.unwrap();
        if line.contains(&string_to_find) {
            return true;
        }
    }
    false
}

#[cfg(target_os = "macos")]
fn is_service_using_the_specified_user(username: &str, service_label: ServiceLabel) -> bool {
    use plist::Value;
    use std::fs::File;
    use std::path::PathBuf;

    let plist_path = PathBuf::from(format!(
        "/Library/LaunchDaemons/{}.plist",
        service_label.to_qualified_name()
    ));
    let file = File::open(plist_path).unwrap();
    let plist_data: Value = plist::from_reader(file).unwrap();

    if let Some(dict) = plist_data.into_dictionary() {
        if let Some(user_value) = dict.get("UserName") {
            if let Some(user_str) = user_value.as_string() {
                return user_str == username;
            }
        }
    }
    false
}

// For all other platforms (i.e. Windows, FreeBSD) we do not support specific users right now
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn is_service_using_the_specified_user(_username: &str, _service_label: ServiceLabel) -> bool {
    false
}
