use serde_json::{Value, json};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[cfg(windows)]
pub struct Instance(*mut std::ffi::c_void);
#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateMutexW(
        attributes: *const std::ffi::c_void,
        owner: i32,
        name: *const u16,
    ) -> *mut std::ffi::c_void;
    fn GetLastError() -> u32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
}
#[cfg(windows)]
impl Instance {
    pub fn acquire() -> Option<Self> {
        let name: Vec<u16> = "Local\\GoXLR_EX_Rust_Studio"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // The handle is owned by this guard for the duration of the main thread.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return None;
        }
        if unsafe { GetLastError() } == 183 {
            unsafe { CloseHandle(handle) };
            return None;
        }
        Some(Self(handle))
    }
}
#[cfg(windows)]
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub struct Runtime {
    pub data: PathBuf,
    child: Option<Child>,
}
fn extract(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.exists() {
        if std::fs::read(path).map_err(|e| e.to_string())? != bytes {
            return Err(format!("Fichier inattendu : {}", path.display()));
        }
    } else {
        std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}
impl Runtime {
    pub fn start() -> Result<Self, String> {
        let data = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .unwrap()
            .join("GoXLR-EX-Rust-data");
        let runtime = data.join("runtime-1.2.4");
        std::fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
        let engine = runtime.join("goxlr-daemon.exe");
        extract(&engine, include_bytes!("../vendor-assets/goxlr-daemon.exe"))?;
        extract(
            &runtime.join("goxlr-defaults.exe"),
            include_bytes!("../vendor-assets/goxlr-defaults.exe"),
        )?;
        let licences = data.join("licences");
        std::fs::create_dir_all(&licences).map_err(|e| e.to_string())?;
        for (name, content) in [
            ("LICENSE", include_bytes!("../vendor-assets/LICENSE").as_slice()),
            ("LICENSE-UI", include_bytes!("../../../LICENSE").as_slice()),
            (
                "LICENSE-3RD-PARTY",
                include_bytes!("../vendor-assets/LICENSE-3RD-PARTY").as_slice(),
            ),
            (
                "LICENSE-FONT",
                include_bytes!("../assets/OFL.txt").as_slice(),
            ),
            (
                "LICENSE-EGUI",
                include_bytes!("../assets/LICENSE-egui.txt").as_slice(),
            ),
        ] {
            std::fs::write(licences.join(name), content).map_err(|e| e.to_string())?;
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_millis(700))
            .build();
        if let Ok(response) = agent.get("http://127.0.0.1:14564/api/get-devices").call() {
            if response.into_json::<goxlr_ipc::DaemonStatus>().is_ok() {
                return Ok(Self { data, child: None });
            }
            return Err("Un service incompatible occupe le port 14564.".into());
        }
        if std::env::args().any(|arg| arg == "--ui-test") {
            return Ok(Self { data, child: None });
        }
        let config_path = data.join("settings.json");
        let mut config: Value = if config_path.exists() {
            serde_json::from_slice(&std::fs::read(&config_path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Configuration invalide : {e}"))?
        } else {
            json!({})
        };
        if !config.is_object() {
            return Err("Le fichier settings.json est invalide.".into());
        }
        for (key, folder) in [
            ("profile_directory", "profiles"),
            ("mic_profile_directory", "mic-profiles"),
            ("samples_directory", "samples"),
            ("presets_directory", "presets"),
            ("icons_directory", "icons"),
            ("logs_directory", "logs"),
            ("backup_directory", "backups"),
        ] {
            let path = data.join(folder);
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            config[key] = json!(path);
        }
        config["show_tray_icon"] = json!(false);
        config["allow_network_access"] = json!(false);
        config["open_ui_on_launch"] = json!(false);
        config["check_app_updates"] = json!(false);
        config["tts_enabled"] = json!(false);
        config["log_level"] = json!("Info");
        std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap())
            .map_err(|e| e.to_string())?;
        let mut command = Command::new(engine);
        command
            .current_dir(&runtime)
            .arg("--config")
            .arg(config_path)
            .args([
                "--http-bind-address",
                "127.0.0.1",
                "--http-port",
                "14564",
                "--disable-tray",
                "true",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let child = command.spawn().map_err(|e| e.to_string())?;
        Ok(Self {
            data,
            child: Some(child),
        })
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let Some(child) = &mut self.child else {
            return;
        };
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(3))
            .build();
        let _ = agent
            .post("http://127.0.0.1:14564/api/command")
            .send_json(json!({"Daemon":"StopDaemon"}));
        for _ in 0..30 {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // Only the child spawned by this instance is owned and eligible for termination.
        let _ = child.kill();
        let _ = child.wait();
    }
}
