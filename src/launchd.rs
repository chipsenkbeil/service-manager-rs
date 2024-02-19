use super::{
    utils, ServiceInstallCtx, ServiceLevel, ServiceManager, ServiceStartCtx,
    ServiceStopCtx, ServiceUninstallCtx,
};
use plist::{Dictionary, Value};
use std::{
    ffi::OsStr,
    io,
    path::PathBuf,
    process::{Command, Stdio},
};

static LAUNCHCTL: &str = "launchctl";
const PLIST_FILE_PERMISSIONS: u32 = 0o644;

/// Configuration settings tied to launchd services
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchdConfig {
    pub install: LaunchdInstallConfig,
}

/// Configuration settings tied to launchd services during installation
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchdInstallConfig {
    /// If true, will include `KeepAlive` flag set to true
    pub keep_alive: bool,
}

impl Default for LaunchdInstallConfig {
    fn default() -> Self {
        Self { keep_alive: true }
    }
}

/// Implementation of [`ServiceManager`] for MacOS's [Launchd](https://en.wikipedia.org/wiki/Launchd)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchdServiceManager {
    /// Whether or not this manager is operating at the user-level
    pub user: bool,

    /// Configuration settings tied to launchd services
    pub config: LaunchdConfig,
}

impl LaunchdServiceManager {
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
    pub fn with_config(self, config: LaunchdConfig) -> Self {
        Self {
            config,
            user: self.user,
        }
    }

    fn get_plist_path(&self, qualified_name: String) -> PathBuf {
        let dir_path = if self.user {
            user_agent_dir_path().unwrap()
        } else {
            global_daemon_dir_path()
        };
        let plist_path = dir_path.join(format!("{}.plist", qualified_name));
        plist_path
    }
}

impl ServiceManager for LaunchdServiceManager {
    fn available(&self) -> io::Result<bool> {
        match which::which(LAUNCHCTL) {
            Ok(_) => Ok(true),
            Err(which::Error::CannotFindBinaryPath) => Ok(false),
            Err(x) => Err(io::Error::new(io::ErrorKind::Other, x)),
        }
    }

    fn install(&self, ctx: ServiceInstallCtx) -> io::Result<()> {
        let dir_path = if self.user {
            user_agent_dir_path()?
        } else {
            global_daemon_dir_path()
        };

        std::fs::create_dir_all(&dir_path)?;

        let qualified_name = ctx.label.to_qualified_name();
        let plist_path = dir_path.join(format!("{}.plist", qualified_name));
        let plist = match ctx.contents {
            Some(contents) => contents,
            _ => make_plist(
                &self.config.install,
                &qualified_name,
                ctx.cmd_iter(),
                ctx.username.clone(),
                ctx.working_directory.clone(),
                ctx.environment.clone(),
            ),
        };

        utils::write_file(
            plist_path.as_path(),
            plist.as_bytes(),
            PLIST_FILE_PERMISSIONS,
        )?;

        launchctl("load", plist_path.to_string_lossy().as_ref())
    }

    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()> {
        let plist_path = self.get_plist_path(ctx.label.to_qualified_name());

        launchctl("unload", plist_path.to_string_lossy().as_ref())?;
        std::fs::remove_file(plist_path)
    }

    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()> {
        let plist_path = self.get_plist_path(ctx.label.to_qualified_name());

        launchctl("load", plist_path.to_string_lossy().as_ref())?;
        launchctl("start", &ctx.label.to_qualified_name())
    }

    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()> {
        let plist_path = self.get_plist_path(ctx.label.to_qualified_name());

        let _ =launchctl("stop", &ctx.label.to_qualified_name());
        launchctl("unload", plist_path.to_string_lossy().as_ref())
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

fn launchctl(cmd: &str, label: &str) -> io::Result<()> {
    let output = Command::new(LAUNCHCTL)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg(cmd)
        .arg(label)
        .output()?;

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
fn global_daemon_dir_path() -> PathBuf {
    PathBuf::from("/Library/LaunchDaemons")
}

fn user_agent_dir_path() -> io::Result<PathBuf> {
    Ok(dirs::home_dir()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Unable to locate home directory",
            )
        })?
        .join("Library")
        .join("LaunchAgents"))
}

fn make_plist<'a>(
    config: &LaunchdInstallConfig,
    label: &str,
    args: impl Iterator<Item=&'a OsStr>,
    username: Option<String>,
    working_directory: Option<PathBuf>,
    environment: Option<Vec<(String, String)>>,
) -> String {
    let mut dict = Dictionary::new();

    dict.insert("Label".to_string(), Value::String(label.to_string()));

    let program_arguments: Vec<Value> = args
        .map(|arg| Value::String(arg.to_string_lossy().into_owned()))
        .collect();
    dict.insert(
        "ProgramArguments".to_string(),
        Value::Array(program_arguments),
    );

    dict.insert("KeepAlive".to_string(), Value::Boolean(config.keep_alive));

    if let Some(username) = username {
        dict.insert("UserName".to_string(), Value::String(username));
    }

    if let Some(working_dir) = working_directory {
        dict.insert(
            "WorkingDirectory".to_string(),
            Value::String(working_dir.to_string_lossy().to_string()),
        );
    }

    if let Some(env_vars) = environment {
        let env_dict: Dictionary = env_vars
            .into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect();
        dict.insert(
            "EnvironmentVariables".to_string(),
            Value::Dictionary(env_dict),
        );
    }

    let plist = Value::Dictionary(dict);

    let mut buffer = Vec::new();
    plist.to_writer_xml(&mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}
