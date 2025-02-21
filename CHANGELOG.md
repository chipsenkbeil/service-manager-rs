# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
