use super::{
    ServiceInstallCtx, ServiceManager, ServiceStartCtx, ServiceStopCtx, ServiceUninstallCtx,
};
use std::{
    fs::OpenOptions,
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    process::Command,
};

static RC_SERVICE: &str = "rc-service";
static RC_UPDATE: &str = "rc-update";

/// Implementation of [`ServiceManager`] for Linux's [OpenRC](https://en.wikipedia.org/wiki/OpenRC)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenRcServiceManager;

impl OpenRcServiceManager {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ServiceManager for OpenRcServiceManager {
    fn available(&self) -> io::Result<bool> {
        which::which(RC_SERVICE)
            .map(|_| true)
            .map_err(|x| io::Error::new(io::ErrorKind::NotFound, x))
    }

    fn install(&self, ctx: ServiceInstallCtx) -> io::Result<()> {
        // NOTE: OpenRC does not support user-level services
        //
        // For more discussion, see: https://github.com/OpenRC/openrc/issues/432
        if ctx.user {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "User-level services not supported for OpenRC",
            ));
        }

        let dir_path = service_dir_path();
        std::fs::create_dir_all(&dir_path)?;

        let script_name = ctx.label.to_script_name();
        let script_path = dir_path.join(&script_name);

        let script = make_script(&script_name, &script_name, ctx.program.as_str(), ctx.args);

        // Create our script and ensure it is executable; fail if a script
        // exists at the location because we don't want to break something
        // and because OpenOptionsExt's mode(...) won't overwrite the
        // permissions of an existing file. We'd have to separately use
        // PermissionsExt to update those permissions if we wanted to
        // change an existing file's permissions
        //
        // NOTE: On Alpine Linux, /etc/init.d/{script} has permissions of rwxr-xr-x (755)
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o755)
            .open(script_path.as_path())?;
        file.write_all(script.as_bytes())?;

        rc_update("add", &script_name)
    }

    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()> {
        rc_update("delete", &ctx.label.to_script_name())
    }

    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()> {
        rc_service("start", &ctx.label.to_script_name())
    }

    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()> {
        rc_service("stop", &ctx.label.to_script_name())
    }
}

fn rc_service(cmd: &str, service: &str) -> io::Result<()> {
    let output = Command::new(RC_SERVICE).arg(service).arg(cmd).output()?;

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

fn rc_update(cmd: &str, service: &str) -> io::Result<()> {
    let output = Command::new(RC_UPDATE).arg(cmd).arg(service).output()?;

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

#[inline]
fn service_dir_path() -> PathBuf {
    PathBuf::from("/etc/init.d")
}

fn make_script(description: &str, provide: &str, program: &str, args: Vec<String>) -> String {
    format!(
        r#"
#!/sbin/openrc-run

description="{description}"
command="${{DISTANT_BINARY:-"{program}"}}"
command_args="{}"
pidfile="/run/${{RC_SVCNAME}}.pid"
command_background=true

depend() {{
    provide {provide}
}}
    "#,
        args.join(" "),
    )
    .trim()
    .to_string()
}
