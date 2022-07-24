use std::io;

/// Represents the kind of service manager
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(::clap::ValueEnum))]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
#[cfg_attr(feature = "clap", clap(rename_all = "lowercase"))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum ServiceManagerKind {
    /// Use launchd to manage the service
    Launchd,

    /// Use OpenRC to manage the service
    OpenRc,

    /// Use rc.d to manage the service
    Rcd,

    /// Use Windows service controller to manage the service
    Sc,

    /// Use systemd to manage the service
    Systemd,
}

impl ServiceManagerKind {
    /// Looks up the kind of service management platform native to the operating system
    #[cfg(target_os = "macos")]
    pub fn native() -> io::Result<Self> {
        Ok(Self::Launchd)
    }

    /// Looks up the kind of service management platform native to the operating system
    #[cfg(target_os = "windows")]
    pub fn native() -> io::Result<Self> {
        Ok(Self::Sc)
    }

    /// Looks up the kind of service management platform native to the operating system
    #[cfg(any(
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd"
    ))]
    pub fn native() -> io::Result<Self> {
        Ok(Self::Rcd)
    }

    /// Looks up the kind of service management platform native to the operating system
    #[cfg(target_os = "linux")]
    pub fn native() -> io::Result<Self> {
        use super::{ServiceManager, TypedServiceManager};

        let manager = TypedServiceManager::target(Self::Systemd);
        if let Ok(true) = manager.available() {
            return Ok(Self::Systemd);
        }

        let manager = TypedServiceManager::target(Self::OpenRc);
        if let Ok(true) = manager.available() {
            return Ok(Self::OpenRc);
        }

        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Only systemd and openrc are supported on Linux",
        ))
    }
}
