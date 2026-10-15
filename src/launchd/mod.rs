use crate::utils::wrap_output;

use super::{
    utils, RestartPolicy, ServiceInstallCtx, ServiceLabel, ServiceLevel, ServiceManager,
    ServiceStartCtx, ServiceStopCtx, ServiceUninstallCtx,
};
use plist::{Dictionary, Value};
use rustix::{fs::Uid, process::getuid};
use std::{
    convert::Infallible,
    error::Error,
    ffi::OsStr,
    fmt::Display,
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{self, Command, Output, Stdio},
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

mod launchctl;

use launchctl::{
    LaunchCtl, LaunchCtlBootoutError, LaunchCtlWaitError, ServiceInfo, ServicePrintoutError,
};

/// The appropriate file permissions for a PLIST file.
const PLIST_FILE_PERMISSIONS: u32 = 0o644;

/// How long `install()`, `stop()` and `uninstall()` wait for launchd to boot a service out.
///
/// `LaunchCtl::bootout` already bounds the wait by the job's own ExitTimeOut; this caps it for a
/// plist whose ExitTimeOut is longer than a caller should block. macOS's own daemons go up to
/// 240s (`com.apple.xsan`).
const BOOTOUT_TIMEOUT: Duration = Duration::from_secs(300);

/// The domain which a service can run.
///
/// `launchd` supports user, gui and system services. It supports more, but they appear to be
/// irrelevant.
///
/// System services are services which run on the system for all the time through which a system is
/// booted up.
///
/// Gui services (or gui agents) start when a user logs into the graphical session and last for as
/// long as the user is logged in.
///
/// User services are background services that run on a per-user basis. The lifetime appears
/// unclear, as the man page says:
///
/// > Targets the user domain for the given UID or a service within that domain. A user domain may
/// > exist independently of a logged-in user.
///
/// For more details on these, please see `man launchctl`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ServiceDomain {
    /// `user/<uid>/` services.
    ///
    /// launchd loads a plist into this domain only if it sets `LimitLoadToSessionType` to
    /// `Background`; bootstrap fails with EIO otherwise.
    User(Uid),
    /// `gui/<uid>/` services.
    Gui(Uid),
    /// `system/` services.
    #[default]
    System,
}

impl ServiceDomain {
    /// Infer the service domain from the active execution context.
    ///
    /// Deprecated `launchctl` commands would infer the domain in which services would run. For
    /// example, `launchctl load` would request a service be loaded into an inferred domain. This
    /// function models this inference.
    #[must_use]
    pub fn infer() -> Self {
        let uid = rustix::process::getuid();

        if uid.is_root() {
            ServiceDomain::System
        } else {
            ServiceDomain::Gui(uid)
        }
    }

    /// Determine the appropriate path for creating service plists.
    ///
    /// `gui/<uid>` and `user/<uid>` agents live in that user's `~/Library/LaunchAgents`, so this
    /// looks the user's home directory up and fails when no user has the uid.
    pub fn plist_dir(self) -> io::Result<PathBuf> {
        let uid = match self {
            ServiceDomain::System => return Ok(PathBuf::from("/Library/LaunchDaemons")),
            ServiceDomain::User(uid) | ServiceDomain::Gui(uid) => uid,
        };
        let user = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid.as_raw()))?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("no user has uid {}", uid.as_raw()),
                )
            })?;

        Ok(user.dir.join("Library").join("LaunchAgents"))
    }
}

impl Display for ServiceDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceDomain::User(uid) => write!(f, "user/{}", uid.as_raw()),
            ServiceDomain::Gui(uid) => write!(f, "gui/{}", uid.as_raw()),
            ServiceDomain::System => write!(f, "system"),
        }
    }
}

/// A service in a launchd domain, held as the `<domain>/<label>` service target launchctl takes.
struct ServiceTarget {
    domain: ServiceDomain,
    /// `<domain>/<label>`.
    service_target: String,
    /// The index of the `/` between the domain and the label in `service_target`.
    separator: usize,
}

impl ServiceTarget {
    /// The service `label` in `domain`.
    pub fn new(domain: ServiceDomain, label: &str) -> Self {
        use std::fmt::Write as _;

        // The longest domain is `user/4294967295`.
        const MAX_DOMAIN_LEN: usize = 15;

        let mut service_target = String::with_capacity(MAX_DOMAIN_LEN + 1 + label.len());
        write!(service_target, "{domain}").expect("ServiceDomain's Display never fails");

        let separator = service_target.len();
        service_target.push('/');
        service_target.push_str(label);

        Self {
            domain,
            service_target,
            separator,
        }
    }

    /// The domain half of the service target, such as `gui/501`.
    pub fn domain_target(&self) -> &str {
        &self.service_target[..self.separator]
    }

    /// The service's label.
    pub fn label(&self) -> &str {
        &self.service_target[self.separator + 1..]
    }

    /// The plist launchd loads the service from.
    pub fn plist_path(&self) -> io::Result<PathBuf> {
        Ok(self
            .domain
            .plist_dir()?
            .join(format!("{}.plist", self.label())))
    }

    /// The service's info, or `None` when it is not loaded.
    pub fn info(&self) -> Result<Option<ServiceInfo>, ServicePrintoutError> {
        LaunchCtl::print_service(&self.service_target)
    }

    /// Polls the service's info until `done` holds or the service is not loaded.
    ///
    /// See [`LaunchCtl::wait_until`].
    pub fn wait_until(
        &self,
        timeout: Duration,
        done: impl Fn(&ServiceInfo) -> bool,
    ) -> Result<Option<ServiceInfo>, LaunchCtlWaitError> {
        LaunchCtl::wait_until(&self.service_target, timeout, done)
    }

    /// Loads the plist at `plist_path` into the service's domain.
    pub fn bootstrap(&self, plist_path: &Path) -> io::Result<()> {
        LaunchCtl::bootstrap_plists(self.domain_target(), std::iter::once(plist_path))?;
        Ok(())
    }

    /// Removes the service from its domain and waits for launchd to finish tearing it down.
    ///
    /// See [`LaunchCtl::bootout`].
    pub fn bootout(&self, timeout: Duration) -> Result<(), LaunchCtlBootoutError> {
        LaunchCtl::bootout(&self.service_target, timeout)
    }

    /// Boots the service out, counting a service that was not loaded as booted out.
    pub fn bootout_if_loaded(&self) -> io::Result<()> {
        match self.bootout(BOOTOUT_TIMEOUT) {
            Ok(()) | Err(LaunchCtlBootoutError::NotLoaded) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    /// Starts the loaded service.
    pub fn kickstart(&self) -> io::Result<()> {
        LaunchCtl::kickstart(&self.service_target)?;
        Ok(())
    }

    /// The service's status as [`ServiceManager::status`] reports it.
    pub fn status(&self) -> io::Result<crate::ServiceStatus> {
        let info = match self.info() {
            Ok(info) => info,
            // With no graphical login the job cannot run, but launchd loads its plist at the next
            // login, so the plist decides, as it does for a job that is not loaded.
            Err(ServicePrintoutError::DomainNotFound(_)) => None,
            Err(err) => return Err(err.into()),
        };

        match info {
            Some(info) if info.state.is_running() => Ok(crate::ServiceStatus::Running),
            Some(_) => Ok(crate::ServiceStatus::Stopped(None)),
            None if self.plist_path()?.try_exists()? => Ok(crate::ServiceStatus::Stopped(None)),
            None => Ok(crate::ServiceStatus::NotInstalled),
        }
    }
}

impl Display for ServiceTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.service_target)
    }
}

/// Configuration settings tied to launchd services
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchdConfig {
    pub install: LaunchdInstallConfig,
}

/// Configuration settings tied to launchd services during installation
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchdInstallConfig {
    /// Launchd-specific keep-alive setting. If `Some`, this takes precedence over the generic
    /// `RestartPolicy` in `ServiceInstallCtx`. If `None`, the generic policy is used.
    pub keep_alive: Option<bool>,
}

impl Default for LaunchdInstallConfig {
    fn default() -> Self {
        Self { keep_alive: None }
    }
}

/// Implementation of [`ServiceManager`] for MacOS's [Launchd](https://en.wikipedia.org/wiki/Launchd)
///
/// System services live in the `system` domain. User services live in the calling user's
/// `gui/<uid>` domain, which exists only while that user is logged in graphically. `user/<uid>`
/// exists without a graphical login, but loads only plists that set `LimitLoadToSessionType` to
/// `Background`, which the generated plist does not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchdServiceManager {
    /// The domain the service covers.
    pub domain: ServiceDomain,

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
            domain: ServiceDomain::System,
        }
    }

    /// Change manager to work with user services
    pub fn into_user(self) -> Self {
        Self {
            config: self.config,
            domain: ServiceDomain::Gui(getuid()),
        }
    }

    /// Update manager to use the specified config
    pub fn with_config(self, config: LaunchdConfig) -> Self {
        Self { config, ..self }
    }

    fn plist_dir(&self) -> io::Result<PathBuf> {
        self.domain.plist_dir()
    }

    /// The service `label` in this manager's domain.
    fn target(&self, label: &ServiceLabel) -> ServiceTarget {
        ServiceTarget::new(self.domain, &label.to_qualified_name())
    }
}

impl ServiceManager for LaunchdServiceManager {
    fn available(&self) -> io::Result<bool> {
        Ok(LaunchCtl::available())
    }

    fn install(&self, ctx: ServiceInstallCtx) -> io::Result<()> {
        std::fs::create_dir_all(self.plist_dir()?)?;

        let qualified_name = ctx.label.to_qualified_name();
        let plist = match ctx.contents {
            Some(contents) => contents,
            _ => make_plist(
                &self.config.install,
                &qualified_name,
                ctx.cmd_iter(),
                ctx.username.clone(),
                ctx.working_directory.clone(),
                ctx.environment.clone(),
                ctx.autostart,
                ctx.restart_policy,
            ),
        };

        let target = ServiceTarget::new(self.domain, &qualified_name);
        let plist_path = target.plist_path()?;
        target.bootout_if_loaded()?;
        utils::write_file(
            plist_path.as_path(),
            plist.as_bytes(),
            PLIST_FILE_PERMISSIONS,
        )?;

        // bootstrap refuses a plist that sets Disabled, which make_plist does for KeepAlive
        // services so that installing one does not start it. start() lifts it.
        if sets_disabled(plist.as_bytes()) {
            return Ok(());
        }

        // As with `launchctl load`, RunAtLoad (autostart) runs the job now. A plist launchd
        // refuses is removed again so that a failed install() leaves nothing installed; the
        // bootstrap failure is the error worth reporting.
        target.bootstrap(&plist_path).map_err(|err| {
            let _ = std::fs::remove_file(&plist_path);
            err
        })
    }

    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()> {
        let target = self.target(&ctx.label);
        let plist_path = target.plist_path()?;
        target.bootout_if_loaded()?;
        std::fs::remove_file(&plist_path).or_else(|err| match err.kind() {
            io::ErrorKind::NotFound => Ok(()),
            _ => Err(err),
        })
    }

    /// Starts a service, loading it into its launchd domain first if it is not loaded.
    ///
    /// Blocks while launchd tears down a job that is being stopped, and while it throttles a job
    /// that exited less than its ThrottleInterval (`10s` by default) ago.
    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()> {
        let target = self.target(&ctx.label);
        let plist_path = target.plist_path()?;
        if !plist_path.try_exists()? {
            return Err(not_installed(&ctx.label));
        }

        // kickstart does nothing to a job launchd is tearing down, so let the teardown finish
        // and see whether the job is still loaded afterwards.
        let info = match target.info()? {
            Some(info) if info.state.is_stopping() => {
                target.wait_until(info.teardown_timeout(), |info| !info.state.is_stopping())?
            }
            info => info,
        };
        if info.is_some() {
            return target.kickstart();
        }

        lift_disabled(&plist_path).map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => not_installed(&ctx.label),
            _ => err,
        })?;
        // A concurrent start() can load the job first, which fails this bootstrap with EIO, and a
        // concurrent uninstall() can remove the plist, so the state afterwards decides.
        let bootstrapped = target.bootstrap(&plist_path);
        match target.info()? {
            // RunAtLoad and KeepAlive spawn the job during bootstrap, and kickstarting a job that
            // already ran and exited would run it a second time.
            Some(info) if info.runs > 0 => Ok(()),
            Some(_) => target.kickstart(),
            None if !plist_path.try_exists()? => Err(not_installed(&ctx.label)),
            None => bootstrapped,
        }
    }

    /// Stops a service by removing it from its launchd domain, so KeepAlive cannot restart it.
    ///
    /// The plist stays in place, so launchd loads the service again at the next boot or login
    /// unless the plist sets `Disabled`.
    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()> {
        let target = self.target(&ctx.label);
        match target.bootout(BOOTOUT_TIMEOUT) {
            Ok(()) => Ok(()),
            Err(LaunchCtlBootoutError::NotLoaded) if target.plist_path()?.try_exists()? => Ok(()),
            Err(LaunchCtlBootoutError::NotLoaded) => Err(not_installed(&ctx.label)),
            Err(err) => Err(err.into()),
        }
    }

    fn level(&self) -> ServiceLevel {
        match self.domain {
            ServiceDomain::System => ServiceLevel::System,
            ServiceDomain::User(_) | ServiceDomain::Gui(_) => ServiceLevel::User,
        }
    }

    fn set_level(&mut self, level: ServiceLevel) -> io::Result<()> {
        self.domain = match level {
            ServiceLevel::System => ServiceDomain::System,
            ServiceLevel::User => ServiceDomain::Gui(getuid()),
        };

        Ok(())
    }

    fn status(&self, ctx: crate::ServiceStatusCtx) -> io::Result<crate::ServiceStatus> {
        self.target(&ctx.label).status()
    }
}

/// Whether a plist sets `Disabled`, which launchd honours by refusing to load it.
///
/// A plist that does not parse counts as not disabled, and bootstrap reports it.
fn sets_disabled(plist: &[u8]) -> bool {
    Value::from_reader(io::Cursor::new(plist))
        .ok()
        .and_then(Value::into_dictionary)
        .and_then(|dict| dict.get("Disabled").and_then(Value::as_boolean))
        .unwrap_or(false)
}

/// Removes `Disabled = true` from the plist at `path`, so that bootstrap loads it.
fn lift_disabled(path: &Path) -> io::Result<()> {
    let mut plist = Value::from_reader(io::Cursor::new(std::fs::read(path)?))
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let Some(dict) = plist.as_dictionary_mut() else {
        return Ok(());
    };
    if dict.get("Disabled").and_then(Value::as_boolean) != Some(true) {
        return Ok(());
    }
    dict.remove("Disabled");

    let mut buffer = Vec::new();
    plist.to_writer_xml(&mut buffer).map_err(io::Error::other)?;
    // Not created afresh, so a plist a concurrent uninstall() removed stays removed.
    let mut file = OpenOptions::new().write(true).truncate(true).open(path)?;
    file.write_all(&buffer)?;
    file.sync_all()
}

fn not_installed(label: &ServiceLabel) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("Service {} is not installed", label.to_qualified_name()),
    )
}

fn make_plist<'a>(
    config: &LaunchdInstallConfig,
    label: &str,
    args: impl Iterator<Item = &'a OsStr>,
    username: Option<String>,
    working_directory: Option<PathBuf>,
    environment: Option<Vec<(String, String)>>,
    autostart: bool,
    restart_policy: RestartPolicy,
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

    // Handle restart configuration
    // Priority: launchd-specific config > generic RestartPolicy
    if let Some(keep_alive) = config.keep_alive {
        // Use launchd-specific keep_alive configuration
        if keep_alive {
            dict.insert("KeepAlive".to_string(), Value::Boolean(true));
        }
    } else {
        // Fall back to generic RestartPolicy
        // Convert generic `RestartPolicy` to Launchd `KeepAlive`.
        //
        // Right now we are only supporting binary restart for Launchd, with `KeepAlive` either set or
        // not.
        //
        // However, Launchd does support further options when `KeepAlive` is set, e.g.,
        // `SuccessfulExit`. These could be extensions for the future.
        match restart_policy {
            RestartPolicy::Never => {
                // Don't set KeepAlive
            }
            RestartPolicy::Always { delay_secs } => {
                // KeepAlive *without* the SuccessfulExit construct will keep the service alive
                // whether the process exits successfully or not.
                dict.insert("KeepAlive".to_string(), Value::Boolean(true));
                if delay_secs.is_some() {
                    log::warn!(
                        "Launchd does not support restart delays; delay_secs will be ignored for service '{}'",
                        label
                    );
                }
            }
            RestartPolicy::OnFailure {
                delay_secs,
                max_retries: _,      // TODO: no direct launchd equivalent
                reset_after_secs: _, // TODO: no direct launchd equivalent
            } => {
                // Create KeepAlive dictionary with SuccessfulExit=false
                // This means: restart when exit is NOT successful
                let mut keep_alive_dict = Dictionary::new();
                keep_alive_dict.insert("SuccessfulExit".to_string(), Value::Boolean(false));
                dict.insert("KeepAlive".to_string(), Value::Dictionary(keep_alive_dict));

                if delay_secs.is_some() {
                    log::warn!(
                        "Launchd does not support restart delays; delay_secs will be ignored for service '{}'",
                        label
                    );
                }
            }
            RestartPolicy::OnSuccess { delay_secs } => {
                // Create KeepAlive dictionary with SuccessfulExit=true
                // This means: restart when exit is successful
                let mut keep_alive_dict = Dictionary::new();
                keep_alive_dict.insert("SuccessfulExit".to_string(), Value::Boolean(true));
                dict.insert("KeepAlive".to_string(), Value::Dictionary(keep_alive_dict));

                if delay_secs.is_some() {
                    log::warn!(
                        "Launchd does not support restart delays; delay_secs will be ignored for service '{}'",
                        label
                    );
                }
            }
        }
    }

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

    if autostart {
        dict.insert("RunAtLoad".to_string(), Value::Boolean(true));
    } else {
        dict.insert("RunAtLoad".to_string(), Value::Boolean(false));
    }

    let has_keep_alive = if let Some(keep_alive) = config.keep_alive {
        keep_alive
    } else {
        !matches!(restart_policy, RestartPolicy::Never)
    };

    // Set Disabled key to prevent the service automatically starting on load when KeepAlive is present.
    // This provides consistent cross-platform behaviour which is much more intuitive.
    // The service must be explicitly started via start().
    if has_keep_alive {
        dict.insert("Disabled".to_string(), Value::Boolean(true));
    }

    let plist = Value::Dictionary(dict);

    let mut buffer = Vec::new();
    plist.to_writer_xml(&mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;

    fn generated_plist(restart_policy: RestartPolicy) -> String {
        make_plist(
            &LaunchdInstallConfig::default(),
            "com.example.app",
            [OsStr::new("/bin/sleep")].into_iter(),
            None,
            None,
            None,
            true,
            restart_policy,
        )
    }

    #[test]
    fn generated_plist_is_disabled_only_for_keep_alive_services() {
        assert!(sets_disabled(
            generated_plist(RestartPolicy::default()).as_bytes()
        ));
        assert!(!sets_disabled(
            generated_plist(RestartPolicy::Never).as_bytes()
        ));
    }

    #[test]
    fn lift_disabled_rewrites_only_a_disabled_plist() {
        let dir = assert_fs::TempDir::new().unwrap();

        let disabled = dir.child("disabled.plist");
        disabled
            .write_str(&generated_plist(RestartPolicy::default()))
            .unwrap();
        lift_disabled(disabled.path()).unwrap();
        let lifted = std::fs::read(disabled.path()).unwrap();
        assert!(!sets_disabled(&lifted));
        assert!(String::from_utf8(lifted).unwrap().contains("KeepAlive"));

        let enabled = dir.child("enabled.plist");
        let contents = generated_plist(RestartPolicy::Never);
        enabled.write_str(&contents).unwrap();
        lift_disabled(enabled.path()).unwrap();
        enabled.assert(contents.as_str());

        let missing = lift_disabled(&dir.path().join("missing.plist")).unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
    }
}
