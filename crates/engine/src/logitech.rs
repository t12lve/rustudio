//! Souris Logitech G (récepteur LIGHTSPEED/Unifying) : un fil HID indépendant. L'API lit le cache ;
//! les commandes passent par ce fil, une à la fois, avec relecture de confirmation côté crate.
use rustudio_logitech_g::{Mouse, Settings, Transport};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub const ID: &str = "logitech-g";

struct Request {
    action: String,
    body: Value,
    deadline: Instant,
    answer: mpsc::SyncSender<Result<Value, String>>,
}
#[derive(Clone)]
struct State {
    settings: Option<Settings>,
    connected: bool,
    present: bool,
    error: Option<String>,
}
struct Shared {
    state: Mutex<State>,
    version: AtomicU64,
    simulated: bool,
}
pub struct LogitechHub {
    shared: Arc<Shared>,
    tx: mpsc::SyncSender<Request>,
    rx: Mutex<Option<mpsc::Receiver<Request>>>,
}

impl LogitechHub {
    pub fn new(simulated: bool) -> Self {
        let (tx, rx) = mpsc::sync_channel(1);
        let settings = if simulated { Mouse::new(rustudio_logitech_g::sim::Sim::default()).read().ok() } else { None };
        let state = State { connected: settings.is_some(), present: settings.is_some(), settings, error: None };
        Self { shared: Arc::new(Shared { state: Mutex::new(state), version: AtomicU64::new(0), simulated }), tx, rx: Mutex::new(Some(rx)) }
    }
    pub fn changes(&self) -> u64 {
        self.shared.version.load(Ordering::Relaxed)
    }
    pub fn runtime(&self) -> rustudio_core::modules::ModuleRuntime {
        use rustudio_core::modules::{ModuleRuntime, ModuleStatus};
        let s = self.shared.state.lock().unwrap();
        let status = if s.connected { ModuleStatus::Ready } else if s.present { ModuleStatus::Error } else { ModuleStatus::Disconnected };
        ModuleRuntime {
            id: ID, status, connected: s.connected, simulated: self.shared.simulated,
            firmware: if s.connected { s.settings.as_ref().and_then(|x| x.firmware.clone()) } else { None },
            firmware_kind: None,
            error: s.error.clone(),
        }
    }
    pub fn state(&self) -> Value {
        let s = self.shared.state.lock().unwrap();
        json!({"state": s.settings, "v": self.changes()})
    }
    pub fn command(&self, action: &str, body: &Value) -> Result<Value, String> {
        if !matches!(action, "setting" | "settings" | "refresh") {
            return Err("Commande souris Logitech inconnue.".into());
        }
        if self.rx.lock().unwrap().is_some() {
            return Err("Le module souris Logitech n’est pas démarré".into());
        }
        let (answer, rx) = mpsc::sync_channel(1);
        let req = Request { action: action.into(), body: body.clone(), deadline: Instant::now() + Duration::from_secs(15), answer };
        self.tx.try_send(req).map_err(|_| "Une commande souris est déjà en cours".to_string())?;
        rx.recv_timeout(Duration::from_secs(15)).map_err(|_| "La souris ne répond pas : actualise avant de réessayer".to_string())?
    }
    pub fn start(&self) {
        let Some(rx) = self.rx.lock().unwrap().take() else { return };
        let weak = Arc::downgrade(&self.shared);
        std::thread::Builder::new()
            .name("logitech-hid".into())
            .spawn(move || {
                let mut mouse: Option<Mouse<Box<dyn Transport>>> = None;
                loop {
                    let Some(shared) = weak.upgrade() else { break };
                    // relecture lente quand la souris répond : détecte un débranchement sans charger le bus HID
                    let wait = if mouse.is_some() { Duration::from_secs(10) } else { Duration::from_millis(100) };
                    let request = match rx.recv_timeout(wait) {
                        Ok(req) => Some(req),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(_) => break,
                    };
                    if request.as_ref().is_some_and(|r| r.deadline <= Instant::now()) {
                        continue;
                    }
                    let result = (|| -> Result<Settings, String> {
                        if mouse.is_none() {
                            let opened = open(shared.simulated);
                            shared.state.lock().unwrap().present = opened.is_ok();
                            mouse = Some(Mouse::new(opened?));
                        }
                        let m = mouse.as_mut().unwrap();
                        match &request {
                            Some(r) if r.action == "setting" || r.action == "settings" => {
                                let key = r.body.get("key").and_then(|v| v.as_str()).ok_or("Clé manquante")?;
                                let value = r.body.get("value").cloned().ok_or("Valeur manquante")?;
                                m.apply(key, &value)
                            }
                            _ => m.read(),
                        }
                    })();
                    // une demande refusée ne fait pas disparaître une souris qui répond encore
                    let recovered = if result.is_err() && request.is_some() { mouse.as_mut().and_then(|m| m.read().ok()) } else { None };
                    {
                        let mut state = shared.state.lock().unwrap();
                        match result.as_ref().ok().cloned().or(recovered) {
                            Some(settings) => {
                                state.settings = Some(settings);
                                state.connected = true;
                                state.error = None;
                            }
                            None => {
                                state.connected = false;
                                state.error = result.as_ref().err().cloned();
                                mouse = None;
                            }
                        }
                        shared.version.fetch_add(1, Ordering::Relaxed);
                    }
                    if let Some(r) = request {
                        let _ = r.answer.send(result.map(|s| json!({"ok": true, "state": s})));
                    }
                    if mouse.is_none() {
                        drop(shared);
                        std::thread::sleep(Duration::from_secs(2));
                    }
                }
            })
            .expect("fil souris logitech");
    }
}

fn open(simulated: bool) -> Result<Box<dyn Transport>, String> {
    if simulated {
        return Ok(Box::new(rustudio_logitech_g::sim::Sim::default()));
    }
    #[cfg(windows)]
    {
        rustudio_logitech_g::Hid::open().map(|h| Box::new(h) as Box<dyn Transport>)
    }
    #[cfg(not(windows))]
    {
        Err("Le module souris Logitech réel nécessite Windows".into())
    }
}
