use super::{
    ServiceInstallCtx, ServiceManager, ServiceStartCtx, ServiceStopCtx, ServiceUninstallCtx,
};
use std::{io, path::PathBuf, process::Command};

static LAUNCHCTL: &str = "launchctl";

/// Implementation of [`ServiceManager`] for MacOS's [Launchd](https://en.wikipedia.org/wiki/Launchd)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchdServiceManager {
    /// If true, calls to install service will include `KeepAlive` flag set to true
    pub keep_alive: bool,
}

impl LaunchdServiceManager {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for LaunchdServiceManager {
    fn default() -> Self {
        Self { keep_alive: true }
    }
}

impl ServiceManager for LaunchdServiceManager {
    fn available(&self) -> io::Result<bool> {
        which::which(LAUNCHCTL)
            .map(|_| true)
            .map_err(|x| io::Error::new(io::ErrorKind::NotFound, x))
    }

    fn supports_user_level_service(&self) -> bool {
        true
    }

    fn install(&self, ctx: ServiceInstallCtx) -> io::Result<()> {
        let dir_path = if ctx.user {
            user_agent_dir_path()?
        } else {
            global_daemon_dir_path()
        };

        std::fs::create_dir_all(&dir_path)?;

        let qualified_name = ctx.label.to_qualified_name();
        let plist_path = dir_path.join(format!("{}.plist", qualified_name));
        let plist = make_plist(&qualified_name, ctx.cmd_iter(), self.keep_alive);
        std::fs::write(plist_path.as_path(), plist)?;

        launchctl("load", plist_path.to_string_lossy().as_ref())
    }

    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()> {
        let dir_path = if ctx.user {
            user_agent_dir_path()?
        } else {
            global_daemon_dir_path()
        };
        let qualified_name = ctx.label.to_qualified_name();
        let plist_path = dir_path.join(format!("{}.plist", qualified_name));

        launchctl("unload", plist_path.to_string_lossy().as_ref())?;
        std::fs::remove_file(plist_path)
    }

    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()> {
        launchctl("start", &ctx.label.to_qualified_name())
    }

    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()> {
        launchctl("stop", &ctx.label.to_qualified_name())
    }
}

fn launchctl(cmd: &str, label: &str) -> io::Result<()> {
    let output = Command::new(LAUNCHCTL).arg(cmd).arg(label).output()?;

    if output.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8(output.stderr)
            .ok()
            .filter(|s| !s.trim().is_empty())
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
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Unable to locate home directory"))?
        .join("Library")
        .join("LaunchAgents"))
}

fn make_plist<'a>(label: &str, args: impl Iterator<Item = &'a str>, keep_alive: bool) -> String {
    let args = args
        .map(|arg| format!("<string>{arg}</string>"))
        .collect::<Vec<String>>()
        .join("");
    format!(r#"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
    <dict>
        <key>Label</key>
        <string>{label}</string>
        <key>ProgramArguments</key>
        <array>
            {args}
        </array>
        <key>KeepAlive</key>
        <{keep_alive}/>
    </dict>
</plist>
"#).trim().to_string()
}
