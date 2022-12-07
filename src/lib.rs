use std::{
    ffi::{OsStr, OsString},
    fmt, io,
    path::PathBuf,
    str::FromStr,
};

mod kind;
mod launchd;
mod openrc;
mod rcd;
mod sc;
mod systemd;
mod typed;
mod utils;

pub use kind::*;
pub use launchd::*;
pub use openrc::*;
pub use rcd::*;
pub use sc::*;
pub use systemd::*;
pub use typed::*;

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
        native_service_manager()
    }
}


    /// Attempts to select a native service manager for the current operating system1
    ///
    /// * For MacOS, this will use [`LaunchdServiceManager`]
    /// * For Windows, this will use [`ScServiceManager`]
    /// * For BSD variants, this will use [`RcdServiceManager`]
    /// * For Linux variants, this will use either [`SystemdServiceManager`] or [`OpenRcServiceManager`]
#[inline]
pub fn native_service_manager() -> io::Result<Box<dyn ServiceManager>> {
    Ok(TypedServiceManager::native()?.into_box())
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
    pub qualifier: Option<String>,

    /// Organization associated with the service
    ///
    /// E.g. `example`
    pub organization: Option<String>,

    /// Application name associated with the service
    ///
    /// E.g. `my_application`
    pub application: String,
}

impl ServiceLabel {
    /// Produces a fully-qualified name in the form of `{qualifier}.{organization}.{application}`
    pub fn to_qualified_name(&self) -> String {
        let mut qualified_name = String::new(); 
        if let Some(qualifier) = self.qualifier.as_ref() {
            qualified_name.push_str(qualifier.as_str());
            qualified_name.push('.');
        }
        if let Some(organization) = self.organization.as_ref() {
            qualified_name.push_str(organization.as_str());
            qualified_name.push('.');
        }
        qualified_name.push_str(self.application.as_str());
        qualified_name
    }

    /// Produces a script name using the organization and application
    /// in the form of `{organization}-{application}`
    pub fn to_script_name(&self) -> String {
        let mut script_name = String::new();
        if let Some(organization) = self.organization.as_ref() {
            script_name.push_str(organization.as_str());
            script_name.push('-');
        }
        script_name.push_str(self.application.as_str());
        script_name
    }
}

impl fmt::Display for ServiceLabel {
    /// Produces a fully-qualified name
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_qualified_name().as_str())
    }
}

impl FromStr for ServiceLabel {
    type Err = io::Error;

    /// Parses a fully-qualified name in the form of `{qualifier}.{organization}.{application}`
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let tokens = s.split('.').collect::<Vec<&str>>();

        let label = match tokens.len() {
            1 => Self {
                qualifier: None,
                organization: None,
                application: tokens[0].to_string(),
            },
            2 => Self {
                qualifier: None,
                organization: Some(tokens[0].to_string()),
                application: tokens[1].to_string(),
            },
            3 => Self {
                qualifier: Some(tokens[0].to_string()),
                organization: Some(tokens[1].to_string()),
                application: tokens[2].to_string(),
            },
            _ => Self {
                qualifier: Some(tokens[0].to_string()),
                organization: Some(tokens[1].to_string()),
                application: (&tokens[2..]).join("."),
            }
        };

        Ok(label)
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_label_parssing_1() {
        let label = ServiceLabel::from_str("com.example.app123").unwrap();

        assert_eq!(label.qualifier, Some("com".to_string()));
        assert_eq!(label.organization, Some("example".to_string()));
        assert_eq!(label.application, "app123".to_string());

        assert_eq!(label.to_qualified_name(), "com.example.app123");
        assert_eq!(label.to_script_name(), "example-app123");
    }

    #[test]
    fn test_service_label_parssing_2() {
        let label = ServiceLabel::from_str("example.app123").unwrap();

        assert_eq!(label.qualifier, None);
        assert_eq!(label.organization, Some("example".to_string()));
        assert_eq!(label.application, "app123".to_string());

        assert_eq!(label.to_qualified_name(), "example.app123");
        assert_eq!(label.to_script_name(), "example-app123");
    }

    #[test]
    fn test_service_label_parssing_3() {
        let label = ServiceLabel::from_str("app123").unwrap();

        assert_eq!(label.qualifier, None);
        assert_eq!(label.organization, None);
        assert_eq!(label.application, "app123".to_string());

        assert_eq!(label.to_qualified_name(), "app123");
        assert_eq!(label.to_script_name(), "app123");
    }

}