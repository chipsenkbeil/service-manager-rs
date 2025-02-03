
use super::{
    ServiceInstallCtx, ServiceLevel, ServiceManager, ServiceStartCtx, ServiceStatusCtx,
    ServiceStopCtx, ServiceUninstallCtx
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ScmConfig {
    pub install: ScmInstallConfig,
}

impl Default for ScmConfig {
    fn default() -> Self {
        ScmConfig {
            install: ScmInstallConfig {
                description: None,
                dependencies: None,
                display_name: None,
                start_type: None,
                service_type: ScmServiceType::Own,
                error_severity: ScmErrorControl::Normal,
                delay_autostart: false,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ScmInstallConfig {
    pub description: Option<String>,
    pub dependencies: Option<Vec<String>>,
    pub display_name: Option<String>,
    pub start_type: Option<ScmStartType>,
    pub service_type: ScmServiceType,
    pub error_severity: ScmErrorControl,
    pub delay_autostart: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ScmServiceType {
    Kernel = 1u32,
    FileSys = 2u32,
    Own = 16u32, 
    Share = 32u32,
    UserOwn = 80u32,
    UserShare = 96u32,
    Interactive = 256u32,
}

impl Default for ScmServiceType {
    fn default() -> Self {
        Self::Own
    }    
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ScmStartType {
    BootStart = 0u32,
    SystemStart = 1u32,
    AutoStart = 2u32,
    OnDemand = 3u32,
    Disabled = 4u32
}

impl Default for ScmStartType {
    fn default() -> Self {
        Self::OnDemand
    }
}


#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ScmErrorControl {
    Ignore = 0u32,
    Normal = 1u32,
    Severe = 2u32,
    Critical = 3u32
}

impl Default for ScmErrorControl {
    fn default() -> Self {
        Self::Normal
    }    
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct ScmServiceManager {
    pub config: ScmConfig
}



impl ScmServiceManager {
    pub fn system() -> Self {
        Self {
            config: ScmConfig::default()
        }
    }

    pub fn with_config(self, config: ScmConfig) -> Self {
        Self { config }
    }
}

impl ServiceManager for ScmServiceManager {
    fn available(&self) -> std::io::Result<bool> {
        if cfg!(target_os = "windows") {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn install(&self, ctx: ServiceInstallCtx) -> std::io::Result<()> {
        scm_handler::service_install(&ctx, &self.config.install)
    }

    fn uninstall(&self, ctx: ServiceUninstallCtx) -> std::io::Result<()> {
        scm_handler::service_uninstall(&ctx)
    }

    fn start(&self, ctx: ServiceStartCtx) -> std::io::Result<()> {
        scm_handler::service_start(&ctx)
    }

    fn stop(&self, ctx: ServiceStopCtx) -> std::io::Result<()> {
        scm_handler::service_stop(&ctx)
    }

    fn level(&self) -> ServiceLevel {
        ServiceLevel::System
    }

    fn set_level(&mut self, level: ServiceLevel) -> std::io::Result<()> {
        match level {
            ServiceLevel::System => Ok(()),
            ServiceLevel::User => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "windows Service control manager does not support user-level services",
            )),
        }
    }

    fn status(&self, ctx: crate::ServiceStatusCtx) -> std::io::Result<crate::ServiceStatus> {
        scm_handler::service_status(&ctx)
    }
}

#[cfg(target_os = "windows")]
mod scm_handler {
    use std::{
        ffi::{OsStr, OsString},
        io,
    };

    use windows_service::{
        service::{
            ServiceAccess, ServiceDependency, ServiceErrorControl, ServiceExitCode, ServiceInfo,
            ServiceStartType, ServiceState, ServiceType,
        },
        service_manager::{ServiceManager, ServiceManagerAccess},
    };

    pub fn service_install(ctx: &super::ServiceInstallCtx, install_config: &crate::ScmInstallConfig) -> std::io::Result<()> {
        let manager = get_win_service_manager()?;
        let name = ctx.label.to_qualified_name().parse::<OsString>().unwrap();
        let display_name = if let Some(ref v) = install_config.display_name {
            v.parse::<OsString>().unwrap()
        } else {
            name.clone()
        };
        let executable_path = ctx.program.clone();
        let launch_arguments = ctx.args.clone();
        let service_type = ServiceType::from_bits(install_config.service_type as u32).unwrap();
        let start_type = if let Some(v) = install_config.start_type {
            ServiceStartType::from_raw(v as u32).unwrap()
        } else {
            if ctx.autostart { ServiceStartType::AutoStart } else { ServiceStartType::OnDemand }
        };
        let dependencies: Vec<ServiceDependency> = if let Some(ref v) = install_config.dependencies {
            v.iter()
                .map(|s| ServiceDependency::from_system_identifier(s))
                .collect()
        } else {
            Vec::new()
        };
        let service_info = ServiceInfo {
            name,
            display_name,
            service_type,
            start_type,
            error_control: ServiceErrorControl::Normal,
            executable_path,
            launch_arguments,
            dependencies,
            account_name: None,
            account_password: None,
        };

        let service = manager
            .create_service(&service_info, ServiceAccess::ALL_ACCESS)
            .map_err(|e| {
                std::io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to create service: {}", e),
                )
            })?;

        service.set_delayed_auto_start(install_config.delay_autostart).map_err(|e| {
            std::io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to set service delayed autostart: {}", e),
            )
        })?;

        if let Some(ref v) = install_config.description {
            service.set_description(v).map_err(|e| {
                std::io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to set service description: {}", e),
                )
            })?;
        }

        Ok(())
    }

    pub fn service_uninstall(ctx: &super::ServiceUninstallCtx) -> std::io::Result<()> {
        let manager = get_win_service_manager()?;
        let service = manager
            .open_service(ctx.label.to_qualified_name(), ServiceAccess::DELETE)
            .map_err(|e| {
                std::io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to open service: {}", e),
                )
            })?;
        service.delete().map_err(|e| {
            std::io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to delete service: {}", e),
            )
        })?;
        Ok(())
    }

    pub fn service_start(ctx: &super::ServiceStartCtx) -> std::io::Result<()> {
        let manager = get_win_service_manager()?;
        let service = manager
            .open_service(ctx.label.to_qualified_name(), ServiceAccess::START)
            .map_err(|e| {
                std::io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to open service: {}", e),
                )
            })?;

        service.start(&[] as &[&str]).map_err(|e| {
            std::io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to start service: {}", e),
            )
        })?;

        Ok(())
    }

    pub fn service_stop(ctx: &super::ServiceStopCtx) -> std::io::Result<()> {
        let manager = get_win_service_manager()?;
        let service = manager
            .open_service(ctx.label.to_qualified_name(), ServiceAccess::STOP)
            .map_err(|e| {
                std::io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to open service: {}", e),
                )
            })?;

        service.stop().map_err(|e| {
            std::io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to stop service: {}", e),
            )
        })?;

        Ok(())
    }

    pub fn service_status(ctx: &super::ServiceStatusCtx) -> std::io::Result<crate::ServiceStatus> {
        let manager = get_win_service_manager()?;

        match manager.open_service(ctx.label.to_qualified_name(), ServiceAccess::QUERY_STATUS) {
            Ok(service) => {
                let status = service.query_status().map_err(|e| {
                    std::io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to query service status: {}", e),
                    )
                })?;

                if status.current_state == ServiceState::Stopped {
                    Ok(crate::ServiceStatus::Stopped(match status.exit_code {
                        ServiceExitCode::NO_ERROR => None,
                        ServiceExitCode::Win32(code) => {
                            Some(format!("Win32 error code: {:x}", code))
                        }
                        ServiceExitCode::ServiceSpecific(code) => {
                            Some(format!("Service specific error code: {:x}", code))
                        }
                    }))
                } else {
                    Ok(crate::ServiceStatus::Running)
                }
            }
            Err(e) => {
                if let windows_service::Error::Winapi(ref win_err) = e {
                    if win_err.raw_os_error() == Some(0x424) {
                        return Ok(crate::ServiceStatus::NotInstalled);
                    }
                }
                return Err(io::Error::new(io::ErrorKind::Other, e));
            }
        }
    }

    fn get_win_service_manager() -> std::io::Result<ServiceManager> {
        ServiceManager::local_computer(None::<&OsStr>, ServiceManagerAccess::ALL_ACCESS).map_err(
            |e| {
                std::io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to get service manager: {}", e),
                )
            },
        )
    }
    
}

#[cfg(not(target_os = "windows"))]
mod scm_handler {
    use std::io;
    const ERROR_MSG: &str = "Service control manager is not supported on this platform";

    pub fn service_install(_ctx: &super::ServiceInstallCtx) -> std::io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, ERROR_MSG))
    }

    pub fn service_uninstall(_ctx: &super::ServiceUninstallCtx) -> std::io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, ERROR_MSG))
    }

    pub fn service_start(_ctx: &super::ServiceStartCtx) -> std::io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, ERROR_MSG))
    }

    pub fn service_stop(_ctx: &super::ServiceStopCtx) -> std::io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, ERROR_MSG))
    }

    pub fn service_status(_ctx: &super::ServiceStatusCtx) -> std::io::Result<crate::ServiceStatus> {
        Err(io::Error::new(io::ErrorKind::Unsupported, ERROR_MSG))
    }
}
