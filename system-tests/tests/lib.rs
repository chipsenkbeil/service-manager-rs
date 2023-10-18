use service_manager::*;

mod runner;

const TEST_ITER_CNT: usize = 3;

#[test]
// #[cfg(target_os = "macos")]
fn should_support_launchd_for_system_services() {
    runner::run_test_n(LaunchdServiceManager::system(), TEST_ITER_CNT)
}

#[test]
#[cfg(target_os = "macos")]
fn should_support_launchd_for_user_services() {
    runner::run_test_n(LaunchdServiceManager::user(), TEST_ITER_CNT)
}

#[test]
#[cfg(target_os = "macos")]
fn should_support_launchd_for_system_services_running_as_specific_user() {
    create_user_account("test_account");

    let is_user_specified = runner::run_test_n_as_user(
        LaunchdServiceManager::system(),
        TEST_ITER_CNT,
        "test_account",
    );

    remove_user_account("test_account");

    assert!(is_user_specified);
}

#[test]
#[cfg(target_os = "linux")]
fn should_support_openrc_for_system_services() {
    // TODO: There's some problem running OpenRC within the CI's docker container where stopping
    //       the service fails, so subsequent test runs do not succeed. For now, if we detect
    //       that we are running in the CI, we want to only test once. We'll have to manually
    //       test with our local Alpine VM instead.
    let cnt = if runner::is_running_in_ci() {
        1
    } else {
        TEST_ITER_CNT
    };

    runner::run_test_n(OpenRcServiceManager::system(), cnt)
}

#[test]
#[cfg(any(
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd"
))]
fn should_support_rc_d_for_system_services() {
    runner::run_test_n(RcdServiceManager::system(), TEST_ITER_CNT)
}

#[test]
#[cfg(target_os = "windows")]
fn should_support_sc_for_system_services() {
    runner::run_test_n(ScServiceManager::system(), TEST_ITER_CNT)
}

#[test]
#[cfg(target_os = "linux")]
fn should_support_systemd_for_system_services() {
    runner::run_test_n(SystemdServiceManager::system(), TEST_ITER_CNT)
}

#[test]
#[cfg(target_os = "linux")]
fn should_support_systemd_for_system_services_running_as_specific_user() {
    create_user_account("test_account");

    let is_user_specified = runner::run_test_n_as_user(
        SystemdServiceManager::system(),
        TEST_ITER_CNT,
        "test_account",
    );

    remove_user_account("test_account");

    assert!(is_user_specified);
}

#[test]
#[cfg(target_os = "linux")]
fn should_support_systemd_for_user_services() {
    runner::run_test_n(SystemdServiceManager::user(), TEST_ITER_CNT)
}

#[cfg(target_os = "linux")]
fn create_user_account(username: &str) {
    use std::process::Command;

    let status = Command::new("useradd")
        .arg("-m")
        .arg("-s")
        .arg("/bin/bash")
        .arg(username)
        .status()
        .unwrap();
    assert!(status.success(), "Failed to create user test_account");
}

#[cfg(target_os = "macos")]
fn create_user_account(username: &str) {
    use std::process::Command;
    use std::str;

    let output = Command::new("dscl")
        .arg(".")
        .arg("-list")
        .arg("/Users")
        .arg("UniqueID")
        .output()
        .unwrap();
    let output_str = str::from_utf8(&output.stdout).unwrap();
    let mut max_id = 0;

    for line in output_str.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() == 2 {
            if let Ok(id) = parts[1].parse::<u32>() {
                if id > max_id {
                    max_id = id;
                }
            }
        }
    }
    let new_unique_id = max_id + 1;

    let commands = vec![
        format!("dscl . -create /Users/{}", username),
        format!(
            "dscl . -create /Users/{} UserShell /usr/bin/false",
            username
        ),
        format!(
            "dscl . -create /Users/{} UniqueID {}",
            username, new_unique_id
        ),
        format!("dscl . -create /Users/{} PrimaryGroupID 20", username),
    ];
    for cmd in commands {
        let status = Command::new("sh").arg("-c").arg(&cmd).status().unwrap();
        assert!(status.success(), "Failed to create user test_account");
    }
}

#[cfg(target_os = "linux")]
fn remove_user_account(username: &str) {
    use std::process::Command;

    let status = Command::new("userdel")
        .arg("-r")
        .arg("-f")
        .arg(username)
        .status()
        .unwrap();
    assert!(status.success(), "Failed to delete user test_account");
}

#[cfg(target_os = "macos")]
fn remove_user_account(username: &str) {
    use std::process::Command;

    let status = Command::new("dscl")
        .arg(".")
        .arg("-delete")
        .arg(format!("/Users/{username}"))
        .status()
        .unwrap();
    assert!(status.success(), "Failed to delete user test_account");
}
