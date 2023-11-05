# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
### Changed
### Fixed
### Removed

## [0.5.0] - 2023-11-03

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

[Unreleased]: https://github.com/chipsenkbeil/distant/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/chipsenkbeil/service-manager-rs/releases/tag/v0.1.0
