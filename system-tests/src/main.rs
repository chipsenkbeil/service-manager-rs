use clap::{Parser, Subcommand};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    thread,
};

#[derive(Clone)]
struct Logger {
    file: Option<PathBuf>,
}

impl Logger {
    pub fn new(file: Option<PathBuf>) -> Self {
        Self { file }
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
    },
}

impl Cli {
    #[cfg(windows)]
    pub fn is_listen_action(&self) -> bool {
        matches!(self.action, Action::Listen { .. })
    }

    /// Runs CLI to completion
    pub fn run(self) -> io::Result<()> {
        match self.action {
            Action::Talk { addr, msg } => {
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

                println!("{}", String::from_utf8_lossy(&bytes));
                Ok(())
            }

            Action::Listen { addr, log_file } => {
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
    if cli.is_listen_action() {
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
    use super::{Cli, Parser};
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
            quick_xml::se::to_writer(&mut bytes, self)
                .map_err(|x| io::Error::new(io::ErrorKind::Other, x))?;
            std::fs::write(Self::config_file(), bytes)
        }

        pub fn load() -> io::Result<Self> {
            let bytes = std::fs::read(Self::config_file())?;
            quick_xml::de::from_slice(&bytes).map_err(|x| io::Error::new(io::ErrorKind::Other, x))
        }

        fn config_file() -> std::path::PathBuf {
            let mut path = std::env::current_exe().unwrap();
            path.set_extension("exe.config");
            std::env::temp_dir().join(path.file_name().unwrap())
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
        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

        // Tell the system that service is running
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
        thread::spawn(move || {
            Cli::try_parse_from(Config::load().unwrap().args)
                .unwrap()
                .run()
                .unwrap();
            shutdown_tx.send(()).unwrap();
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
