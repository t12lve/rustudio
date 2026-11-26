//! Cartes réseau Windows : état relu périodiquement (lecture seule riche), actions via netsh/ipconfig
//! sur liste blanche, avec relecture de confirmation. Aucune commande fabriquée hors de cette liste.
use rustudio_netcard::{Adapter, Net, Probe};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub const ID: &str = "netcard";

struct Request {
    action: String,
    body: Value,
    deadline: Instant,
    answer: mpsc::SyncSender<Result<Value, String>>,
}
struct Shared {
    adapters: Mutex<Vec<Adapter>>,
    version: AtomicU64,
    simulated: bool,
    error: Mutex<Option<String>>,
}
pub struct NetcardHub {
    shared: Arc<Shared>,
    tx: mpsc::SyncSender<Request>,
    rx: Mutex<Option<mpsc::Receiver<Request>>>,
}

impl NetcardHub {
    pub fn new(simulated: bool) -> Self {
        let (tx, rx) = mpsc::sync_channel(1);
        let adapters = if simulated { Net::new(rustudio_netcard::sim::Sim::default()).read().unwrap_or_default() } else { Vec::new() };
        Self {
            shared: Arc::new(Shared { adapters: Mutex::new(adapters), version: AtomicU64::new(0), simulated, error: Mutex::new(None) }),
            tx,
            rx: Mutex::new(Some(rx)),
        }
    }
    pub fn changes(&self) -> u64 {
        self.shared.version.load(Ordering::Relaxed)
    }
    pub fn runtime(&self) -> rustudio_core::modules::ModuleRuntime {
        use rustudio_core::modules::{ModuleRuntime, ModuleStatus};
        let adapters = self.shared.adapters.lock().unwrap();
        // au moins une carte physique "Up" : le module est prêt ; ce n'est pas un appareil USB
        let up = adapters.iter().any(|a| a.status == "Up");
        let status = if up { ModuleStatus::Ready } else if adapters.is_empty() { ModuleStatus::Disconnected } else { ModuleStatus::Error };
        ModuleRuntime {
            id: ID, status, connected: up, simulated: self.shared.simulated,
            firmware: None, firmware_kind: None,
            error: if up { None } else { self.shared.error.lock().unwrap().clone() },
        }
    }
    pub fn state(&self) -> Value {
        let adapters = self.shared.adapters.lock().unwrap();
        json!({"state": {"adapters": *adapters}, "v": self.changes()})
    }
    pub fn command(&self, action: &str, body: &Value) -> Result<Value, String> {
        if !matches!(action, "enable" | "disable" | "renew" | "refresh") {
            return Err("Commande carte réseau inconnue.".into());
        }
        if self.rx.lock().unwrap().is_some() {
            return Err("Le module réseau n’est pas démarré".into());
        }
        let (answer, rx) = mpsc::sync_channel(1);
        let req = Request { action: action.into(), body: body.clone(), deadline: Instant::now() + Duration::from_secs(30), answer };
        self.tx.try_send(req).map_err(|_| "Une commande réseau est déjà en cours".to_string())?;
        rx.recv_timeout(Duration::from_secs(30)).map_err(|_| "La carte réseau ne répond pas".to_string())?
    }
    pub fn start(&self) {
        let Some(rx) = self.rx.lock().unwrap().take() else { return };
        let weak = Arc::downgrade(&self.shared);
        std::thread::Builder::new()
            .name("netcard".into())
            .spawn(move || {
                let mut net: Option<Net<Box<dyn Probe>>> = None;
                loop {
                    let Some(shared) = weak.upgrade() else { break };
                    let wait = if net.is_some() { Duration::from_secs(5) } else { Duration::from_millis(500) };
                    let request = match rx.recv_timeout(wait) {
                        Ok(req) => Some(req),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(_) => break,
                    };
                    if request.as_ref().is_some_and(|r| r.deadline <= Instant::now()) {
                        continue;
                    }
                    if net.is_none() {
                        net = Some(Net::new(open(shared.simulated)));
                    }
                    let n = net.as_mut().unwrap();
                    let result = match &request {
                        Some(r) => n.apply(&r.action, &r.body),
                        None => n.read().map(|a| json!({"adapters": a})),
                    };
                    {
                        let mut adapters = shared.adapters.lock().unwrap();
                        match result.as_ref() {
                            Ok(v) => {
                                if let Some(list) = v.get("adapters").and_then(|x| serde_json::from_value::<Vec<Adapter>>(x.clone()).ok()) {
                                    *adapters = list;
                                }
                                *shared.error.lock().unwrap() = None;
                            }
                            Err(e) => {
                                *shared.error.lock().unwrap() = Some(e.clone());
                            }
                        }
                        drop(adapters);
                        shared.version.fetch_add(1, Ordering::Relaxed);
                    }
                    if let Some(r) = request {
                        let _ = r.answer.send(result);
                    }
                    drop(shared);
                }
            })
            .expect("fil réseau");
    }
}

fn open(simulated: bool) -> Box<dyn Probe> {
    if simulated {
        return Box::new(rustudio_netcard::sim::Sim::default());
    }
    #[cfg(windows)]
    {
        Box::new(rustudio_netcard::PowerShell::new())
    }
    #[cfg(not(windows))]
    {
        Box::new(rustudio_netcard::sim::Sim::default())
    }
}
