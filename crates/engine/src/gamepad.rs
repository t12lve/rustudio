//! Manettes XInput : interrogation périodique des 4 slots, cache relu par l'API, vibration sur commande.
use rustudio_gamepad::{Gamepad, Pad, PadState};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub const ID: &str = "gamepad-xinput";

struct Request {
    action: String,
    body: Value,
    deadline: Instant,
    answer: mpsc::SyncSender<Result<Value, String>>,
}
struct Shared {
    pads: Mutex<Vec<PadState>>,
    version: AtomicU64,
    simulated: bool,
    error: Mutex<Option<String>>,
}
pub struct GamepadHub {
    shared: Arc<Shared>,
    tx: mpsc::SyncSender<Request>,
    rx: Mutex<Option<mpsc::Receiver<Request>>>,
}

impl GamepadHub {
    pub fn new(simulated: bool) -> Self {
        let (tx, rx) = mpsc::sync_channel(1);
        let pads = if simulated { Gamepad::new(rustudio_gamepad::sim::Sim::default()).read().unwrap_or_default() } else { Vec::new() };
        Self {
            shared: Arc::new(Shared { pads: Mutex::new(pads), version: AtomicU64::new(0), simulated, error: Mutex::new(None) }),
            tx,
            rx: Mutex::new(Some(rx)),
        }
    }
    pub fn changes(&self) -> u64 {
        self.shared.version.load(Ordering::Relaxed)
    }
    pub fn runtime(&self) -> rustudio_core::modules::ModuleRuntime {
        use rustudio_core::modules::{ModuleRuntime, ModuleStatus};
        let pads = self.shared.pads.lock().unwrap();
        let any = pads.iter().any(|p| p.connected);
        let status = if any { ModuleStatus::Ready } else { ModuleStatus::Disconnected };
        ModuleRuntime {
            id: ID, status, connected: any, simulated: self.shared.simulated,
            firmware: None, firmware_kind: None,
            error: if any { None } else { self.shared.error.lock().unwrap().clone() },
        }
    }
    pub fn state(&self) -> Value {
        let pads = self.shared.pads.lock().unwrap();
        json!({"state": {"pads": *pads}, "v": self.changes()})
    }
    pub fn command(&self, action: &str, body: &Value) -> Result<Value, String> {
        if !matches!(action, "vibrate" | "refresh") {
            return Err("Commande manette inconnue.".into());
        }
        if self.rx.lock().unwrap().is_some() {
            return Err("Le module manette n’est pas démarré".into());
        }
        let (answer, rx) = mpsc::sync_channel(1);
        let req = Request { action: action.into(), body: body.clone(), deadline: Instant::now() + Duration::from_secs(10), answer };
        self.tx.try_send(req).map_err(|_| "Une commande manette est déjà en cours".to_string())?;
        rx.recv_timeout(Duration::from_secs(10)).map_err(|_| "La manette ne répond pas".to_string())?
    }
    pub fn start(&self) {
        let Some(rx) = self.rx.lock().unwrap().take() else { return };
        let weak = Arc::downgrade(&self.shared);
        std::thread::Builder::new()
            .name("xinput".into())
            .spawn(move || {
                let mut pad: Option<Gamepad<Box<dyn Pad>>> = None;
                loop {
                    let Some(shared) = weak.upgrade() else { break };
                    let wait = if pad.is_some() { Duration::from_secs(2) } else { Duration::from_millis(500) };
                    let request = match rx.recv_timeout(wait) {
                        Ok(req) => Some(req),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(_) => break,
                    };
                    if request.as_ref().is_some_and(|r| r.deadline <= Instant::now()) {
                        continue;
                    }
                    if pad.is_none() {
                        pad = Some(Gamepad::new(open(shared.simulated)));
                    }
                    let p = pad.as_mut().unwrap();
                    let result = match &request {
                        Some(r) => p.apply(&r.action, &r.body),
                        None => p.read().map(|pads| json!({"pads": pads})),
                    };
                    {
                        let mut pads = shared.pads.lock().unwrap();
                        match result.as_ref() {
                            Ok(v) => {
                                if let Some(list) = v.get("pads").and_then(|x| serde_json::from_value::<Vec<PadState>>(x.clone()).ok()) {
                                    *pads = list;
                                }
                                *shared.error.lock().unwrap() = None;
                            }
                            Err(e) => {
                                *shared.error.lock().unwrap() = Some(e.clone());
                            }
                        }
                        drop(pads);
                        shared.version.fetch_add(1, Ordering::Relaxed);
                    }
                    if let Some(r) = request {
                        let _ = r.answer.send(result);
                    }
                    drop(shared);
                }
            })
            .expect("fil manette");
    }
}

fn open(simulated: bool) -> Box<dyn Pad> {
    if simulated {
        return Box::new(rustudio_gamepad::sim::Sim::default());
    }
    #[cfg(windows)]
    {
        Box::new(rustudio_gamepad::Xinput::new())
    }
    #[cfg(not(windows))]
    {
        Box::new(rustudio_gamepad::sim::Sim::default())
    }
}
