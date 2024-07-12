use crate::utils::wrap_output;

use super::{
    ServiceInstallCtx, ServiceLevel, ServiceManager, ServiceStartCtx, ServiceStopCtx,
    ServiceUninstallCtx,
};
use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    fmt, io,
    process::{Command, Output, Stdio},
};

#[cfg(windows)]
mod shell_escape;

#[cfg(not(windows))]
mod shell_escape {
    use std::{borrow::Cow, ffi::OsStr};

    /// When not on windows, this will do nothing but return the input str
    pub fn escape(s: Cow<'_, OsStr>) -> Cow<'_, OsStr> {
        s
    }
}

static SC_EXE: &str = "sc.exe";

/// Configuration settings tied to sc.exe services
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScConfig {
    pub install: ScInstallConfig,
}

/// Configuration settings tied to sc.exe services during installation
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScInstallConfig {
    /// Type of windows service for install
    pub service_type: WindowsServiceType,

    /// Start type for windows service for install
    pub start_type: WindowsStartType,

    /// Severity of the error if the windows service fails when the computer is started
    pub error_severity: WindowsErrorSeverity,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum WindowsServiceType {
    /// Service runs in its own process. It does not share an executable file with other services
    Own,

    /// Service runs as a shared process. It shares an executable file with other services
    Share,

    /// Service is a driver
    Kernel,

    /// Service is a file-system driver
    FileSys,

    /// Server is a file system recognized driver (identifies file systems used on the computer)
    Rec,
}

impl Default for WindowsServiceType {
    fn default() -> Self {
        Self::Own
    }
}

impl fmt::Display for WindowsServiceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Own => write!(f, "own"),
            Self::Share => write!(f, "share"),
            Self::Kernel => write!(f, "kernel"),
            Self::FileSys => write!(f, "filesys"),
            Self::Rec => write!(f, "rec"),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum WindowsStartType {
    /// Specifies a device driver that is loaded by the boot loader
    Boot,

    /// Specifies a device driver that is started during kernel initialization
    System,

    /// Specifies a service that automatically starts each time the computer is restarted. Note
    /// that the service runs even if no one logs on to the computer
    Auto,

    /// Specifies a service that must be started manually
    Demand,

    /// Specifies a service that cannot be started. To start a disabled service, change the start
    /// type to some other value.
    Disabled,
}

impl Default for WindowsStartType {
    fn default() -> Self {
        Self::Auto
    }
}

impl fmt::Display for WindowsStartType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boot => write!(f, "boot"),
            Self::System => write!(f, "system"),
            Self::Auto => write!(f, "auto"),
            Self::Demand => write!(f, "demand"),
            Self::Disabled => write!(f, "disabled"),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum WindowsErrorSeverity {
    /// Specifies that the error is logged. A message box is displayed, informing the user that a service has failed to start. Startup will continue
    Normal,

    /// Specifies that the error is logged (if possible). The computer attempts to restart with the
    /// last-known good configuration. This could result in the computer being able to restart, but
    /// the service may still be unable to run
    Severe,

    /// Specifies that the error is logged (if possible). The computer attempts to restart with the
    /// last-known good configuration. If the last-known good configuration fails, startup also
    /// fails, and the boot process halts with a Stop error
    Critical,

    /// Specifies that the error is logged and startup continues. No notification is given to the
    /// user beyond recording the error in the event log
    Ignore,
}

impl Default for WindowsErrorSeverity {
    fn default() -> Self {
        Self::Normal
    }
}

impl fmt::Display for WindowsErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::Severe => write!(f, "severe"),
            Self::Critical => write!(f, "critical"),
            Self::Ignore => write!(f, "ignore"),
        }
    }
}

/// Implementation of [`ServiceManager`] for [Window Service](https://en.wikipedia.org/wiki/Windows_service)
/// leveraging [`sc.exe`](https://docs.microsoft.com/en-us/previous-versions/windows/it-pro/windows-server-2012-r2-and-2012/cc754599(v=ws.11))
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScServiceManager {
    /// Configuration settings tied to rc.d services
    pub config: ScConfig,
}

impl ScServiceManager {
    /// Creates a new manager instance working with system services
    pub fn system() -> Self {
        Self::default()
    }

    /// Update manager to use the specified config
    pub fn with_config(self, config: ScConfig) -> Self {
        Self { config }
    }
}

impl ServiceManager for ScServiceManager {
    fn available(&self) -> io::Result<bool> {
        match which::which(SC_EXE) {
            Ok(_) => Ok(true),
            Err(which::Error::CannotFindBinaryPath) => Ok(false),
            Err(x) => Err(io::Error::new(io::ErrorKind::Other, x)),
        }
    }

    fn install(&self, ctx: ServiceInstallCtx) -> io::Result<()> {
        let service_name = ctx.label.to_qualified_name();

        let service_type = OsString::from(self.config.install.service_type.to_string());
        let error_severity = OsString::from(self.config.install.error_severity.to_string());
        let start_type = if ctx.autostart {
            OsString::from("Auto")
        } else {
            // TODO: Perhaps it could be useful to make `start_type` an `Option`? That way you
            // could have `Auto`/`Demand` based on `autostart`, and if `start_type` is set, its
            // special value will override `autostart`.
            OsString::from(self.config.install.start_type.to_string())
        };

        // Build our binary including arguments, following similar approach as windows-service-rs
        let mut binpath = OsString::new();
        binpath.push(shell_escape::escape(Cow::Borrowed(ctx.program.as_ref())));
        for arg in ctx.args_iter() {
            binpath.push(" ");
            binpath.push(shell_escape::escape(Cow::Borrowed(arg)));
        }

        let display_name = OsStr::new(&service_name);

        wrap_output(sc_exe(
            "create",
            &service_name,
            [
                // type= {service_type}
                OsStr::new("type="),
                service_type.as_os_str(),
                // start= {start_type}
                OsStr::new("start="),
                start_type.as_os_str(),
                // error= {error_severity}
                OsStr::new("error="),
                error_severity.as_os_str(),
                // binpath= "{program} {args}"
                OsStr::new("binpath="),
                binpath.as_os_str(),
                // displayname= {display_name}
                OsStr::new("displayname="),
                display_name,
            ],
        )?)?;
        Ok(())
    }

    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()> {
        let service_name = ctx.label.to_qualified_name();
        wrap_output(sc_exe("delete", &service_name, [])?)?;
        Ok(())
    }

    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()> {
        let service_name = ctx.label.to_qualified_name();
        wrap_output(sc_exe("start", &service_name, [])?)?;
        Ok(())
    }

    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()> {
        let service_name = ctx.label.to_qualified_name();
        wrap_output(sc_exe("stop", &service_name, [])?)?;
        Ok(())
    }

    fn level(&self) -> ServiceLevel {
        ServiceLevel::System
    }

    fn set_level(&mut self, level: ServiceLevel) -> io::Result<()> {
        match level {
            ServiceLevel::System => Ok(()),
            ServiceLevel::User => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "sc.exe does not support user-level services",
            )),
        }
    }

    fn status(&self, ctx: crate::ServiceStatusCtx) -> io::Result<crate::ServiceStatus> {
        let service_name = ctx.label.to_qualified_name();
        let output = sc_exe("query", &service_name, [])?;

        if !output.status.success() {
            if output.status.code() == Some(1060) {
                // 1060 = The specified service does not exist as an installed service.
                return Ok(crate::ServiceStatus::NotInstalled);
            }
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Command failed with exit code {}: {}",
                    output.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout
            .split('\n')
            .filter(|line| line.trim().starts_with("state"))
            .next();
        let status = match line {
            Some(line) if line.contains("RUNNING") => crate::ServiceStatus::Running,
            _ => crate::ServiceStatus::Stopped(None), // TODO: more statuses?
        };
        Ok(status)
    }
}

fn sc_exe<'a>(
    cmd: &str,
    service_name: &str,
    args: impl IntoIterator<Item = &'a OsStr>,
) -> io::Result<Output> {
    let mut command = Command::new(SC_EXE);

    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    command.arg(cmd).arg(service_name);

    for arg in args {
        command.arg(arg);
    }

    command.output()
}
