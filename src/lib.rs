use std::{
    ffi::{OsStr, OsString},
    fmt, io,
    path::PathBuf,
    str::FromStr,
};

mod kind;
mod typed;

#[cfg(target_os = "macos")]
mod launchd;

#[cfg(unix)]
mod openrc;

#[cfg(unix)]
mod rcd;

#[cfg(windows)]
mod sc;

#[cfg(unix)]
mod systemd;

pub use kind::ServiceManagerKind;
pub use typed::TypedServiceManager;

#[cfg(target_os = "macos")]
pub use launchd::LaunchdServiceManager;

#[cfg(unix)]
pub use openrc::OpenRcServiceManager;

#[cfg(unix)]
pub use rcd::RcdServiceManager;

#[cfg(windows)]
pub use sc::ScServiceManager;

#[cfg(unix)]
pub use systemd::SystemdServiceManager;

/// Interface for a service manager
pub trait ServiceManager {
    /// Determines if the service manager exists (e.g. is `launchd` available on the system?) and
    /// can be used
    fn available(&self) -> io::Result<bool>;

    /// Installs a new service using the manager
    fn install(&self, ctx: ServiceInstallCtx) -> io::Result<()>;

    /// Uninstalls an existing service using the manager
    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()>;

    /// Starts a service using the manager
    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()>;

    /// Stops a running service using the manager
    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()>;

    /// Returns the current target level for the manager
    fn level(&self) -> ServiceLevel;

    /// Sets the target level for the manager
    fn set_level(&mut self, level: ServiceLevel) -> io::Result<()>;
}

impl dyn ServiceManager {
    /// Creates a new service using the specified type, falling back to selecting
    /// based on native service manager for the current operating system if no type provided
    pub fn target_or_native(
        kind: impl Into<Option<ServiceManagerKind>>,
    ) -> io::Result<Box<dyn ServiceManager>> {
        Ok(TypedServiceManager::target_or_native(kind)?.into_box())
    }

    /// Creates a new service manager targeting the specific service manager kind using the
    /// default service manager instance
    pub fn target(kind: ServiceManagerKind) -> Box<dyn ServiceManager> {
        TypedServiceManager::target(kind).into_box()
    }

    /// Attempts to select a native service manager for the current operating system
    ///
    /// * For MacOS, this will use [`LaunchdServiceManager`]
    /// * For Windows, this will use [`ScServiceManager`]
    /// * For BSD variants, this will use [`RcdServiceManager`]
    /// * For Linux variants, this will use either [`SystemdServiceManager`] or [`OpenRcServiceManager`]
    pub fn native() -> io::Result<Box<dyn ServiceManager>> {
        Ok(TypedServiceManager::native()?.into_box())
    }
}

impl<'a, S> From<S> for Box<dyn ServiceManager + 'a>
where
    S: ServiceManager + 'a,
{
    fn from(service_manager: S) -> Self {
        Box::new(service_manager)
    }
}

/// Represents whether a service is system-wide or user-level
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ServiceLevel {
    System,
    User,
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

    /// Path to the program to run
    ///
    /// E.g. `/usr/local/bin/my-program`
    pub program: PathBuf,

    /// Arguments to use for the program
    ///
    /// E.g. `--arg`, `value`, `--another-arg`
    pub args: Vec<OsString>,
}

impl ServiceInstallCtx {
    /// Iterator over the program and its arguments
    pub fn cmd_iter(&self) -> impl Iterator<Item = &OsStr> {
        std::iter::once(self.program.as_os_str()).chain(self.args_iter())
    }

    /// Iterator over the program arguments
    pub fn args_iter(&self) -> impl Iterator<Item = &OsStr> {
        self.args.iter().map(OsString::as_os_str)
    }
}

/// Context provided to the uninstall function of [`ServiceManager`]
pub struct ServiceUninstallCtx {
    /// Label associated with the service
    ///
    /// E.g. `rocks.distant.manager`
    pub label: ServiceLabel,
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
