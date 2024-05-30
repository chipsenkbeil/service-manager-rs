use super::{
    utils, ServiceInstallCtx, ServiceLevel, ServiceManager, ServiceStartCtx, ServiceStopCtx,
    ServiceUninstallCtx,
};
use std::{
    ffi::{OsStr, OsString},
    io,
    path::PathBuf,
    process::{Command, Stdio},
};

static RC_SERVICE: &str = "rc-service";
static RC_UPDATE: &str = "rc-update";

// NOTE: On Alpine Linux, /etc/init.d/{script} has permissions of rwxr-xr-x (755)
const SCRIPT_FILE_PERMISSIONS: u32 = 0o755;

/// Configuration settings tied to OpenRC services
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenRcConfig {}

/// Implementation of [`ServiceManager`] for Linux's [OpenRC](https://en.wikipedia.org/wiki/OpenRC)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenRcServiceManager {
    /// Configuration settings tied to OpenRC services
    pub config: OpenRcConfig,
}

impl OpenRcServiceManager {
    /// Creates a new manager instance working with system services
    pub fn system() -> Self {
        Self::default()
    }

    /// Update manager to use the specified config
    pub fn with_config(self, config: OpenRcConfig) -> Self {
        Self { config }
    }
}

impl ServiceManager for OpenRcServiceManager {
    fn available(&self) -> io::Result<bool> {
        match which::which(RC_SERVICE) {
            Ok(_) => Ok(true),
            Err(which::Error::CannotFindBinaryPath) => Ok(false),
            Err(x) => Err(io::Error::new(io::ErrorKind::Other, x)),
        }
    }

    fn install(&self, ctx: ServiceInstallCtx) -> io::Result<()> {
        let dir_path = service_dir_path();
        std::fs::create_dir_all(&dir_path)?;

        let script_name = ctx.label.to_script_name();
        let script_path = dir_path.join(&script_name);

        let script = match ctx.contents {
            Some(contents) => contents,
            _ => make_script(
                &script_name,
                &script_name,
                ctx.program.as_os_str(),
                ctx.args,
            ),
        };

        utils::write_file(
            script_path.as_path(),
            script.as_bytes(),
            SCRIPT_FILE_PERMISSIONS,
        )?;

        if ctx.autostart {
            // Add with default run level explicitly defined to prevent weird systems
            // like alpine's docker container with openrc from setting a different
            // run level than default
            rc_update("add", &script_name, [OsStr::new("default")])?;
        }

        Ok(())
    }

    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()> {
        rc_update(
            "delete",
            &ctx.label.to_script_name(),
            [OsStr::new("default")],
        )
    }

    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()> {
        rc_service("start", &ctx.label.to_script_name())
    }

    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()> {
        rc_service("stop", &ctx.label.to_script_name())
    }

    fn level(&self) -> ServiceLevel {
        ServiceLevel::System
    }

    fn set_level(&mut self, level: ServiceLevel) -> io::Result<()> {
        match level {
            ServiceLevel::System => Ok(()),
            ServiceLevel::User => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "OpenRC does not support user-level services",
            )),
        }
    }
}

fn rc_service(cmd: &str, service: &str) -> io::Result<()> {
    let output = Command::new(RC_SERVICE)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg(service)
        .arg(cmd)
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8(output.stderr)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("Failed to {cmd} {service}"));

        Err(io::Error::new(io::ErrorKind::Other, msg))
    }
}

fn rc_update<'a>(
    cmd: &str,
    service: &str,
    args: impl IntoIterator<Item = &'a OsStr>,
) -> io::Result<()> {
    let mut command = Command::new(RC_UPDATE);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg(cmd)
        .arg(service);

    for arg in args {
        command.arg(arg);
    }

    let output = command.output()?;

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
            .unwrap_or_else(|| format!("Failed to {cmd} {service}"));

        Err(io::Error::new(io::ErrorKind::Other, msg))
    }
}

#[inline]
fn service_dir_path() -> PathBuf {
    PathBuf::from("/etc/init.d")
}

fn make_script(description: &str, provide: &str, program: &OsStr, args: Vec<OsString>) -> String {
    let program = program.to_string_lossy();
    let args = args
        .into_iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<String>>()
        .join(" ");
    format!(
        r#"
#!/sbin/openrc-run

description="{description}"
command="{program}"
command_args="{args}"
pidfile="/run/${{RC_SVCNAME}}.pid"
command_background=true

depend() {{
    provide {provide}
}}
    "#
    )
    .trim()
    .to_string()
}
