use std::{fmt, io, str::FromStr};

mod kind;

#[cfg(target_os = "macos")]
mod launchd;

#[cfg(unix)]
mod openrc;

#[cfg(unix)]
mod rc;

pub use kind::ServiceManagerKind;

#[cfg(target_os = "macos")]
pub use launchd::LaunchdServiceManager;

#[cfg(unix)]
pub use openrc::OpenRcServiceManager;

#[cfg(unix)]
pub use rc::RcServiceManager;

/// Interface for a service manager
pub trait ServiceManager {
    /// Determines if the service manager exists (e.g. is `launchd` available on the system?) and
    /// can be used
    fn available(&self) -> io::Result<bool>;

    /// Returns whether or not the service manager supports user-level services
    fn supports_user_level_service(&self) -> bool {
        false
    }

    /// Installs a new service using the manager
    fn install(&self, ctx: ServiceInstallCtx) -> io::Result<()>;

    /// Uninstalls an existing service using the manager
    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()>;

    /// Starts a service using the manager
    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()>;

    /// Stops a running service using the manager
    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()>;
}

impl dyn ServiceManager {
    /// Creates a new service using the specified type, falling back to selecting
    /// based on native targeting for the current operating system if no type provided
    pub fn target_or_native(
        kind: impl Into<Option<ServiceManagerKind>>,
    ) -> io::Result<Box<dyn ServiceManager>> {
        match kind.into() {
            Some(kind) => Ok(<dyn ServiceManager>::target(kind)),
            None => <dyn ServiceManager>::native_target(),
        }
    }

    /// Creates a new service manager targeting the specific service manager kind using the
    /// default service manager instance
    pub fn target(kind: ServiceManagerKind) -> Box<dyn ServiceManager> {
        match kind {
            #[cfg(target_os = "macos")]
            ServiceManagerKind::Launchd => Box::new(launchd::LaunchdServiceManager::default()),
            #[cfg(unix)]
            ServiceManagerKind::OpenRc => Box::new(openrc::OpenRcServiceManager::default()),
            #[cfg(unix)]
            ServiceManagerKind::Rc => Box::new(rc::RcServiceManager::default()),
            #[cfg(windows)]
            ServiceManagerKind::Sc => todo!(),
            #[cfg(unix)]
            ServiceManagerKind::Systemd => todo!(),
        }
    }

    /// Attempts to select a native target for the current operating system
    ///
    /// * For MacOS, this will use [`LaunchdServiceManager`]
    /// * For Windows, this will use [`ScServiceManager`]
    /// * For BSD variants, this will use [`RcServiceManager`]
    /// * For Linux variants, this will use either [`SystemdServiceManager`] or [`OpenRcServiceManager`]
    pub fn native_target() -> io::Result<Box<dyn ServiceManager>> {
        #[cfg(target_os = "macos")]
        fn native_target_kind() -> io::Result<ServiceManagerKind> {
            Ok(ServiceManagerKind::Launchd)
        }

        #[cfg(target_os = "windows")]
        fn native_target_kind() -> io::Result<ServiceManagerKind> {
            Ok(ServiceManagerKind::Sc)
        }

        #[cfg(any(
            target_os = "freebsd",
            target_os = "dragonfly",
            target_os = "openbsd",
            target_os = "netbsd"
        ))]
        fn native_target_kind() -> io::Result<ServiceManagerKind> {
            Ok(ServiceManagerKind::Rc)
        }

        #[cfg(target_os = "linux")]
        fn native_target_kind() -> io::Result<ServiceManagerKind> {
            let service = <dyn ServiceManager>::target(ServiceManagerKind::Systemd);
            if let Ok(true) = service.available() {
                return Ok(ServiceManagerKind::Systemd);
            }

            let service = <dyn ServiceManager>::target(ServiceManagerKind::OpenRc);
            if let Ok(true) = service.available() {
                return Ok(ServiceManagerKind::OpenRc);
            }

            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Only systemd and openrc are supported on Linux",
            ))
        }

        Ok(Self::target(native_target_kind()?))
    }
}

/// Label describing the service (e.g. `org.example.my_application`
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServiceLabel {
    /// Qualifier used for services tied to management systems like `launchd`
    ///
    /// E.g. `org` or `com`
    pub qualifier: String,

    /// Organization associated with the service
    ///
    /// E.g. `example`
    pub organization: String,

    /// Application name associated with the service
    ///
    /// E.g. `my_application`
    pub application: String,
}

impl ServiceLabel {
    /// Produces a fully-qualified name in the form of `{qualifier}.{organization}.{application}`
    pub fn to_qualified_name(&self) -> String {
        format!(
            "{}.{}.{}",
            self.qualifier, self.organization, self.application
        )
    }

    /// Produces a script name using the organization and application
    /// in the form of `{organization}-{application}`
    pub fn to_script_name(&self) -> String {
        format!("{}-{}", self.organization, self.application)
    }
}

impl fmt::Display for ServiceLabel {
    /// Produces a fully-qualified name
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}",
            self.qualifier, self.organization, self.application
        )
    }
}

impl FromStr for ServiceLabel {
    type Err = io::Error;

    /// Parses a fully-qualified name in the form of `{qualifier}.{organization}.{application}`
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let tokens = s.split('.').collect::<Vec<&str>>();
        if tokens.len() != 3 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                concat!(
                    "Unexpected token count! ",
                    "Expected 3 items in the form {qualifier}.{organization}.{application}"
                ),
            ));
        }

        Ok(Self {
            qualifier: tokens[0].to_string(),
            organization: tokens[1].to_string(),
            application: tokens[2].to_string(),
        })
    }
}

/// Context provided to the install function of [`ServiceManager`]
pub struct ServiceInstallCtx {
    /// Label associated with the service
    ///
    /// E.g. `org.example.my_application`
    pub label: ServiceLabel,

    /// Whether or not this service is at the user-level
    pub user: bool,

    /// Path to the program to run
    ///
    /// E.g. `/usr/local/bin/my-program`
    pub program: String,

    /// Arguments to use for the program
    ///
    /// E.g. `--arg`, `value`, `--another-arg`
    pub args: Vec<String>,
}

impl ServiceInstallCtx {
    /// Iterator over the program and its arguments
    pub fn cmd_iter(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.program.as_str()).chain(self.args_iter())
    }

    /// Iterator over the program arguments
    pub fn args_iter(&self) -> impl Iterator<Item = &str> {
        self.args.iter().map(String::as_str)
    }
}

/// Context provided to the uninstall function of [`ServiceManager`]
pub struct ServiceUninstallCtx {
    /// Label associated with the service
    ///
    /// E.g. `rocks.distant.manager`
    pub label: ServiceLabel,

    /// Whether or not this service is at the user-level
    pub user: bool,
}

/// Context provided to the start function of [`ServiceManager`]
pub struct ServiceStartCtx {
    /// Label associated with the service
    ///
    /// E.g. `rocks.distant.manager`
    pub label: ServiceLabel,
}

/// Context provided to the stop function of [`ServiceManager`]
pub struct ServiceStopCtx {
    /// Label associated with the service
    ///
    /// E.g. `rocks.distant.manager`
    pub label: ServiceLabel,
}
