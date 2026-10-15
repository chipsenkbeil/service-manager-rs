use super::*;
use std::str::Utf8Error;

/// Core interface to interact with `launchctl`.
pub(super) struct LaunchCtl {
    cmd: Command,
}

/// Errors thrown when trying to boot out a service.
#[derive(Debug)]
pub enum LaunchCtlBootoutError {
    /// There was a timeout attempting to boot out the service.
    Timeout,
    /// There was an unexpected IO error booting out the service.
    Io(std::io::Error),
    /// The service's info could not be read.
    Print(ServicePrintoutError),
    /// The service was not loaded, so there was nothing to boot out.
    NotLoaded,
}

impl Error for LaunchCtlBootoutError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LaunchCtlBootoutError::Timeout | LaunchCtlBootoutError::NotLoaded => None,
            LaunchCtlBootoutError::Io(io) => Some(io),
            LaunchCtlBootoutError::Print(err) => Some(err),
        }
    }
}

impl Display for LaunchCtlBootoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LaunchCtlBootoutError::Timeout => write!(f, "unexpected timeout waiting for bootout"),
            LaunchCtlBootoutError::Io(_) => write!(f, "unexpected io error"),
            LaunchCtlBootoutError::Print(_) => write!(f, "could not read the service's info"),
            LaunchCtlBootoutError::NotLoaded => write!(f, "the service is not loaded"),
        }
    }
}

impl From<io::Error> for LaunchCtlBootoutError {
    fn from(err: io::Error) -> Self {
        LaunchCtlBootoutError::Io(err)
    }
}

impl From<LaunchCtlWaitError> for LaunchCtlBootoutError {
    fn from(err: LaunchCtlWaitError) -> Self {
        match err {
            LaunchCtlWaitError::Timeout => LaunchCtlBootoutError::Timeout,
            LaunchCtlWaitError::Print(err) => LaunchCtlBootoutError::Print(err),
        }
    }
}

impl From<LaunchCtlBootoutError> for io::Error {
    fn from(err: LaunchCtlBootoutError) -> Self {
        let kind = match &err {
            LaunchCtlBootoutError::Timeout => io::ErrorKind::TimedOut,
            LaunchCtlBootoutError::Io(io) => io.kind(),
            LaunchCtlBootoutError::Print(print) => print.kind(),
            LaunchCtlBootoutError::NotLoaded => io::ErrorKind::NotFound,
        };

        io::Error::new(kind, err)
    }
}

/// Errors thrown when waiting on a service with [`LaunchCtl::wait_until`].
#[derive(Debug)]
pub enum LaunchCtlWaitError {
    /// The service was still loaded and not done when the timeout passed.
    Timeout,
    /// The service's info could not be read.
    Print(ServicePrintoutError),
}

impl Error for LaunchCtlWaitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LaunchCtlWaitError::Timeout => None,
            LaunchCtlWaitError::Print(err) => Some(err),
        }
    }
}

impl Display for LaunchCtlWaitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LaunchCtlWaitError::Timeout => write!(f, "timed out waiting for the service"),
            LaunchCtlWaitError::Print(_) => write!(f, "could not read the service's info"),
        }
    }
}

impl From<ServicePrintoutError> for LaunchCtlWaitError {
    fn from(err: ServicePrintoutError) -> Self {
        LaunchCtlWaitError::Print(err)
    }
}

impl From<LaunchCtlWaitError> for io::Error {
    fn from(err: LaunchCtlWaitError) -> Self {
        let kind = match &err {
            LaunchCtlWaitError::Timeout => io::ErrorKind::TimedOut,
            LaunchCtlWaitError::Print(print) => print.kind(),
        };

        io::Error::new(kind, err)
    }
}

/// Represents the state of the launchctl service.
///
/// These are all exposed by `launchctl print`.
pub enum ServiceState {
    Spawning,
    XpcProxy,
    Running,
    Sigtermed,
    Sigkilled,
    Other(String),
}

impl ServiceState {
    /// Whether the service is currently booting up or not.
    ///
    /// After the service has booted up and is running, it will no longer return true for this.
    pub fn is_booting(&self) -> bool {
        matches!(self, ServiceState::Spawning | ServiceState::XpcProxy)
    }

    /// Whether the job has a process, counting one launchd has forked and is still exec'ing.
    ///
    /// Returns true either when the service is [`Self::is_booting`] or it's running.
    pub fn is_running(&self) -> bool {
        matches!(
            self,
            ServiceState::Running | ServiceState::Spawning | ServiceState::XpcProxy
        )
    }

    /// Whether launchd has signalled the job to exit and has not reaped it yet.
    pub fn is_stopping(&self) -> bool {
        matches!(self, ServiceState::Sigtermed | ServiceState::Sigkilled)
    }
}

impl FromStr for ServiceState {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "spawning" => ServiceState::Spawning,
            "xpcproxy" => ServiceState::XpcProxy,
            "running" => ServiceState::Running,
            "SIGTERMed" => ServiceState::Sigtermed,
            "SIGKILLed" => ServiceState::Sigkilled,
            other => ServiceState::Other(other.into()),
        })
    }
}

/// The output of the `launchctl print` command.
pub struct ServiceInfo {
    /// The currently reported state of the service.
    pub state: ServiceState,
    /// How many runs the service had.
    pub runs: u64,
    /// How long launchd waits after SIGTERM before it SIGKILLs the job, if set.
    ///
    /// Note this can be infinite, in which case it is [`Option::None`].
    pub exit_timeout: Option<Duration>,
}

/// Why the output of `launchctl print <service-target>` could not be read.
#[derive(Debug)]
pub enum ServicePrintoutParseError {
    /// The output is not a UTF8 byte array.
    NotUtf8(Utf8Error),
    /// The service's dictionary has no such field.
    MissingField(&'static str),
    /// The field appears more than once in the service's own dictionary.
    ///
    /// Likely a parsing error, and you should probably report this.
    DuplicateField(&'static str),
    /// Lines follow the service's closing brace, so a printed value closed it early.
    ///
    /// This likely means something was misparsed and you should consider opening an issue.
    TrailingLines,
    /// The field's value could not be parsed.
    ParsingError {
        field: &'static str,
        source: Box<dyn Error + Send + Sync>,
    },
}

impl Error for ServicePrintoutParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ServicePrintoutParseError::NotUtf8(err) => Some(err),
            ServicePrintoutParseError::ParsingError { source, .. } => Some(source.as_ref()),
            ServicePrintoutParseError::MissingField(_)
            | ServicePrintoutParseError::DuplicateField(_)
            | ServicePrintoutParseError::TrailingLines => None,
        }
    }
}

impl Display for ServicePrintoutParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServicePrintoutParseError::NotUtf8(err) => {
                write!(f, "print output is not UTF-8: {err}")
            }
            ServicePrintoutParseError::MissingField(field) => {
                write!(f, "print output has no `{field}` field")
            }
            ServicePrintoutParseError::DuplicateField(field) => {
                write!(f, "print output has more than one `{field}` field")
            }
            ServicePrintoutParseError::TrailingLines => {
                write!(f, "trailing lines detected. likely misparsed")
            }
            ServicePrintoutParseError::ParsingError { field, .. } => {
                write!(f, "print field `{field}` could not be parsed")
            }
        }
    }
}

impl From<Utf8Error> for ServicePrintoutParseError {
    fn from(err: Utf8Error) -> Self {
        ServicePrintoutParseError::NotUtf8(err)
    }
}

/// Errors reading a service's state with `launchctl print`.
#[derive(Debug)]
pub enum ServicePrintoutError {
    /// `launchctl print` could not be run, or failed for a reason other than the service or its
    /// domain not existing.
    Io(io::Error),
    /// The service target's domain does not exist, such as `gui/<uid>` for a user with no
    /// graphical login.
    DomainNotFound(String),
    /// `launchctl print` succeeded, but its output could not be parsed.
    Parse(ServicePrintoutParseError),
}

impl Error for ServicePrintoutError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ServicePrintoutError::Io(err) => Some(err),
            ServicePrintoutError::Parse(err) => Some(err),
            ServicePrintoutError::DomainNotFound(_) => None,
        }
    }
}

impl Display for ServicePrintoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServicePrintoutError::Io(_) => write!(f, "launchctl print failed"),
            ServicePrintoutError::Parse(_) => {
                write!(f, "launchctl print output could not be parsed")
            }
            ServicePrintoutError::DomainNotFound(service_target) => {
                write!(f, "launchctl domain of `{service_target}` does not exist")
            }
        }
    }
}

impl From<io::Error> for ServicePrintoutError {
    fn from(err: io::Error) -> Self {
        ServicePrintoutError::Io(err)
    }
}

impl From<ServicePrintoutParseError> for ServicePrintoutError {
    fn from(err: ServicePrintoutParseError) -> Self {
        ServicePrintoutError::Parse(err)
    }
}

impl ServicePrintoutError {
    /// The `io::ErrorKind` this error becomes when converted to an `io::Error`.
    fn kind(&self) -> io::ErrorKind {
        match self {
            ServicePrintoutError::Io(io) => io.kind(),
            ServicePrintoutError::DomainNotFound(_) => io::ErrorKind::NotFound,
            ServicePrintoutError::Parse(_) => io::ErrorKind::InvalidData,
        }
    }
}

impl From<ServicePrintoutError> for io::Error {
    fn from(err: ServicePrintoutError) -> Self {
        io::Error::new(err.kind(), err)
    }
}

impl ServiceInfo {
    /// How long launchd can take to tear the job down once it has been signalled.
    pub fn teardown_timeout(&self) -> Duration {
        // launchd SIGKILLs a job still running ExitTimeOut after its SIGTERM, then needs a
        // moment to reap it. launchd.plist(5) calls an ExitTimeOut of 0 infinite, so that wait
        // gets a fixed bound.
        const REAP_MARGIN: Duration = Duration::from_secs(2);
        const NO_EXIT_TIMEOUT_BOUND: Duration = Duration::from_secs(10);

        self.exit_timeout.unwrap_or(NO_EXIT_TIMEOUT_BOUND) + REAP_MARGIN
    }

    /// The `key = value` lines of the service's dictionary, in printed order.
    fn own_fields(
        printout: &str,
    ) -> impl Iterator<Item = Result<(&str, &str), ServicePrintoutParseError>> + '_ {
        let mut lines = printout.lines();
        let mut depth = 0;
        std::iter::from_fn(move || loop {
            let line = lines.next()?;
            let text = line.trim_start_matches('\t');
            let indent = line.len() - text.len();
            if text == "}" && indent + 1 == depth {
                depth -= 1;
                if depth > 0 {
                    continue;
                }
                // The service's closing brace is the last line, unless a value forged it. Either
                // way nothing after it is a field.
                let trailing = lines.any(|line| !line.is_empty());
                lines = "".lines();
                return trailing.then_some(Err(ServicePrintoutParseError::TrailingLines));
            }
            if indent != depth {
                continue;
            }
            if text.ends_with(" = {") {
                depth += 1;
                continue;
            }
            if depth != 1 {
                continue;
            }
            if let Some(field) = text.split_once(" = ") {
                return Some(Ok(field));
            }
        })
    }

    /// The service's own `key` field, parsed as `T`.
    fn field<T>(printout: &str, key: &'static str) -> Result<T, ServicePrintoutParseError>
    where
        T: FromStr,
        T::Err: Error + Send + Sync + 'static,
    {
        let mut found = None;
        for field in Self::own_fields(printout) {
            let (name, value) = field?;
            if name != key {
                continue;
            }

            if found.replace(value).is_some() {
                return Err(ServicePrintoutParseError::DuplicateField(key));
            }
        }

        found
            .ok_or(ServicePrintoutParseError::MissingField(key))?
            .parse()
            .map_err(|source| ServicePrintoutParseError::ParsingError {
                field: key,
                source: Box::new(source),
            })
    }
}

impl TryFrom<&[u8]> for ServiceInfo {
    type Error = ServicePrintoutParseError;

    /// Try to parse the output of `launchctl print`.
    ///
    /// Please note `launchctl print`'s output is not stable and Apple **EXPLICITLY** says it's not
    /// stable but we have no other option.
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        // Apple's print format mixes `key = value` fields, nested `key = {` dictionaries,
        // `KEY => value` environment entries and bare lines. We need three fields of the
        // service's own dictionary:
        //
        // gui/501/2BUA8C4S2C.com.1password.browser-helper = {
        //         state = running
        //         ...
        //         exit timeout = 5
        //         runs = 1
        //         ...
        // }
        let printout = std::str::from_utf8(value)?;

        let state: ServiceState = Self::field(printout, "state")?;
        let runs: u64 = Self::field(printout, "runs")?;
        let exit_timeout = match Self::field::<u64>(printout, "exit timeout")? {
            0 => None,
            secs => Some(Duration::from_secs(secs)),
        };

        Ok(Self {
            state,
            runs,
            exit_timeout,
        })
    }
}

impl LaunchCtl {
    const LAUNCHCTL_PATH: &'static str = "/bin/launchctl";

    /// `launchctl` exit codes for a service that is not loaded: `ESRCH` from `bootout`, then
    /// "Could not find specified domain" and "Could not find specified service".
    pub const NOT_LOADED: [i32; 3] = [3, 112, 113];

    /// `launchctl` exit code for "Could not find specified domain", such as `gui/<uid>` for a user
    /// with no graphical login.
    pub const DOMAIN_NOT_FOUND: i32 = 112;

    /// `launchctl` exit code for `EPERM`, which a non-root caller gets for the `system` domain.
    pub const EPERM: i32 = 1;

    /// `launchctl bootout` exit code for `EINPROGRESS`, returned while the job is still exiting.
    pub const EINPROGRESS: i32 = 36;

    pub fn available() -> bool {
        Path::new(Self::LAUNCHCTL_PATH).exists()
    }

    /// `launchctl bootstrap [service-path service-path2 ...]`
    pub fn bootstrap_plists(
        domain: &str,
        plists: impl Iterator<Item = impl AsRef<OsStr>>,
    ) -> io::Result<process::Output> {
        Self::start()
            .arg("bootstrap")
            .arg(domain)
            .args(plists)
            .output()
    }

    /// `launchctl bootout service-target`.
    ///
    /// Waits for the launchd to finish tearing out the service. Please note this function can
    /// block the active thread until the requested service is booted out.
    ///
    /// Fails with [`LaunchCtlBootoutError::NotLoaded`] when the service is not loaded.
    pub fn bootout(service_target: &str, timeout: Duration) -> Result<(), LaunchCtlBootoutError> {
        let procedure_start_time = Instant::now();

        const BOOTUP_TIMEOUT: Duration = Duration::from_secs(1);

        // A process may be in progress of being booted in. We have to make sure it's fully loaded
        // before we try to bootout otherwise bootout will yell at us.
        let status = match Self::wait_until(service_target, BOOTUP_TIMEOUT.min(timeout), |info| {
            !info.state.is_booting()
        }) {
            Ok(Some(status)) => status,
            // A domain that does not exist, such as gui/<uid> with no graphical login, has no jobs.
            Ok(None) | Err(LaunchCtlWaitError::Print(ServicePrintoutError::DomainNotFound(_))) => {
                return Err(LaunchCtlBootoutError::NotLoaded);
            }
            Err(err) => return Err(err.into()),
        };

        // Well now we now that the service is not in the booting up state, we can try to boot it
        // out.
        //
        // We call output_raw here since launchctl will return non-zero exit codes and
        // [`Self::output`] would abort on those.
        let mut bootout = Self::start().arg("bootout").arg(service_target);
        let output = bootout.output_raw()?;
        match output.status.code() {
            // The service left its domain since it was read.
            Some(code) if Self::NOT_LOADED.contains(&code) => {
                return Err(LaunchCtlBootoutError::NotLoaded);
            }
            // launchd has signalled the job and is still waiting for it to exit.
            Some(Self::EINPROGRESS) => {}
            _ => {
                bootout.map_exit_status(output)?;
            }
        }

        // We then have to block until we can confirm that the service actually goes down. Only
        // leaving its domain ends this wait, so no info counts as done.
        let remaining = timeout.saturating_sub(procedure_start_time.elapsed());
        Self::wait_until(
            service_target,
            status.teardown_timeout().min(remaining),
            |_| false,
        )?;

        Ok(())
    }

    /// `launchctl kickstart service-target`
    pub fn kickstart(service_target: &str) -> io::Result<process::Output> {
        Self::start().arg("kickstart").arg(service_target).output()
    }

    /// `launchctl print service-target`
    ///
    /// Returns `None` when the service is not loaded.
    ///
    /// **Warning**: Apple explicitly documents this API as not being stable and safe to use, but
    /// unfortunately, I couldn't find a different, more appropriate API. We use this API, but we
    /// keep the surface of what we read very minimal.
    pub fn print_service(
        service_target: &str,
    ) -> Result<Option<ServiceInfo>, ServicePrintoutError> {
        let output = Self::start()
            .arg("print")
            .arg(service_target)
            .output_raw()?;
        match output.status.code() {
            Some(Self::DOMAIN_NOT_FOUND) => {
                return Err(ServicePrintoutError::DomainNotFound(
                    service_target.to_owned(),
                ));
            }
            Some(code) if Self::NOT_LOADED.contains(&code) => return Ok(None),
            _ => {}
        }

        let output = wrap_output(output)?;
        Ok(Some(ServiceInfo::try_from(output.stdout.as_slice())?))
    }

    /// Polls the service's info until `done` holds or the service is not loaded, failing once
    /// `timeout` passes.
    ///
    /// Returns the last info read, `None` meaning the service is not loaded.
    pub fn wait_until(
        service_target: &str,
        timeout: Duration,
        done: impl Fn(&ServiceInfo) -> bool,
    ) -> Result<Option<ServiceInfo>, LaunchCtlWaitError> {
        const POLL_INTERVAL: Duration = Duration::from_millis(100);

        let started = Instant::now();
        loop {
            let info = Self::print_service(service_target)?;
            if info.as_ref().map_or(true, &done) {
                return Ok(info);
            }
            if started.elapsed() >= timeout {
                return Err(LaunchCtlWaitError::Timeout);
            }

            thread::sleep(POLL_INTERVAL);
        }
    }

    fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.cmd.arg(arg);
        Self { ..self }
    }

    fn args(mut self, args: impl Iterator<Item = impl AsRef<OsStr>>) -> Self {
        self.cmd.args(args);
        Self { ..self }
    }

    fn output_raw(&mut self) -> io::Result<std::process::Output> {
        self.cmd.output()
    }

    /// Run a command, parsing the exit code.
    fn output(mut self) -> std::io::Result<std::process::Output> {
        let output = self.output_raw()?;
        self.map_exit_status(output)
    }

    /// Maps a non-zero exit to an `io::Error` whose kind follows the exit code and whose message
    /// names the command; a zero exit passes `output` through.
    fn map_exit_status(&self, output: Output) -> io::Result<Output> {
        let kind = match output.status.code() {
            Some(Self::EPERM) => io::ErrorKind::PermissionDenied,
            Some(code) if Self::NOT_LOADED.contains(&code) => io::ErrorKind::NotFound,
            _ => io::ErrorKind::Other,
        };

        wrap_output(output).map_err(|err| {
            let args: Vec<_> = self.cmd.get_args().map(OsStr::to_string_lossy).collect();
            io::Error::new(kind, format!("launchctl {}: {err}", args.join(" ")))
        })
    }

    fn start() -> Self {
        let mut cmd = Command::new(Self::LAUNCHCTL_PATH);

        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Self { cmd }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses `printout` as the output of `launchctl print <service-target>`.
    #[track_caller]
    fn parse(printout: &str) -> ServiceInfo {
        ServiceInfo::try_from(printout.as_bytes()).expect("the printout is well formed")
    }

    #[test]
    fn service_info_ignores_fields_of_nested_dictionaries() {
        let info = parse(
            "gui/501/com.example.app = {\n\
             \tstate = not running\n\
             \texit timeout = 40\n\
             \tresource coalition = {\n\
             \t\tstate = running\n\
             \t\truns = 7\n\
             \t}\n\
             \truns = 0\n\
             }\n",
        );

        assert!(!info.state.is_running());
        assert_eq!(info.runs, 0);
        assert_eq!(info.exit_timeout, Some(Duration::from_secs(40)));
    }

    #[test]
    fn service_info_ignores_fields_forged_by_an_environment_value() {
        let info = parse(
            "gui/501/com.example.app = {\n\
             \tstate = not running\n\
             \tenvironment = {\n\
             \t\tAPP_CONFIG => [retry]\n\
             \truns = 3\n\
             \t}\n\
             \texit timeout = 5\n\
             \truns = 0\n\
             }\n",
        );

        assert_eq!(info.runs, 0);
    }

    #[test]
    fn service_info_treats_a_job_being_execed_as_running() {
        let info = parse(
            "gui/501/com.example.app = {\n\
             \tstate = xpcproxy\n\
             \texit timeout = 0\n\
             \truns = 1\n\
             }\n",
        );

        assert!(info.state.is_running());
        assert_eq!(info.runs, 1);
        assert_eq!(info.exit_timeout, None);
    }
}
