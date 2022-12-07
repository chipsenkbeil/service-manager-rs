use std::io;
use cfg_if::cfg_if;

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
    pub fn native() -> io::Result<ServiceManagerKind> {
        cfg_if! {
            if #[cfg(target_os = "macos")] {
                Ok(ServiceManagerKind::Launchd)
            } else if #[cfg(target_os = "windows")] {
                Ok(ServiceManagerKind::Sc)
            } else if #[cfg(any(
                target_os = "freebsd",
                target_os = "dragonfly",
                target_os = "openbsd",
                target_os = "netbsd"
            ))] {
                Ok(ServiceManagerKind::Rcd)
            } else if #[cfg(target_os = "linux")] {
                use super::{ServiceManager, TypedServiceManager};
    
                let manager = TypedServiceManager::target(ServiceManagerKind::Systemd);
                if let Ok(true) = manager.available() {
                    return Ok(ServiceManagerKind::Systemd);
                }
        
                let manager = TypedServiceManager::target(ServiceManagerKind::OpenRc);
                if let Ok(true) = manager.available() {
                    return Ok(ServiceManagerKind::OpenRc);
                }
        
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Only systemd and openrc are supported on Linux",
                ))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Service manager are not supported on current Operating System!",
                ))
            }
        }
    }
    
}



