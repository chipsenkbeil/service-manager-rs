use service_manager::*;

mod runner;

#[test]
#[cfg(target_os = "macos")]
fn should_support_launchd_for_system_services() {
    runner::run_test(LaunchdServiceManager::system())
}

#[test]
#[cfg(target_os = "macos")]
fn should_support_launchd_for_user_services() {
    runner::run_test(LaunchdServiceManager::user())
}

#[test]
#[cfg(target_os = "linux")]
fn should_support_openrc_for_system_services() {
    runner::run_test(OpenRcServiceManager::system())
}

#[test]
#[cfg(any(
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd"
))]
fn should_support_rc_d_for_system_services() {
    runner::run_test(RcdServiceManager::system())
}

#[test]
#[cfg(target_os = "windows")]
fn should_support_sc_for_system_services() {
    runner::run_test(ScServiceManager::system())
}

#[test]
#[cfg(target_os = "linux")]
fn should_support_systemd_for_system_services() {
    runner::run_test(SystemdServiceManager::system())
}

#[test]
#[cfg(target_os = "linux")]
fn should_support_systemd_for_user_services() {
    runner::run_test(SystemdServiceManager::user())
}
