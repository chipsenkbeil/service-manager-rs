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

static SERVICE: &str = "service";

// NOTE: On FreeBSD, /usr/local/etc/rc.d/{script} has permissions of rwxr-xr-x (755)
const SCRIPT_FILE_PERMISSIONS: u32 = 0o755;

/// Configuration settings tied to rc.d services
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RcdConfig {}

/// Implementation of [`ServiceManager`] for FreeBSD's [rc.d](https://en.wikipedia.org/wiki/Init#Research_Unix-style/BSD-style)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RcdServiceManager {
    /// Configuration settings tied to rc.d services
    pub config: RcdConfig,
}

impl RcdServiceManager {
    /// Creates a new manager instance working with system services
    pub fn system() -> Self {
        Self::default()
    }

    /// Update manager to use the specified config
    pub fn with_config(self, config: RcdConfig) -> Self {
        Self { config }
    }
}

impl ServiceManager for RcdServiceManager {
    fn available(&self) -> io::Result<bool> {
        match std::fs::metadata(service_dir_path()) {
            Ok(_) => Ok(true),
            Err(x) if x.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(x) => Err(x),
        }
    }

    fn install(&self, ctx: ServiceInstallCtx) -> io::Result<()> {
        let service = ctx.label.to_script_name();
        let script = match ctx.contents {
            Some(contents) => contents,
            _ => make_script(&service, &service, ctx.program.as_os_str(), ctx.args),
        };

        utils::write_file(
            &rc_d_script_path(&service),
            script.as_bytes(),
            SCRIPT_FILE_PERMISSIONS,
        )?;

        if ctx.autostart {
            rc_d_script("enable", &service)?;
        }

        Ok(())
    }

    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()> {
        let service = ctx.label.to_script_name();

        // Remove the service from rc.conf
        rc_d_script("delete", &service)?;

        // Delete the actual service file
        std::fs::remove_file(rc_d_script_path(&service))
    }

    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()> {
        let service = ctx.label.to_script_name();
        rc_d_script("start", &service)
    }

    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()> {
        let service = ctx.label.to_script_name();
        rc_d_script("stop", &service)
    }

    fn level(&self) -> ServiceLevel {
        ServiceLevel::System
    }

    fn set_level(&mut self, level: ServiceLevel) -> io::Result<()> {
        match level {
            ServiceLevel::System => Ok(()),
            ServiceLevel::User => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "rc.d does not support user-level services",
            )),
        }
    }
}

#[inline]
fn rc_d_script_path(name: &str) -> PathBuf {
    service_dir_path().join(name)
}

#[inline]
fn service_dir_path() -> PathBuf {
    PathBuf::from("/usr/local/etc/rc.d")
}

fn rc_d_script(cmd: &str, service: &str) -> io::Result<()> {
    // NOTE: We MUST mark stdout/stderr as null, otherwise this hangs. Attempting to use output()
    //       does not work. The alternative is to spawn threads to read the stdout and stderr,
    //       but that seems overkill for the purpose of displaying an error message.
    let status = Command::new(SERVICE)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .arg(service)
        .arg(cmd)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        let msg = format!("Failed to {cmd} {service}");
        Err(io::Error::new(io::ErrorKind::Other, msg))
    }
}

fn make_script(description: &str, provide: &str, program: &OsStr, args: Vec<OsString>) -> String {
    let name = provide.replace('-', "_");
    let program = program.to_string_lossy();
    let args = args
        .into_iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<String>>()
        .join(" ");
    format!(
        r#"
#!/bin/sh
#
# PROVIDE: {provide}
# REQUIRE: LOGIN FILESYSTEMS
# KEYWORD: shutdown

. /etc/rc.subr

name="{name}"
desc="{description}"
rcvar="{name}_enable"

load_rc_config ${{name}}

: ${{{name}_options="{args}"}}

pidfile="/var/run/{name}.pid"
procname="{program}"
command="/usr/sbin/daemon"
command_args="-c -S -T ${{name}} -p ${{pidfile}} ${{procname}} ${{{name}_options}}"

run_rc_command "$1"
    "#
    )
    .trim()
    .to_string()
}
