use crate::utils::wrap_output;
use crate::ServiceStatus;

use super::{
    ServiceInstallCtx, ServiceLevel, ServiceManager, ServiceStartCtx, ServiceStopCtx,
    ServiceUninstallCtx,
};
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufWriter, Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use xml::common::XmlVersion;
use xml::reader::EventReader;
use xml::writer::{EmitterConfig, EventWriter, XmlEvent};

static WINSW_EXE: &str = "winsw.exe";

///
/// Service configuration
///

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WinSwConfig {
    pub install: WinSwInstallConfig,
    pub options: WinSwOptionsConfig,
    pub service_definition_dir_path: PathBuf,
}

impl Default for WinSwConfig {
    fn default() -> Self {
        WinSwConfig {
            install: WinSwInstallConfig::default(),
            options: WinSwOptionsConfig::default(),
            service_definition_dir_path: PathBuf::from("C:\\ProgramData\\service-manager"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WinSwInstallConfig {
    pub description: Option<String>,
    pub display_name: Option<String>,
    pub failure_action: WinSwOnFailureAction,   
    pub reset_failure_time: Option<String>,
    pub security_descriptor: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WinSwOptionsConfig {
    pub priority: Option<WinSwPriority>,
    pub stop_timeout: Option<String>,
    pub stop_executable: Option<PathBuf>,
    pub stop_args: Option<Vec<OsString>>,
    pub start_mode: Option<WinSwStartType>,
    pub delayed_autostart: Option<bool>,
    pub dependent_services: Option<Vec<String>>,
    pub interactive: Option<bool>,
    pub beep_on_shutdown: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum WinSwOnFailureAction {
    Restart(Option<String>),
    Reboot,
    #[default]
    None,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WinSwStartType {
    // The service automatically starts along with the OS, before user login.
    Automatic,
    /// The service is a device driver loaded by the boot loader.
    Boot,
    /// The service must be started manually.
    Manual,
    /// The service is a device driver started during kernel initialization.
    System,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum WinSwPriority {
    #[default]
    Normal,
    Idle,
    High,
    RealTime,
    BelowNormal,
    AboveNormal,
}

///
/// Service manager implementation
///

/// Implementation of [`ServiceManager`] for [Window Service](https://en.wikipedia.org/wiki/Windows_service)
/// leveraging [`winsw.exe`](https://github.com/winsw/winsw)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WinSwServiceManager {
    pub config: WinSwConfig,
}

impl WinSwServiceManager {
    pub fn system() -> Self {
        let config = WinSwConfig {
            install: WinSwInstallConfig::default(),
            options: WinSwOptionsConfig::default(),
            service_definition_dir_path: PathBuf::from("C:\\ProgramData\\service-manager"),
        };
        Self { config }
    }

    pub fn with_config(self, config: WinSwConfig) -> Self {
        Self { config }
    }

    pub fn write_service_configuration(
        path: &PathBuf,
        ctx: &ServiceInstallCtx,
        config: &WinSwConfig,
    ) -> io::Result<()> {
        let mut file = File::create(path).unwrap();
        if let Some(contents) = &ctx.contents {
            if Self::is_valid_xml(contents) {
                file.write_all(contents.as_bytes())?;
                return Ok(());
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The contents override was not a valid XML document",
            ));
        }

        let file = BufWriter::new(file);
        let mut writer = EmitterConfig::new()
            .perform_indent(true)
            .create_writer(file);
        writer
            .write(XmlEvent::StartDocument {
                version: XmlVersion::Version10,
                encoding: Some("UTF-8"),
                standalone: None,
            })
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Writing service config failed: {}", e),
                )
            })?;

        // <service>
        writer
            .write(XmlEvent::start_element("service"))
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Writing service config failed: {}", e),
                )
            })?;

        // Mandatory values
        Self::write_element(&mut writer, "id", &ctx.label.to_qualified_name())?;
        Self::write_element(&mut writer, "executable", &ctx.program.to_string_lossy())?;

        if let Some(display_name) =  &config.install.display_name {
            Self::write_element(&mut writer, "name", display_name)?;
        } else {
            Self::write_element(&mut writer, "name", &ctx.label.to_qualified_name())?;         
        }

        if let Some(description) = &config.install.description {
            Self::write_element(&mut writer, "description", description)?;
        } else {
            Self::write_element(
                &mut writer,
                "description",
                &format!("Service for {}", ctx.label.to_qualified_name()),
            )?;
        }

        let args = ctx
            .args
            .clone()
            .into_iter()
            .map(|s| s.into_string().unwrap_or_default())
            .collect::<Vec<String>>()
            .join(" ");
        Self::write_element(&mut writer, "arguments", &args)?;

        if let Some(working_directory) = &ctx.working_directory {
            Self::write_element(
                &mut writer,
                "workingdirectory",
                &working_directory.to_string_lossy(),
            )?;
        }
        if let Some(env_vars) = &ctx.environment {
            for var in env_vars.iter() {
                Self::write_element_with_attributes(
                    &mut writer,
                    "env",
                    &[("name", &var.0), ("value", &var.1)],
                    None,
                )?;
            }
        }

        // Optional install elements
        let (action, delay) = match &config.install.failure_action {
            WinSwOnFailureAction::Restart(delay) => ("restart", delay.as_deref()),
            WinSwOnFailureAction::Reboot => ("reboot", None),
            WinSwOnFailureAction::None => ("none", None),
        };
        let attributes = delay.map_or_else(
            || vec![("action", action)],
            |d| vec![("action", action), ("delay", d)],
        );
        Self::write_element_with_attributes(&mut writer, "onfailure", &attributes, None)?;

        if let Some(reset_time) = &config.install.reset_failure_time {
            Self::write_element(&mut writer, "resetfailure", reset_time)?;
        }
        if let Some(security_descriptor) = &config.install.security_descriptor {
            Self::write_element(&mut writer, "securityDescriptor", security_descriptor)?;
        }

        // Other optional elements
        if let Some(priority) = &config.options.priority {
            Self::write_element(&mut writer, "priority", &format!("{:?}", priority))?;
        }
        if let Some(stop_timeout) = &config.options.stop_timeout {
            Self::write_element(&mut writer, "stoptimeout", stop_timeout)?;
        }
        if let Some(stop_executable) = &config.options.stop_executable {
            Self::write_element(
                &mut writer,
                "stopexecutable",
                &stop_executable.to_string_lossy(),
            )?;
        }
        if let Some(stop_args) = &config.options.stop_args {
            let stop_args = stop_args
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect::<Vec<String>>()
                .join(" ");
            Self::write_element(&mut writer, "stoparguments", &stop_args)?;
        }

        if let Some(start_mode) = &config.options.start_mode {
            Self::write_element(&mut writer, "startmode", &format!("{:?}", start_mode))?;
        } else if ctx.autostart {
            Self::write_element(&mut writer, "startmode", "Automatic")?;
        } else {
            Self::write_element(&mut writer, "startmode", "Manual")?;
        }

        if let Some(delayed_autostart) = config.options.delayed_autostart {
            Self::write_element(
                &mut writer,
                "delayedAutoStart",
                &delayed_autostart.to_string(),
            )?;
        }
        if let Some(dependent_services) = &config.options.dependent_services {
            for service in dependent_services {
                Self::write_element(&mut writer, "depend", service)?;
            }
        }
        if let Some(interactive) = config.options.interactive {
            Self::write_element(&mut writer, "interactive", &interactive.to_string())?;
        }
        if let Some(beep_on_shutdown) = config.options.beep_on_shutdown {
            Self::write_element(&mut writer, "beeponshutdown", &beep_on_shutdown.to_string())?;
        }

        // </service>
        writer.write(XmlEvent::end_element()).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Writing service config failed: {}", e),
            )
        })?;

        Ok(())
    }

    fn write_element<W: Write>(
        writer: &mut EventWriter<W>,
        name: &str,
        value: &str,
    ) -> io::Result<()> {
        writer.write(XmlEvent::start_element(name)).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to write element '{}': {}", name, e),
            )
        })?;
        writer.write(XmlEvent::characters(value)).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to write value for element '{}': {}", name, e),
            )
        })?;
        writer.write(XmlEvent::end_element()).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to end element '{}': {}", name, e),
            )
        })?;
        Ok(())
    }

    fn write_element_with_attributes<W: Write>(
        writer: &mut EventWriter<W>,
        name: &str,
        attributes: &[(&str, &str)],
        value: Option<&str>,
    ) -> io::Result<()> {
        let mut start_element = XmlEvent::start_element(name);
        for &(attr_name, attr_value) in attributes {
            start_element = start_element.attr(attr_name, attr_value);
        }
        writer.write(start_element).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to write value for element '{}': {}", name, e),
            )
        })?;

        if let Some(val) = value {
            writer.write(XmlEvent::characters(val)).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to write value for element '{}': {}", name, e),
                )
            })?;
        }

        writer.write(XmlEvent::end_element()).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to end element '{}': {}", name, e),
            )
        })?;

        Ok(())
    }

    fn is_valid_xml(xml_string: &str) -> bool {
        let cursor = Cursor::new(xml_string);
        let parser = EventReader::new(cursor);
        for e in parser {
            if e.is_err() {
                return false;
            }
        }
        true
    }
}

impl ServiceManager for WinSwServiceManager {
    fn available(&self) -> io::Result<bool> {
        match which::which(WINSW_EXE) {
            Ok(_) => Ok(true),
            Err(which::Error::CannotFindBinaryPath) => match std::env::var("WINSW_PATH") {
                Ok(val) => {
                    let path = PathBuf::from(val);
                    Ok(path.exists())
                }
                Err(_) => Ok(false),
            },
            Err(x) => Err(io::Error::new(io::ErrorKind::Other, x)),
        }
    }

    fn install(&self, ctx: ServiceInstallCtx) -> io::Result<()> {
        let service_name = ctx.label.to_qualified_name();
        let service_instance_path = self
            .config
            .service_definition_dir_path
            .join(service_name.clone());
        std::fs::create_dir_all(&service_instance_path)?;

        let service_config_path = service_instance_path.join(format!("{service_name}.xml"));
        Self::write_service_configuration(&service_config_path, &ctx, &self.config)?;

        wrap_output(winsw_exe("install", &service_name, &service_instance_path)?)?;
        Ok(())
    }

    fn uninstall(&self, ctx: ServiceUninstallCtx) -> io::Result<()> {
        let service_name = ctx.label.to_qualified_name();
        let service_instance_path = self
            .config
            .service_definition_dir_path
            .join(service_name.clone());
        wrap_output(winsw_exe(
            "uninstall",
            &service_name,
            &service_instance_path,
        )?)?;

        // The service directory is populated with the service definition, and other log files that
        // get generated by WinSW. It can be problematic if a service is later created with the
        // same name. Things are easier to manage if the directory is deleted.
        std::fs::remove_dir_all(service_instance_path)?;

        Ok(())
    }

    fn start(&self, ctx: ServiceStartCtx) -> io::Result<()> {
        let service_name = ctx.label.to_qualified_name();
        let service_instance_path = self
            .config
            .service_definition_dir_path
            .join(service_name.clone());
        wrap_output(winsw_exe("start", &service_name, &service_instance_path)?)?;
        Ok(())
    }

    fn stop(&self, ctx: ServiceStopCtx) -> io::Result<()> {
        let service_name = ctx.label.to_qualified_name();
        let service_instance_path = self
            .config
            .service_definition_dir_path
            .join(service_name.clone());
        wrap_output(winsw_exe("stop", &service_name, &service_instance_path)?)?;
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
                "Windows does not support user-level services",
            )),
        }
    }

    fn status(&self, ctx: crate::ServiceStatusCtx) -> io::Result<ServiceStatus> {
        let service_name = ctx.label.to_qualified_name();
        let service_instance_path = self
            .config
            .service_definition_dir_path
            .join(service_name.clone());
        if !service_instance_path.exists() {
            return Ok(ServiceStatus::NotInstalled);
        }
        let output = winsw_exe("status", &service_name, &service_instance_path)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // It seems the error message is thrown by WinSW v2.x because only WinSW.[xml|yml] is supported
            if stderr.contains("System.IO.FileNotFoundException: Unable to locate WinSW.[xml|yml] file within executable directory") {
                return Ok(ServiceStatus::NotInstalled);
            }
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to get service status: {}", stderr),
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("NonExistent") {
            Ok(ServiceStatus::NotInstalled)
        } else if stdout.contains("running") {
            Ok(ServiceStatus::Running)
        } else {
            Ok(ServiceStatus::Stopped(None))
        }
    }
}

fn winsw_exe(cmd: &str, service_name: &str, working_dir_path: &Path) -> io::Result<Output> {
    let winsw_path = match std::env::var("WINSW_PATH") {
        Ok(val) => {
            let path = PathBuf::from(val);
            if path.exists() {
                path
            } else {
                PathBuf::from(WINSW_EXE)
            }
        }
        Err(_) => PathBuf::from(WINSW_EXE),
    };

    let mut command = Command::new(winsw_path);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.current_dir(working_dir_path);
    command.arg(cmd).arg(format!("{}.xml", service_name));

    command.output()
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use indoc::indoc;
    use std::ffi::OsString;
    use std::io::Cursor;
    use xml::reader::{EventReader, XmlEvent};

    fn get_element_value(xml: &str, element_name: &str) -> String {
        let cursor = Cursor::new(xml);
        let parser = EventReader::new(cursor);
        let mut inside_target_element = false;

        for e in parser {
            match e {
                Ok(XmlEvent::StartElement { name, .. }) if name.local_name == element_name => {
                    inside_target_element = true;
                }
                Ok(XmlEvent::Characters(text)) if inside_target_element => {
                    return text;
                }
                Ok(XmlEvent::EndElement { name }) if name.local_name == element_name => {
                    inside_target_element = false;
                }
                Err(e) => panic!("Error while parsing XML: {}", e),
                _ => {}
            }
        }

        panic!("Element {} not found", element_name);
    }

    fn get_element_attribute_value(xml: &str, element_name: &str, attribute_name: &str) -> String {
        let cursor = Cursor::new(xml);
        let parser = EventReader::new(cursor);

        for e in parser {
            match e {
                Ok(XmlEvent::StartElement {
                    name, attributes, ..
                }) if name.local_name == element_name => {
                    for attr in attributes {
                        if attr.name.local_name == attribute_name {
                            return attr.value;
                        }
                    }
                }
                Err(e) => panic!("Error while parsing XML: {}", e),
                _ => {}
            }
        }

        panic!("Attribute {} not found", attribute_name);
    }

    fn get_element_values(xml: &str, element_name: &str) -> Vec<String> {
        let cursor = Cursor::new(xml);
        let parser = EventReader::new(cursor);
        let mut values = Vec::new();
        let mut inside_target_element = false;

        for e in parser {
            match e {
                Ok(XmlEvent::StartElement { name, .. }) if name.local_name == element_name => {
                    inside_target_element = true;
                }
                Ok(XmlEvent::Characters(text)) if inside_target_element => {
                    values.push(text);
                }
                Ok(XmlEvent::EndElement { name }) if name.local_name == element_name => {
                    inside_target_element = false;
                }
                Err(e) => panic!("Error while parsing XML: {}", e),
                _ => {}
            }
        }

        values
    }

    fn get_environment_variables(xml: &str) -> Vec<(String, String)> {
        let cursor = Cursor::new(xml);
        let parser = EventReader::new(cursor);
        let mut env_vars = Vec::new();

        for e in parser.into_iter().flatten() {
            if let XmlEvent::StartElement {
                name, attributes, ..
            } = e
            {
                if name.local_name == "env" {
                    let mut name_value_pair = (String::new(), String::new());
                    for attr in attributes {
                        match attr.name.local_name.as_str() {
                            "name" => name_value_pair.0 = attr.value,
                            "value" => name_value_pair.1 = attr.value,
                            _ => {}
                        }
                    }
                    if !name_value_pair.0.is_empty() && !name_value_pair.1.is_empty() {
                        env_vars.push(name_value_pair);
                    }
                }
            }
        }
        env_vars
    }

    #[test]
    fn test_service_configuration_with_mandatory_elements() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let service_config_file = temp_dir.child("service_config.xml");

        let ctx = ServiceInstallCtx {
            label: "org.example.my_service".parse().unwrap(),
            program: PathBuf::from("C:\\Program Files\\org.example\\my_service.exe"),
            args: vec![
                OsString::from("--arg"),
                OsString::from("value"),
                OsString::from("--another-arg"),
            ],
            contents: None,
            username: None,
            working_directory: None,
            environment: None,
            autostart: true
        };

        WinSwServiceManager::write_service_configuration(
            &service_config_file.to_path_buf(),
            &ctx,
            &WinSwConfig::default(),
        )
        .unwrap();

        let xml = std::fs::read_to_string(service_config_file.path()).unwrap();

        service_config_file.assert(predicates::path::is_file());
        assert_eq!("org.example.my_service", get_element_value(&xml, "id"));
        assert_eq!("org.example.my_service", get_element_value(&xml, "name"));
        assert_eq!(
            "C:\\Program Files\\org.example\\my_service.exe",
            get_element_value(&xml, "executable")
        );
        assert_eq!(
            "Service for org.example.my_service",
            get_element_value(&xml, "description")
        );
        assert_eq!(
            "--arg value --another-arg",
            get_element_value(&xml, "arguments")
        );
        assert_eq!("Automatic", get_element_value(&xml, "startmode"));
    }

    #[test]
    fn test_service_configuration_with_autostart_false() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let service_config_file = temp_dir.child("service_config.xml");

        let ctx = ServiceInstallCtx {
            label: "org.example.my_service".parse().unwrap(),
            program: PathBuf::from("C:\\Program Files\\org.example\\my_service.exe"),
            args: vec![
                OsString::from("--arg"),
                OsString::from("value"),
                OsString::from("--another-arg"),
            ],
            contents: None,
            username: None,
            working_directory: None,
            environment: None,
            autostart: false
        };

        WinSwServiceManager::write_service_configuration(
            &service_config_file.to_path_buf(),
            &ctx,
            &WinSwConfig::default(),
        )
        .unwrap();

        let xml = std::fs::read_to_string(service_config_file.path()).unwrap();

        service_config_file.assert(predicates::path::is_file());
        assert_eq!("org.example.my_service", get_element_value(&xml, "id"));
        assert_eq!("org.example.my_service", get_element_value(&xml, "name"));
        assert_eq!(
            "C:\\Program Files\\org.example\\my_service.exe",
            get_element_value(&xml, "executable")
        );
        assert_eq!(
            "Service for org.example.my_service",
            get_element_value(&xml, "description")
        );
        assert_eq!(
            "--arg value --another-arg",
            get_element_value(&xml, "arguments")
        );
        assert_eq!("Manual", get_element_value(&xml, "startmode"));
    }

    #[test]
    fn test_service_configuration_with_special_start_type_should_override_autostart() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let service_config_file = temp_dir.child("service_config.xml");

        let ctx = ServiceInstallCtx {
            label: "org.example.my_service".parse().unwrap(),
            program: PathBuf::from("C:\\Program Files\\org.example\\my_service.exe"),
            args: vec![
                OsString::from("--arg"),
                OsString::from("value"),
                OsString::from("--another-arg"),
            ],
            contents: None,
            username: None,
            working_directory: None,
            environment: None,
            autostart: false
        };

        let mut config = WinSwConfig::default();
        config.options.start_mode = Some(WinSwStartType::Boot);
        WinSwServiceManager::write_service_configuration(
            &service_config_file.to_path_buf(),
            &ctx,
            &config,
        )
        .unwrap();

        let xml = std::fs::read_to_string(service_config_file.path()).unwrap();

        service_config_file.assert(predicates::path::is_file());
        assert_eq!("org.example.my_service", get_element_value(&xml, "id"));
        assert_eq!("org.example.my_service", get_element_value(&xml, "name"));
        assert_eq!(
            "C:\\Program Files\\org.example\\my_service.exe",
            get_element_value(&xml, "executable")
        );
        assert_eq!(
            "Service for org.example.my_service",
            get_element_value(&xml, "description")
        );
        assert_eq!(
            "--arg value --another-arg",
            get_element_value(&xml, "arguments")
        );
        assert_eq!("Boot", get_element_value(&xml, "startmode"));
    }

    #[test]
    fn test_service_configuration_with_full_options() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let service_config_file = temp_dir.child("service_config.xml");

        let ctx = ServiceInstallCtx {
            label: "org.example.my_service".parse().unwrap(),
            program: PathBuf::from("C:\\Program Files\\org.example\\my_service.exe"),
            args: vec![
                OsString::from("--arg"),
                OsString::from("value"),
                OsString::from("--another-arg"),
            ],
            contents: None,
            username: None,
            working_directory: Some(PathBuf::from("C:\\Program Files\\org.example")),
            environment: Some(vec![
                ("ENV1".to_string(), "val1".to_string()),
                ("ENV2".to_string(), "val2".to_string()),
            ]),
            autostart: true
        };

        let config = WinSwConfig {
            install: WinSwInstallConfig {
                display_name: Some("org example my_service".to_string()),
                failure_action: WinSwOnFailureAction::Restart(Some("10 sec".to_string())),
                description: Some("Service for org.example.my_service".to_string()),
                reset_failure_time: Some("1 hour".to_string()),
                security_descriptor: Some(
                    "O:AOG:DAD:(A;;RPWPCCDCLCSWRCWDWOGA;;;S-1-0-0)".to_string(),
                ),
            },
            options: WinSwOptionsConfig {
                priority: Some(WinSwPriority::High),
                stop_timeout: Some("15 sec".to_string()),
                stop_executable: Some(PathBuf::from("C:\\Temp\\stop.exe")),
                stop_args: Some(vec![
                    OsString::from("--stop-arg1"),
                    OsString::from("arg1val"),
                    OsString::from("--stop-arg2-flag"),
                ]),
                start_mode: Some(WinSwStartType::Manual),
                delayed_autostart: Some(true),
                dependent_services: Some(vec!["service1".to_string(), "service2".to_string()]),
                interactive: Some(true),
                beep_on_shutdown: Some(true),
            },
            service_definition_dir_path: PathBuf::from("C:\\Temp\\service-definitions"),
        };

        WinSwServiceManager::write_service_configuration(
            &service_config_file.to_path_buf(),
            &ctx,
            &config,
        )
        .unwrap();

        let xml = std::fs::read_to_string(service_config_file.path()).unwrap();
        println!("{xml}");

        service_config_file.assert(predicates::path::is_file());
        assert_eq!("org.example.my_service", get_element_value(&xml, "id"));
        assert_eq!("org.example.my_service", get_element_value(&xml, "name"));
        assert_eq!(
            "C:\\Program Files\\org.example\\my_service.exe",
            get_element_value(&xml, "executable")
        );
        assert_eq!(
            "Service for org.example.my_service",
            get_element_value(&xml, "description")
        );
        assert_eq!(
            "--arg value --another-arg",
            get_element_value(&xml, "arguments")
        );
        assert_eq!(
            "C:\\Program Files\\org.example",
            get_element_value(&xml, "workingdirectory")
        );

        let attributes = get_environment_variables(&xml);
        assert_eq!(attributes[0].0, "ENV1");
        assert_eq!(attributes[0].1, "val1");
        assert_eq!(attributes[1].0, "ENV2");
        assert_eq!(attributes[1].1, "val2");

        // Install options
        assert_eq!(
            "restart",
            get_element_attribute_value(&xml, "onfailure", "action")
        );
        assert_eq!(
            "10 sec",
            get_element_attribute_value(&xml, "onfailure", "delay")
        );
        assert_eq!("1 hour", get_element_value(&xml, "resetfailure"));
        assert_eq!(
            "O:AOG:DAD:(A;;RPWPCCDCLCSWRCWDWOGA;;;S-1-0-0)",
            get_element_value(&xml, "securityDescriptor")
        );

        // Other options
        assert_eq!("High", get_element_value(&xml, "priority"));
        assert_eq!("15 sec", get_element_value(&xml, "stoptimeout"));
        assert_eq!(
            "C:\\Temp\\stop.exe",
            get_element_value(&xml, "stopexecutable")
        );
        assert_eq!(
            "--stop-arg1 arg1val --stop-arg2-flag",
            get_element_value(&xml, "stoparguments")
        );
        assert_eq!("Manual", get_element_value(&xml, "startmode"));
        assert_eq!("true", get_element_value(&xml, "delayedAutoStart"));

        let dependent_services = get_element_values(&xml, "depend");
        assert_eq!("service1", dependent_services[0]);
        assert_eq!("service2", dependent_services[1]);

        assert_eq!("true", get_element_value(&xml, "interactive"));
        assert_eq!("true", get_element_value(&xml, "beeponshutdown"));
    }

    #[test]
    fn test_service_configuration_with_contents() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let service_config_file = temp_dir.child("service_config.xml");

        let contents = indoc! {r#"
            <service>
                <id>jenkins</id>
                <name>Jenkins</name>
                <description>This service runs Jenkins continuous integration system.</description>
                <executable>java</executable>
                <arguments>-Xrs -Xmx256m -jar "%BASE%\jenkins.war" --httpPort=8080</arguments>
                <startmode>Automatic</startmode>
            </service>
        "#};
        let ctx = ServiceInstallCtx {
            label: "org.example.my_service".parse().unwrap(),
            program: PathBuf::from("C:\\Program Files\\org.example\\my_service.exe"),
            args: vec![
                OsString::from("--arg"),
                OsString::from("value"),
                OsString::from("--another-arg"),
            ],
            contents: Some(contents.to_string()),
            username: None,
            working_directory: None,
            environment: None,
            autostart: true
        };

        WinSwServiceManager::write_service_configuration(
            &service_config_file.to_path_buf(),
            &ctx,
            &WinSwConfig::default(),
        )
        .unwrap();

        let xml = std::fs::read_to_string(service_config_file.path()).unwrap();

        service_config_file.assert(predicates::path::is_file());
        assert_eq!("jenkins", get_element_value(&xml, "id"));
        assert_eq!("Jenkins", get_element_value(&xml, "name"));
        assert_eq!("java", get_element_value(&xml, "executable"));
        assert_eq!(
            "This service runs Jenkins continuous integration system.",
            get_element_value(&xml, "description")
        );
        assert_eq!(
            "-Xrs -Xmx256m -jar \"%BASE%\\jenkins.war\" --httpPort=8080",
            get_element_value(&xml, "arguments")
        );
    }

    #[test]
    fn test_service_configuration_with_invalid_contents() {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        let service_config_file = temp_dir.child("service_config.xml");

        let ctx = ServiceInstallCtx {
            label: "org.example.my_service".parse().unwrap(),
            program: PathBuf::from("C:\\Program Files\\org.example\\my_service.exe"),
            args: vec![
                OsString::from("--arg"),
                OsString::from("value"),
                OsString::from("--another-arg"),
            ],
            contents: Some("this is not an XML document".to_string()),
            username: None,
            working_directory: None,
            environment: None,
            autostart: true
        };

        let result = WinSwServiceManager::write_service_configuration(
            &service_config_file.to_path_buf(),
            &ctx,
            &WinSwConfig::default(),
        );

        match result {
            Ok(()) => panic!("This test should result in a failure"),
            Err(e) => assert_eq!(
                "The contents override was not a valid XML document",
                e.to_string()
            ),
        }
    }
}
