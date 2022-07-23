use super::{
    ServiceInstallCtx, ServiceLevel, ServiceManager, ServiceStartCtx, ServiceStopCtx,
    ServiceUninstallCtx,
};
use std::{ffi::OsString, io, path::PathBuf, process::Command};

static SYSTEMCTL: &str = "systemctl";

/// Configuration settings tied to systemd services
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemdConfig {}

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
        which::which(SYSTEMCTL)
            .map(|_| true)
            .map_err(|x| io::Error::new(io::ErrorKind::NotFound, x))
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
            &script_name,
            ctx.program.into_os_string(),
            ctx.args,
            self.user,
        );
        std::fs::write(script_path.as_path(), service)?;

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

fn make_service(description: &str, program: OsString, args: Vec<OsString>, user: bool) -> String {
    let program = program.to_string_lossy();
    let args = args
        .into_iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<String>>()
        .join(" ");
    let install = if user {
        ""
    } else {
        "
[Install]
WantedBy=multi-user.target
        "
        .trim()
    };

    format!(
        r#"
[Unit]
Description={description}
[Service]
ExecStart={program} {args}
{install}
"#
    )
    .trim()
    .to_string()
}
