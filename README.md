# Service Manager

[![CI](https://github.com/chipsenkbeil/service-manager-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/chipsenkbeil/service-manager-rs/actions/workflows/ci.yml)

Rust library that provides an interface towards working with the
following service management platforms:

* [`sc.exe`](https://docs.microsoft.com/en-us/previous-versions/windows/it-pro/windows-server-2012-r2-and-2012/cc754599(v=ws.11)) for use with [Window Service](https://en.wikipedia.org/wiki/Windows_service) (Windows)
* [Launchd](https://en.wikipedia.org/wiki/Launchd) (MacOS)
* [systemd](https://en.wikipedia.org/wiki/Systemd) (Linux)
* [OpenRC](https://en.wikipedia.org/wiki/OpenRC) (Linux)
* [rc.d](https://en.wikipedia.org/wiki/Init#Research_Unix-style/BSD-style) (FreeBSD)

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
service-manager = "0.1"
```

## Examples

### Generic service management

This crate provides a mechanism to detect and use the default service
management platform of the current operating system. Each `ServiceManager`
instance provides four key methods:

* `install` - will install the service specified by a given context
* `uninstall` - will uninstall the service specified by a given context
* `start` - will start an installed service specified by a given context
* `stop` - will stop a running service specified by a given context

```rust
use service_manager::*;
use std::{ffi::OsString, path::PathBuf};

// Create a label for our service
let label: ServiceLabel = "com.example.my-service".parse().unwrap();

// Get generic service by detecting what is available on the platform
let manager = <dyn ServiceManager>::native()
    .expect("Failed to detect management platform");

// Install our service using the underlying service management platform
manager.install(ServiceInstallCtx {
    label: label.clone(),
    program: PathBuf::from("path/to/my-service-executable"),
    args: vec![OsString::from("--some-arg")],
}).expect("Failed to install");

// Start our service using the underlying service management platform
manager.start(ServiceStartCtx {
    label: label.clone()
}).expect("Failed to start");

// Stop our service using the underlying service management platform
manager.stop(ServiceStopCtx {
    label: label.clone()
}).expect("Failed to stop");

// Uninstall our service using the underlying service management platform
manager.uninstall(ServiceUninstallCtx {
    label: label.clone()
}).expect("Failed to stop");
```

### User-level service management

By default, service management platforms will interact with system-level
services; however, some service management platforms like `systemd` and
`launchd` support user-level services. To interact with services at the
user level, you configure your manager using the generic
`ServiceManager::set_level` function.

```rust
use service_manager::*;

// Create a label for our service
let label: ServiceLabel = "com.example.my-service".parse().unwrap();

// Get generic service by detecting what is available on the platform
let mut manager = <dyn ServiceManager>::native()
    .expect("Failed to detect management platform");

// Update our manager to work with user-level services
manager.set_level(ServiceLevel::User)
    .expect("Service manager does not support user-level services");

// Continue operating as usual via install/uninstall/start/stop
// ...
```

### Specific service manager configurations

There are times where you need more control over the configuration of a
service tied to a specific platform. To that end, you can create the service
manager explicitly and set configuration properties appropriately.

```rust
use service_manager::*;

// Create a label for our service
let label: ServiceLabel = "com.example.my-service".parse().unwrap();

// Instantiate a specific service manager
let mut manager = LaunchdServiceManager::system();

// Update an install configuration property where installing a service
// will NOT add the KeepAlive flag
manager.config.install.keep_alive = false;

// Install our service using the explicit service manager
manager.install(ServiceInstallCtx {
    label: label.clone(),
    program: PathBuf::from("path/to/my-service-executable"),
    args: vec![OsString::from("--some-arg")],
}).expect("Failed to install");
```

## License

This project is licensed under either of

Apache License, Version 2.0, (LICENSE-APACHE or
[apache-license][apache-license]) MIT license (LICENSE-MIT or
[mit-license][mit-license]) at your option.

[apache-license]: http://www.apache.org/licenses/LICENSE-2.0
[mit-license]: http://opensource.org/licenses/MIT
