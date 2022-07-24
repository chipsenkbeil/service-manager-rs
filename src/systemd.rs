use super::{
    utils, ServiceInstallCtx, ServiceLevel, ServiceManager, ServiceStartCtx, ServiceStopCtx,
    ServiceUninstallCtx,
};
use std::{
    ffi::OsString,
    fmt, io,
    path::PathBuf,
    process::{Command, Stdio},
};

static SYSTEMCTL: &str = "systemctl";
const SERVICE_FILE_PERMISSIONS: u32 = 0o644;

/// Configuration settings tied to systemd services
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemdConfig {
    pub install: SystemdInstallConfig,
}

/// Configuration settings tied to systemd services during installation
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemdInstallConfig {
    pub start_limit_interval_sec: Option<u32>,
    pub start_limit_burst: Option<u32>,
    pub restart: SystemdServiceRestartType,
    pub restart_sec: Option<u32>,
}

impl Default for SystemdInstallConfig {
    fn default() -> Self {
        Self {
            start_limit_interval_sec: None,
            start_limit_burst: None,
            restart: SystemdServiceRestartType::OnFailure,
            restart_sec: None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SystemdServiceRestartType {
    No,
    Always,
    OnSuccess,
    OnFailure,
    OnAbnormal,
    OnAbort,
    OnWatch,
}

impl Default for SystemdServiceRestartType {
    fn default() -> Self {
        Self::No
    }
}

impl fmt::Display for SystemdServiceRestartType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::No => write!(f, "no"),
            Self::Always => write!(f, "always"),
            Self::OnSuccess => write!(f, "on-success"),
            Self::OnFailure => write!(f, "on-failure"),
            Self::OnAbnormal => write!(f, "on-abnormal"),
            Self::OnAbort => write!(f, "on-abort"),
            Self::OnWatch => write!(f, "on-watch"),
        }
    }
}

/// Implementation of [`ServiceManager`] for Linux's [systemd](https://en.wikipedia.org/wiki/Systemd)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemdServiceManager {
    /// Whether or not this manager is operating at the user-level
    pub user: bool,

    /// Configuration settings tied to systemd services
    pub config: SystemdConfig,
}

impl SystemdServiceManager {
    /// Creates a new manager instance working with system services
    pub fn system() -> Self {
        Self::default()
    }

    /// Creates a new manager instance working with user services
    pub fn user() -> Self {
        Self::default().into_user()
    }

    /// Change manager to work with system services
    pub fn into_system(self) -> Self {
        Self {
            config: self.config,
            user: false,
        }
    }

    /// Change manager to work with user services
    pub fn into_user(self) -> Self {
        Self {
            config: self.config,
            user: true,
        }
    }

    /// Update manager to use the specified config
    pub fn with_config(self, config: SystemdConfig) -> Self {
        Self {
            config,
            user: self.user,
        }
    }
}

impl ServiceManager for SystemdServiceManager {
    fn available(&self) -> io::Result<bool> {
        match which::which(SYSTEMCTL) {
            Ok(_) => Ok(true),
            Err(which::Error::CannotFindBinaryPath) => Ok(false),
            Err(x) => Err(io::Error::new(io::ErrorKind::Other, x)),
        }
    }

    fn install(&self, ctx: ServiceInstallCtx) -> io::Result<()> {
        let dir_path = if self.user {
            user_dir_path()?
        } else {
            global_dir_path()
        };

        std::fs::create_dir_all(&dir_path)?;

        let script_name = ctx.label.to_script_name();
        let script_path = dir_path.join(format!("{script_name}.service"));
        let service = make_service(
            &self.config.install,
            &script_name,
            ctx.program.into_os_string(),
            ctx.args,
            self.user,
        );

        utils::write_file(
            script_path.as_path(),
            service.as_bytes(),
            SERVICE_FILE_PERMISSIONS,
        )?;

        systemctl("enable", script_path.to_string_lossy().as_ref(), self.user)
    }

    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()> {
        let dir_path = if self.user {
            user_dir_path()?
        } else {
            global_dir_path()
        };
        let script_name = ctx.label.to_script_name();
        let script_path = dir_path.join(format!("{script_name}.service"));

        systemctl("disable", script_path.to_string_lossy().as_ref(), self.user)?;
        std::fs::remove_file(script_path)
    }

    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()> {
        systemctl("start", &ctx.label.to_script_name(), self.user)
    }

    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()> {
        systemctl("stop", &ctx.label.to_script_name(), self.user)
    }

    fn level(&self) -> ServiceLevel {
        if self.user {
            ServiceLevel::User
        } else {
            ServiceLevel::System
        }
    }

    fn set_level(&mut self, level: ServiceLevel) -> io::Result<()> {
        match level {
            ServiceLevel::System => self.user = false,
            ServiceLevel::User => self.user = true,
        }

        Ok(())
    }
}

fn systemctl(cmd: &str, label: &str, user: bool) -> io::Result<()> {
    let output = {
        let mut command = Command::new(SYSTEMCTL);

        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if user {
            command.arg("--user");
        }

        command.arg(cmd).arg(label).output()?
    };

    if output.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8(output.stderr)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                String::from_utf8(output.stdout)
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| format!("Failed to {cmd} for {label}"));

        Err(io::Error::new(io::ErrorKind::Other, msg))
    }
}

#[inline]
fn global_dir_path() -> PathBuf {
    PathBuf::from("/etc/systemd/system")
}

fn user_dir_path() -> io::Result<PathBuf> {
    Ok(dirs::config_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Unable to locate home directory"))?
        .join("systemd")
        .join("user"))
}

fn make_service(
    config: &SystemdInstallConfig,
    description: &str,
    program: OsString,
    args: Vec<OsString>,
    user: bool,
) -> String {
    use std::fmt::Write as _;
    let SystemdInstallConfig {
        start_limit_interval_sec,
        start_limit_burst,
        restart,
        restart_sec,
    } = config;

    let mut service = String::new();
    let _ = writeln!(service, "[Unit]");
    let _ = writeln!(service, "Description={description}");

    if let Some(x) = start_limit_interval_sec {
        let _ = writeln!(service, "StartLimitIntervalSec={x}");
    }

    if let Some(x) = start_limit_burst {
        let _ = writeln!(service, "StartLimitBurst={x}");
    }

    let _ = writeln!(service, "[Service]");

    let program = program.to_string_lossy();
    let args = args
        .into_iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<String>>()
        .join(" ");
    let _ = writeln!(service, "ExecStart={program} {args}");

    if *restart != SystemdServiceRestartType::No {
        let _ = writeln!(service, "Restart={restart}");
    }

    if let Some(x) = restart_sec {
        let _ = writeln!(service, "RestartSec={x}");
    }

    let _ = writeln!(service, "[Install]");

    if user {
        let _ = writeln!(service, "WantedBy=default.target");
    } else {
        let _ = writeln!(service, "WantedBy=multi-user.target");
    }

    service.trim().to_string()
}
