use super::{
    utils, ServiceInstallCtx, ServiceLevel, ServiceManager, ServiceStartCtx, ServiceStopCtx,
    ServiceUninstallCtx,
};
use std::{
    io,
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
};

static SERVICE: &str = "service";

// NOTE: On FreeBSD, /usr/local/etc/rc.d/{script} has permissions of rwxr-xr-x (755)
const SCRIPT_FILE_PERMISSIONS: u32 = 0o755;

/// Configuration settings tied to rc.d services
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RcdConfig {
    pub install: RcdInstallConfig,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RcdInstallConfig {
    pub provide: Option<Vec<String>>,
    pub description: Option<String>,
    pub require: Option<Vec<String>>,
    pub before: Option<Vec<String>>,
}

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
        let script = make_script(&ctx, &self.config.install);

        utils::write_file(
            &rc_d_script_path(&service),
            script.as_bytes(),
            SCRIPT_FILE_PERMISSIONS,
        )?;

        if ctx.autostart {
            rc_d_script("enable", &service, true)?;
        }

        Ok(())
    }

    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()> {
        let service = ctx.label.to_script_name();

        // Remove the service from rc.conf
        rc_d_script("delete", &service, true)?;

        // Delete the actual service file
        std::fs::remove_file(rc_d_script_path(&service))
    }

    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()> {
        let service = ctx.label.to_script_name();
        rc_d_script("start", &service, true)?;
        Ok(())
    }

    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()> {
        let service = ctx.label.to_script_name();
        rc_d_script("stop", &service, true)?;
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
                "rc.d does not support user-level services",
            )),
        }
    }

    fn status(&self, ctx: crate::ServiceStatusCtx) -> io::Result<crate::ServiceStatus> {
        let service = ctx.label.to_script_name();
        let status = rc_d_script("status", &service, false)?;
        match status.code() {
            Some(0) => Ok(crate::ServiceStatus::Running),
            Some(3) => Ok(crate::ServiceStatus::Stopped(None)),
            Some(1) => Ok(crate::ServiceStatus::NotInstalled),
            _ => {
                let code = status.code().unwrap_or(-1);
                let msg = format!("Failed to get status of {service}, exit code: {code}");
                Err(io::Error::new(io::ErrorKind::Other, msg))
            }
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

fn rc_d_script(cmd: &str, service: &str, wrap: bool) -> io::Result<ExitStatus> {
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
    if wrap {
        if status.success() {
            Ok(status)
        } else {
            let msg = format!("Failed to {cmd} {service}");
            Err(io::Error::new(io::ErrorKind::Other, msg))
        }
    } else {
        Ok(status)
    }
}

fn make_script(ctx: &ServiceInstallCtx, config: &RcdInstallConfig) -> String {
    if let Some(ref contents) = ctx.contents {
        return contents.clone();
    }

    use std::fmt::Write;

    let script_name = ctx.label.to_script_name();
    let provide = utils::option_iterator_to_string(&config.provide, " ")
        .unwrap_or(ctx.label.to_script_name());
    let name = script_name.replace("-", "_");
    let description = config
        .description
        .as_deref()
        .and_then(|v| {
            let s = v.trim();
            (!s.is_empty()).then(|| s)
        })
        .unwrap_or(provide.as_str());
    let program = ctx.program.display().to_string();
    let args = ctx
        .args
        .iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<String>>()
        .join(" ");
    let require = utils::option_iterator_to_string(&config.require, " ")
        .unwrap_or("LOGIN FILESYSTEMS".to_string());

    let mut script = String::new();

    _ = writeln!(script, "#!/bin/sh");
    _ = writeln!(script, "#");
    _ = writeln!(script, "# PROVIDE: {provide}");
    _ = writeln!(script, "# REQUIRE: {require}");
    if let Some(before) = utils::option_iterator_to_string(&config.before, " ") {
        _ = writeln!(script, "# BEFORE: {before}");
    }
    _ = writeln!(script, "# KEYWORD: shutdown");
    _ = writeln!(script);
    _ = writeln!(script, ". /etc/rc.subr");
    _ = writeln!(script);
    _ = writeln!(script, "name=\"{name}\"");
    _ = writeln!(script, "desc=\"{description}\"");
    _ = writeln!(script, "rcvar=\"{name}_enable\"");
    _ = writeln!(script);
    _ = writeln!(script, "load_rc_config ${{name}}");
    _ = writeln!(script);
    _ = writeln!(script, ": ${{{name}_options=\"{args}\"}}");
    _ = writeln!(script);
    if let Some(ref x) = ctx.working_directory {
        let work_dir = x.display().to_string();
        _ = writeln!(script, "{name}_chdir=\"{work_dir}\"");
    }
    _ = writeln!(script, "pidfile=\"/var/run/${{name}}.pid\"");
    _ = writeln!(script, "procname=\"{program}\"");
    _ = writeln!(script, "command=\"/usr/sbin/daemon\"");
    _ = writeln!(
        script,
        "command_args=\"-c -S -T ${{name}} -p ${{pidfile}} ${{procname}} ${{{name}_options}}\""
    );
    _ = writeln!(script);
    _ = writeln!(script, "run_rc_command \"$1\"");

    script
}
