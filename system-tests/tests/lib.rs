use service_manager::*;

mod runner;

const TEST_ITER_CNT: usize = 3;

#[test]
#[cfg(target_os = "macos")]
fn should_support_launchd_for_system_services() {
    runner::run_test_n(LaunchdServiceManager::system(), TEST_ITER_CNT)
}

#[test]
#[cfg(target_os = "macos")]
fn should_support_launchd_for_user_services() {
    runner::run_test_n(LaunchdServiceManager::user(), TEST_ITER_CNT)
}

#[test]
#[cfg(target_os = "linux")]
fn should_support_openrc_for_system_services() {
    runner::run_test_n(OpenRcServiceManager::system(), TEST_ITER_CNT)
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
fn should_support_systemd_for_user_services() {
    runner::run_test_n(SystemdServiceManager::user(), TEST_ITER_CNT)
}
