# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **BREAKING CHANGE**: Extended `RestartPolicy::OnFailure` with two new fields to
  prevent infinite restart loops:
  - `max_retries: Option<u32>` — Maximum number of restart attempts before the
    service stops. When `None`, the service restarts indefinitely (previous
    behavior).
  - `reset_after_secs: Option<u32>` — Duration in seconds after which the failure
    counter resets. If the service runs successfully for this long, previous
    failures are forgotten and the retry counter starts fresh. When `None`, the
    platform default is used.
  - Migration: Add `max_retries: None, reset_after_secs: None` to existing
    `RestartPolicy::OnFailure { delay_secs }` constructions to preserve current
    behavior.

### Added

- **WinSW**: When `max_retries` is set, the WinSW backend generates multiple
  `<onfailure action="restart"/>` elements (one per retry) followed by
  `<onfailure action="none"/>` to stop the service after exhausting retries. When
  `reset_after_secs` is set, a `<resetfailure>` element is generated (unless a
  WinSW-specific `reset_failure_time` is already configured).
- Other service managers (systemd, launchd, sc, OpenRC, rc.d) do not yet
  implement `max_retries` or `reset_after_secs`, but there is potential to extend
  them in the future (e.g., systemd's `StartLimitBurst` /
  `StartLimitIntervalSec`).
- Added a `fail` subcommand to the system test binary for simulating crashing
  services.
- Added a Windows system test (`should_stop_winsw_service_after_max_retries`)
  that verifies a failing WinSW service stops after exhausting its retry limit.

## [0.10.0] - 2025-12-14

### Changed

- **BREAKING CHANGE**: Launchd services with restart policies (`RestartPolicy::Always`, `OnFailure`, or `OnSuccess`) no longer auto-start when `install()` is called. Services must now be explicitly started using `start()`. This provides cross-platform consistency where `install()` registers the service definition without starting it, matching the behavior of systemd and other service managers.
  - Services with `KeepAlive` configured are now installed with `Disabled=true` in the plist
  - The `start()` function removes the `Disabled` key and reloads the service
  - The `autostart` parameter continues to control only `RunAtLoad` (whether service starts on OS boot), not initial install behavior
  - Migration: Add explicit `manager.start(ctx)?` call after `manager.install(ctx)?` if you need the service to start immediately

### Fixed

- Fixed incorrect Launchd restart policy implementation for `RestartPolicy::OnFailure` and `RestartPolicy::OnSuccess`:
  - `OnFailure` now correctly uses `KeepAlive` dictionary with `SuccessfulExit=false` (restart on non-zero exit) instead of `KeepAlive=true` (always restart)
  - `OnSuccess` now correctly uses `SuccessfulExit=true` (restart on zero exit) instead of `SuccessfulExit=false`

## [0.9.0] - 2025-11-22

### Changed

- **BREAKING CHANGE**: Replaced `disable_restart_on_failure: bool` field with `restart_policy: RestartPolicy` in `ServiceInstallCtx`.
  - The new `RestartPolicy` enum provides a cross-platform abstraction for service-restart behavior with four variants:
    - `RestartPolicy::Never` - Service never restarts
    - `RestartPolicy::Always { delay_secs: Option<u32> }` - Service always restarts regardless of exit status
    - `RestartPolicy::OnFailure { delay_secs: Option<u32> }` - Service restarts only on non-zero exit (default)
    - `RestartPolicy::OnSuccess { delay_secs: Option<u32> }` - Service restarts only on successful exit (exit code 0)
  - Different platforms support different levels of granularity:
    - **systemd** (Linux): Supports all restart policies natively (including `OnSuccess` via `Restart=on-success`)
    - **launchd** (macOS): Supports Never, Always, and OnSuccess; OnFailure is approximated using `KeepAlive=true`; OnSuccess uses `KeepAlive` dictionary with `SuccessfulExit=false`
    - **WinSW** (Windows): Supports Never, Always, and OnFailure with optional delays; OnSuccess falls back to Always with a warning
    - **OpenRC/rc.d/sc.exe**: Limited or no restart support; logs warnings for unsupported policies
  - Migration guide for `ServiceInstallCtx`:
    - `disable_restart_on_failure: false` → `restart_policy: RestartPolicy::OnFailure { delay_secs: None }`
    - `disable_restart_on_failure: true` → `restart_policy: RestartPolicy::Never`

- **BREAKING CHANGE**: Platform-specific restart configuration fields are now `Option` types,
  allowing the generic `RestartPolicy` to be used by default while still supporting platform-specific
  features when needed:
  - `SystemdInstallConfig.restart`: Changed from `SystemdServiceRestartType` to `Option<SystemdServiceRestartType>`
    - When `Some`, the systemd-specific restart type takes precedence over the generic `RestartPolicy`
    - When `None` (default), falls back to the generic `RestartPolicy`
    - Migration: `restart: SystemdServiceRestartType::OnFailure` → 
      `restart: Some(SystemdServiceRestartType::OnFailure)` or `restart: None` to use generic policy
  - `LaunchdInstallConfig.keep_alive`: Changed from `bool` to `Option<bool>`
    - When `Some`, the launchd-specific keep-alive setting takes precedence
    - When `None` (default), falls back to the generic `RestartPolicy`
    - Migration: `keep_alive: true` → `keep_alive: Some(true)` or `keep_alive: None` to use generic policy
  - `WinSwInstallConfig.failure_action`: Changed from `WinSwOnFailureAction` to `Option<WinSwOnFailureAction>`
    - When `Some`, the WinSW-specific failure action takes precedence
    - When `None` (default), falls back to the generic `RestartPolicy`
    - Migration: `failure_action: WinSwOnFailureAction::Restart(...)` →
      `failure_action: Some(WinSwOnFailureAction::Restart(...))` or `failure_action: None` to use generic policy

### Added

- Support for the `log` crate to emit warnings when platform-specific restart features are not supported

## [0.8.0] - 2025-02-21

### Added

- Provide the ability to disable service restarts on failure. This is useful because certain
  applications don't want this behaviour. This is a breaking change because users who upgrade will
  need to update their code for the new field in the `ServiceInstallCtx` struct.

## [0.7.1] - 2024-07-13

### Fixed

- When writing service definition files, the previous file is truncated. Without this, in certain
  cases it would cause these files to be incorrectly re-written.

## [0.7.0] - 2024-05-31

### Added

- The WinSW service manager can read the location of the WinSW binary from the `WINSW_PATH`
  environment variable. This is useful to avoid the necessity of having it in a location that is on
  the `Path` variable, which can be a bit more awkward on Windows. There are a lack of standard
  locations that can be written to without administrative privileges.
- Introduce the `autostart` field on `ServiceInstallCtx`. This controls whether a service should
  automatically start upon rebooting the OS. It's an option common to all service managers and it's
  useful for developers to think about whether their services should automatically start up. If the
  service is resource intensive or uses a lot of bandwidth, some users actually don't want automatic
  start because it can potentially render their machine unusable.

## [0.6.2] - 2024-05-27

- The WinSW service manager will delete service directories upon uninstall

## [0.6.1] - 2024-05-03

- Fix issue where calling stop on MacOS service does not halt the service due to the service's default auto-restart setting. (#19)
- Remove user specification for user-mode service definitions in Systemd. In a user-mode service, it will run as the current user, and the service won't actually start correctly if the user is specified. The user specification is really for system-wide services that don't run as root.

## [0.6.0] - 2024-02-18

- Derive basic traits on the Service context structs. (#18)
- Introduced support for specifying environment variables for systemd. The specified variables are now written on separate lines. (#17)

## [0.5.1] - 2023-11-22

- Fix a small issue in the WinSW service manager which caused the service
  management directories to be created at the current directory, rather than
  the intended location at C:\ProgramData\service-manager.

## [0.5.0] - 2023-11-06

- Support for the WinSW service manager was added. WinSW can run any
  application as a Windows service by providing the boilerplate code for
  interacting with the Windows service infrastructure. Most, but not all,
  configuration options are supported in this initial setup.
- The `ServiceInstallCtx` is extended with optional `working_directory` and
  `environment` fields, which assign a working directory and pass a list of
  environment variables to the process launched by the service. Most service
  managers support these. This is a backwards incompatible change.

## [0.4.0] - 2023-10-19

### Added

- The `ServiceInstallCtx` is extended with a `username` field to support
  running services as a specific user. For now, only systemd and launchd are
  supported, but it has the potential to be used with Windows. This is a
  backwards incompatible change.

## [0.3.0] - 2023-06-08

### Added

- Add .contents to ServiceInstallCtx to use in place of the make_* templates
  This is a backwards incompatible change.

## [0.1.3] - 2022-07-24

### Fixed

- Exported config structs for individual service managers

## [0.1.2] - 2022-07-24

### Added

- systemd now includes `WantedBy=default.target` when user-level service

## [0.1.1] - 2022-07-24

### Added

- `SystemdInstallConfig` and `SystemdServiceRestartType` with install
  config defaulting to `on-failure` to ensure that systemd properly restarts
  the process

## [0.1.0] - 2022-07-24

### Added

- Initial commit of project that includes five different service management
  platforms:
    - [`sc.exe`](https://docs.microsoft.com/en-us/previous-versions/windows/it-pro/windows-server-2012-r2-and-2012/cc754599(v=ws.11)) for use with [Window Service](https://en.wikipedia.org/wiki/Windows_service) (Windows)
    - [Launchd](https://en.wikipedia.org/wiki/Launchd) (MacOS)
    - [systemd](https://en.wikipedia.org/wiki/Systemd) (Linux)
    - [OpenRC](https://en.wikipedia.org/wiki/OpenRC) (Linux)
    - [rc.d](https://en.wikipedia.org/wiki/Init#Research_Unix-style/BSD-style) (FreeBSD)
- Created `TypedServiceManager` enum to associate the manager's type when
  retrieved using native lookup

[Unreleased]: https://github.com/chipsenkbeil/distant/compare/v0.7.1...HEAD
[0.7.1]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.7.1
[0.7.0]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.7.0
[0.6.2]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.6.2
[0.6.1]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.6.1
[0.6.0]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.6.0
[0.5.1]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.5.1
[0.5.0]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.5.0
[0.4.0]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.4.0
[0.3.0]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.3.0
[0.1.3]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.1.3
[0.1.2]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.1.2
[0.1.1]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.1.1
[0.1.0]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.1.0
