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
fn should_support_systemd_for_user_services() {
    runner::run_test_n(SystemdServiceManager::user(), TEST_ITER_CNT)
}
