use goxlr_ipc::{DaemonCommand, DaemonRequest, DaemonResponse, DaemonStatus, GoXLRCommand};
use serde_json::Value;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

#[derive(Default, Clone)]
pub struct State {
    pub status: Option<Value>,
    pub connected: bool,
    pub error: String,
    pub confirmation: String,
    pub revision: u64,
}
pub enum Action {
    Device(String, Value),
    Daemon(Value),
    Stop,
}
pub struct Backend {
    pub state: Arc<Mutex<State>>,
    pub tx: mpsc::Sender<Action>,
}

pub fn device_request(serial: String, mut value: Value) -> Result<DaemonRequest, String> {
    // Use the daemon enum itself to serialize zero-field variants correctly.
    for name in [
        "SaveProfile",
        "SaveMicProfile",
        "SaveActivePreset",
        "ReloadSettings",
        "ClearSampleProcessError",
    ] {
        if let Some(args) = value.get_mut(name) {
            if args.as_array().is_some_and(|args| args.is_empty()) {
                let command = match name {
                    "SaveProfile" => GoXLRCommand::SaveProfile(),
                    "SaveMicProfile" => GoXLRCommand::SaveMicProfile(),
                    "SaveActivePreset" => GoXLRCommand::SaveActivePreset(),
                    "ReloadSettings" => GoXLRCommand::ReloadSettings(),
                    "ClearSampleProcessError" => GoXLRCommand::ClearSampleProcessError(),
                    _ => unreachable!(),
                };
                return Ok(DaemonRequest::Command(serial, command));
            }
        }
    }
    serde_json::from_value::<GoXLRCommand>(value)
        .map(|command| DaemonRequest::Command(serial, command))
        .map_err(|error| format!("Commande invalide : {error}"))
}
pub fn daemon_request(value: Value) -> Result<DaemonRequest, String> {
    serde_json::from_value::<DaemonCommand>(value)
        .map(DaemonRequest::Daemon)
        .map_err(|error| format!("Commande système invalide : {error}"))
}
fn execute(agent: &ureq::Agent, request: DaemonRequest) -> Result<(), String> {
    execute_at(agent, "http://127.0.0.1:14564/api/command", request)
}
fn execute_at(agent: &ureq::Agent, url: &str, request: DaemonRequest) -> Result<(), String> {
    let response: DaemonResponse = agent
        .post(url)
        .send_json(serde_json::to_value(request).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| format!("Réponse du moteur invalide : {e}"))?;
    match response {
        DaemonResponse::Ok => Ok(()),
        DaemonResponse::Error(error) => Err(error),
        _ => Err("Réponse inattendue du moteur".into()),
    }
}
impl Backend {
    pub fn start(context: eframe::egui::Context) -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let shared = state.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let agent = ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(3))
                .build();
            let mut last = Instant::now() - Duration::from_secs(2);
            loop {
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(Action::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Ok(action) => {
                        let request = match action {
                            Action::Device(serial, value) => device_request(serial, value),
                            Action::Daemon(value) => daemon_request(value),
                            Action::Stop => unreachable!(),
                        };
                        let result = request.and_then(|request| execute(&agent, request));
                        let mut state = shared.lock().unwrap();
                        match result {
                            Ok(()) => {
                                state.error.clear();
                                state.confirmation = "Commande confirmée par le moteur".into();
                            }
                            Err(error) => {
                                state.error = error;
                                state.confirmation.clear();
                            }
                        }
                        last = Instant::now() - Duration::from_secs(2);
                        context.request_repaint();
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if last.elapsed() >= Duration::from_millis(350) {
                    let result = agent
                        .get("http://127.0.0.1:14564/api/get-devices")
                        .call()
                        .map_err(|e| e.to_string())
                        .and_then(|response| {
                            response
                                .into_json::<DaemonStatus>()
                                .map_err(|e| e.to_string())
                        });
                    let mut state = shared.lock().unwrap();
                    match result {
                        Ok(status) => {
                            if !state.connected {
                                state.error.clear();
                            }
                            state.status = serde_json::to_value(status).ok();
                            state.connected = true;
                            state.revision += 1;
                        }
                        Err(error) => {
                            state.connected = false;
                            state.error = format!("Moteur déconnecté : {error}");
                        }
                    }
                    last = Instant::now();
                    context.request_repaint();
                }
            }
        });
        Self { state, tx }
    }
    pub fn snapshot(&self) -> State {
        self.state.lock().unwrap().clone()
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        let _ = self.tx.send(Action::Stop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn uses_real_typed_protocol() {
        let request = device_request("MINI".into(), json!({"SetVolume":["Mic",128]})).unwrap();
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({"Command":["MINI",{"SetVolume":["Mic",128]}]})
        );
        assert!(device_request("MINI".into(), json!({"SetVolume":["Mic",999]})).is_err());
        assert!(device_request("MINI".into(), json!({"InventedCommand":true})).is_err());
        assert!(device_request("MINI".into(), json!({"SaveProfile":[]})).is_ok());
        assert!(device_request("MINI".into(), json!({"SetCompressorReleaseTime":12})).is_ok());
        assert!(daemon_request(json!("StopDaemon")).is_ok());
        assert!(daemon_request(json!({"StopDaemon":[]})).is_err());
    }
    #[test]
    fn every_effect_control_serializes_to_the_real_daemon_enum() {
        let schema: Value = serde_json::from_str(include_str!("../assets/effects.json")).unwrap();
        for effect in schema.as_array().unwrap() {
            for style in effect["styles"].as_array().unwrap() {
                let mut command = serde_json::Map::new();
                command.insert(
                    effect["styleCommand"].as_str().unwrap().into(),
                    style[0].clone(),
                );
                device_request("DEVICE".into(), Value::Object(command)).unwrap();
            }
            for param in effect["params"].as_array().unwrap() {
                let value = param["value"].clone();
                let mut prefix = param["prefix"].as_array().cloned().unwrap_or_default();
                let args = if prefix.is_empty() {
                    value
                } else {
                    prefix.push(value);
                    json!(prefix)
                };
                let mut command = serde_json::Map::new();
                command.insert(param["command"].as_str().unwrap().into(), args);
                device_request("DEVICE".into(), Value::Object(command)).unwrap();
            }
        }
    }
    #[test]
    fn http_execution_requires_a_daemon_acknowledgment() {
        use std::io::{Read, Write};
        for (reply, accepted) in [("\"Ok\"", true), ("{\"Error\":\"Refus matériel\"}", false)] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let reply = reply.as_bytes().to_vec();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = vec![];
                loop {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    bytes.push(byte[0]);
                    if bytes.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let header = String::from_utf8(bytes).unwrap();
                let length: usize = header
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse().unwrap())
                    })
                    .unwrap();
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                let request: DaemonRequest = serde_json::from_slice(&body).unwrap();
                assert!(matches!(
                    request,
                    DaemonRequest::Command(_, GoXLRCommand::SaveProfile())
                ));
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",reply.len()).unwrap();
                stream.write_all(&reply).unwrap();
            });
            let result = execute_at(
                &ureq::AgentBuilder::new()
                    .timeout(Duration::from_secs(2))
                    .build(),
                &format!("http://{address}/api/command"),
                device_request("DEVICE".into(), json!({"SaveProfile":[]})).unwrap(),
            );
            assert_eq!(result.is_ok(), accepted);
            server.join().unwrap();
        }
    }
}
