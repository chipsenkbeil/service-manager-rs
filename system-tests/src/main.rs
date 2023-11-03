use clap::{Parser, Subcommand};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

/// Timeout for talking to a server
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct Logger {
    file: Option<PathBuf>,
}

impl Logger {
    pub fn new(file: impl Into<Option<PathBuf>>) -> Self {
        Self { file: file.into() }
    }

    /// Stored next to the service exe
    #[cfg(windows)]
    pub fn file() -> Self {
        let mut path = std::env::current_exe().unwrap();
        path.set_extension("exe.log");
        Self::new(path)
    }

    pub fn log(&self, s: impl AsRef<str>) {
        match self.file.as_deref() {
            Some(file) => {
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(file)
                    .unwrap();
                f.write_all(s.as_ref().as_bytes()).unwrap();
                f.write_all(b"\n").unwrap();
                f.flush().unwrap();
            }
            None => eprintln!("{}", s.as_ref()),
        }
    }
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Sends a message to the server
    Talk {
        /// Address to bind to over TCP
        addr: SocketAddr,

        /// Message to echo over TCP
        msg: String,
    },

    /// Listens for a connection and echoes back anything received
    Listen {
        /// Optional file to write output instead of stderr
        #[clap(long)]
        log_file: Option<PathBuf>,

        /// Address to bind to over TCP
        addr: SocketAddr,

        /// Set to true to run as a Windows service using sc.exe
        #[clap(long)]
        run_as_windows_service: bool,
    },
}

impl Cli {
    #[cfg(windows)]
    pub fn is_listen_action(&self) -> bool {
        matches!(self.action, Action::Listen { .. })
    }

    #[cfg(windows)]
    pub fn run_as_windows_service(&self) -> bool {
        match self.action {
            Action::Listen {
                run_as_windows_service,
                ..
            } => run_as_windows_service,
            _ => false,
        }
    }

    /// Runs CLI to completion
    pub fn run(self) -> io::Result<()> {
        match self.action {
            Action::Talk { addr, msg } => {
                let handle: thread::JoinHandle<io::Result<Vec<u8>>> = thread::spawn(move || {
                    let mut stream = TcpStream::connect(addr)?;
                    stream.write_all(msg.as_bytes())?;

                    let mut remaining = msg.len();
                    let mut bytes = Vec::new();
                    let mut buf = [0u8; 128];
                    loop {
                        match stream.read(&mut buf) {
                            Ok(n) if n > 0 => {
                                bytes.extend(&buf[..n]);
                                if remaining <= n {
                                    break;
                                } else {
                                    remaining -= n;
                                }
                            }
                            Ok(_) => {
                                eprintln!("Connection {addr} closed unexpectedly");
                                break;
                            }
                            Err(x) => eprintln!("Connection {addr} terminated: {x}"),
                        }
                    }

                    Ok(bytes)
                });

                let start = Instant::now();
                while start.elapsed() < TIMEOUT {
                    if handle.is_finished() {
                        let bytes = handle.join().unwrap()?;
                        println!("{}", String::from_utf8_lossy(&bytes));
                        return Ok(());
                    }

                    thread::sleep(Duration::from_millis(100));
                }

                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("No response received in {}s", TIMEOUT.as_secs()),
                ))
            }

            Action::Listen { addr, log_file, .. } => {
                let logger = Logger::new(log_file);
                let handle = thread::spawn(move || {
                    let listener = TcpListener::bind(addr)?;

                    let addr = listener.local_addr()?;
                    logger.log(format!("Listening on {addr}"));

                    let mut connections = Vec::new();
                    while let Ok((mut stream, addr)) = listener.accept() {
                        let logger = logger.clone();
                        logger.log(format!("New connection {addr}"));
                        connections.push(thread::spawn(move || {
                            let mut buf = [0u8; 128];
                            loop {
                                match stream.read(&mut buf) {
                                    Ok(n) if n > 0 => {
                                        if let Err(x) = stream.write_all(&buf[..n]) {
                                            logger.log(format!(
                                                "Connection {addr} failed to write: {x}"
                                            ));
                                            break;
                                        }
                                    }
                                    Ok(_) => {
                                        logger.log(format!("Connection {addr} closed"));
                                        break;
                                    }
                                    Err(x) => {
                                        logger.log(format!("Connection {addr} terminated: {x}"))
                                    }
                                }
                            }
                        }));
                    }

                    Ok(())
                });

                handle
                    .join()
                    .map_err(|_| io::Error::new(io::ErrorKind::Other, "Thread join failed"))?
            }
        }
    }
}

#[cfg(unix)]
fn main() -> io::Result<()> {
    Cli::parse().run()
}

#[cfg(windows)]
fn main() -> io::Result<()> {
    let cli = Cli::parse();
    // We are either running the listener service through sc.exe or winsw.exe. For the former, the
    // 'run as service' flag should be set to indicate that we need to use the boilerplate service
    // infrastructure code. For the latter, the boilerplate is provided for you and therefore we
    // must run without it.
    if cli.is_listen_action() && cli.run_as_windows_service() {
        // Save a config for use by the service
        echo_service::Config {
            args: std::env::args_os().collect(),
        }
        .save()?;

        echo_service::run()
    } else {
        cli.run()
    }
}

#[cfg(windows)]
mod echo_service {
    use super::{Cli, Logger, Parser};
    use std::{ffi::OsString, io, sync::mpsc, thread, time::Duration};
    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher, Result,
    };

    const SERVICE_NAME: &str = "service_manager_system_test_echo_service";
    const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

    #[derive(serde::Serialize, serde::Deserialize)]
    pub struct Config {
        pub args: Vec<std::ffi::OsString>,
    }

    impl Config {
        pub fn save(&self) -> io::Result<()> {
            let mut bytes = Vec::new();
            serde_json::to_writer(&mut bytes, self)
                .map_err(|x| io::Error::new(io::ErrorKind::Other, x))?;
            std::fs::write(Self::config_file(), bytes)
        }

        pub fn load() -> io::Result<Self> {
            let bytes = std::fs::read(Self::config_file())?;
            serde_json::from_slice(&bytes).map_err(|x| io::Error::new(io::ErrorKind::Other, x))
        }

        /// Stored next to the service exe
        fn config_file() -> std::path::PathBuf {
            let mut path = std::env::current_exe().unwrap();
            path.set_extension("exe.config");
            path
        }
    }

    pub fn run() -> io::Result<()> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)
            .map_err(|x| io::Error::new(io::ErrorKind::Other, x))
    }

    define_windows_service!(ffi_service_main, echo_service_main);

    pub fn echo_service_main(_arguments: Vec<OsString>) {
        if let Err(_e) = run_service() {
            // Handle the error, by logging or something.
        }
    }

    fn run_service() -> Result<()> {
        let logger = Logger::file();
        logger.log("Starting windows service for {SERVICE_NAME}");

        // Create a channel to be able to poll a stop event from the service worker loop.
        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

        // Define system service event handler that will be receiving service events.
        let event_handler = {
            let shutdown_tx = shutdown_tx.clone();
            move |control_event| -> ServiceControlHandlerResult {
                match control_event {
                    // Notifies a service to report its current status information to the service
                    // control manager. Always return NoError even if not implemented.
                    ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,

                    // Handle stop
                    ServiceControl::Stop => {
                        shutdown_tx.send(()).unwrap();
                        ServiceControlHandlerResult::NoError
                    }

                    _ => ServiceControlHandlerResult::NotImplemented,
                }
            }
        };

        // Register system service event handler.
        // The returned status handle should be used to report service status changes to the system.
        logger.log(format!(
            "Registering service control handler for {SERVICE_NAME}"
        ));
        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

        // Tell the system that service is running
        logger.log(format!(
            "Setting service status as running for {SERVICE_NAME}"
        ));
        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        // Kick off thread to run our cli
        logger.log(format!("Spawning CLI thread for {SERVICE_NAME}"));
        thread::spawn({
            let logger = logger.clone();
            move || {
                logger.log(format!(
                    "Loading CLI using args from disk for {SERVICE_NAME}"
                ));
                let config = match Config::load() {
                    Ok(config) => config,
                    Err(x) => {
                        logger.log(format!("[ERROR] {x}"));
                        panic!("{x}");
                    }
                };

                logger.log(format!("Parsing CLI args from disk for {SERVICE_NAME}"));
                let cli = match Cli::try_parse_from(config.args) {
                    Ok(cli) => cli,
                    Err(x) => {
                        logger.log(format!("[ERROR] {x}"));
                        panic!("{x}");
                    }
                };

                logger.log(format!("Running CLI for {SERVICE_NAME}"));
                if let Err(x) = cli.run() {
                    logger.log(format!("[ERROR] {x}"));
                }

                shutdown_tx.send(()).unwrap();
            }
        });
        loop {
            match shutdown_rx.recv_timeout(Duration::from_millis(100)) {
                // Break the loop either upon stop or channel disconnect
                Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => break,

                // Continue work if no events were received within the timeout
                Err(mpsc::RecvTimeoutError::Timeout) => (),
            };
        }

        // Tell the system that service has stopped.
        logger.log(format!(
            "Setting service status as stopped for {SERVICE_NAME}"
        ));
        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        Ok(())
    }
}
