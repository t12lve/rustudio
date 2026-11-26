//! ROCCAT Vulcan 120 AIMO (USB 1E7D:3098) : éclairage AIMO en HID brut, rapports de 64 octets
//! précédés du report ID 0 (65 octets sur le fil), sur le canal vendor du clavier.
//! Documentation du protocole : docs/ROCCAT-VULCAN.md. Aucun firmware n'est écrit ni exécuté.
//! Module communautaire (auteur t12lve), validé sur simulateur uniquement.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Result<T> = std::result::Result<T, String>;
pub const VID: u16 = 0x1E7D;
pub const PID: u16 = 0x3098;
/// Le clavier ne répond pas sur son canal de commande.
pub const NO_RESPONSE: &str = "Le Vulcan 120 AIMO ne répond pas (veille ou câble débranché)";
/// Rapport HID de 64 octets, précédé du report ID 0 dans les appels Windows.
pub const REPORT: usize = 64;

pub trait Transport: Send {
    fn exchange(&mut self, request: &[u8; REPORT]) -> Result<Vec<u8>>;
}
impl<T: Transport> Transport for &mut T {
    fn exchange(&mut self, request: &[u8; REPORT]) -> Result<Vec<u8>> { (**self).exchange(request) }
}
impl<T: Transport + ?Sized> Transport for Box<T> {
    fn exchange(&mut self, request: &[u8; REPORT]) -> Result<Vec<u8>> { (**self).exchange(request) }
}

/// Cible 0 : le clavier entier.
pub const TARGET_KEYBOARD: u8 = 0;
/// Écriture de la luminosité 0..=100 (octet de commande 0x05 ; lecture = 0x85).
pub const CMD_BRIGHTNESS: u8 = 0x05;
/// Écriture de la couleur de l'effet, R,V,B (0x0A ; lecture = 0x8A).
pub const CMD_COLOR: u8 = 0x0A;
/// Écriture de la vitesse de l'effet matériel (0x0D ; lecture = 0x8D).
pub const CMD_SPEED: u8 = 0x0D;
/// Écriture de l'effet actif (0x13 ; lecture = 0x93).
pub const CMD_EFFECT: u8 = 0x13;
/// Écriture du mode par touche (0x15 ; lecture = 0x95).
pub const CMD_PER_KEY: u8 = 0x15;
/// Lecture seule des infos, dont la version de firmware (0x0F ; lecture = 0x8F).
pub const CMD_INFO: u8 = 0x0F;

/// Effets reconnus, dans l'ordre de leurs codes sur le fil.
pub const EFFECTS: &[&str] = &["off", "wave", "breathe", "reactive", "custom"];

/// Vitesse demandée (0..=3) vers l'octet du rapport « wave » du pilote Linux (plage réelle 1..=11).
pub fn speed_to_device(speed: u8) -> u8 { [1, 4, 8, 11][speed.min(3) as usize] }
/// Octet lu sur le fil vers la vitesse normalisée ; refuse toute valeur hors plage connue.
pub fn device_to_speed(raw: u8) -> Result<u8> {
    [1u8, 4, 8, 11].iter().position(|&v| v == raw).map(|i| i as u8).ok_or_else(|| "Vitesse lue inconnue".to_string())
}

pub fn frame(target: u8, command: u8, payload: &[u8]) -> Result<[u8; REPORT]> {
    if payload.len() > 58 { return Err("Commande HID trop longue".into()); }
    let mut b = [0; REPORT];
    b[2] = 2; b[3] = payload.len() as u8; b[4] = target; b[5] = command;
    b[6..6 + payload.len()].copy_from_slice(payload);
    Ok(b)
}

/// Normalise le report ID Windows et valide l'enveloppe avant toute lecture.
pub fn payload<'a>(reply: &'a [u8], request: &[u8; REPORT]) -> Result<&'a [u8]> {
    let r = if reply.len() == 65 && reply[0] == 0 { &reply[1..] } else { reply };
    if r.len() < 6 || ![0xa1, 0x02].contains(&r[0]) || r[2] != request[2] || r[4] != request[4] || r[5] != request[5] {
        return Err("Réponse HID inattendue (commande ou appareil différent)".into());
    }
    let n = r[3] as usize;
    if n == 0 || n > 58 || r.len() < 6 + n { return Err("Réponse HID tronquée".into()); }
    Ok(&r[6..6 + n])
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub firmware: Option<String>,
    pub effect: String,
    pub brightness: u8,
    pub speed: u8,
    pub color: [u8; 3],
    pub per_key: bool,
}

pub struct Keyboard<T: Transport> { transport: T }
impl<T: Transport> Keyboard<T> {
    pub fn new(transport: T) -> Self { Self { transport } }
    fn query(&mut self, command: u8, body: &[u8]) -> Result<Vec<u8>> {
        let request = frame(TARGET_KEYBOARD, command, body)?;
        let reply = self.transport.exchange(&request).map_err(|e| format!("{e} (commande {command:02x})"))?;
        Ok(payload(&reply, &request)?.to_vec())
    }
    fn value(&mut self, command: u8) -> Result<u8> {
        self.query(command, &[0])?.first().copied().ok_or("Valeur HID absente".into())
    }
    pub fn read(&mut self) -> Result<Settings> {
        let info = self.query(CMD_INFO | 0x80, &[0])?;
        if info.len() < 3 { return Err("Version de firmware absente".into()); }
        let firmware = Some(format!("{}.{}.{}", info[0], info[1], info[2]));
        let effect_code = self.value(CMD_EFFECT | 0x80)?;
        let effect = EFFECTS.get(effect_code as usize).ok_or("Effet actif inconnu")?.to_string();
        let brightness = self.value(CMD_BRIGHTNESS | 0x80)?;
        if brightness > 100 { return Err("Luminosité illisible".into()); }
        let speed = device_to_speed(self.value(CMD_SPEED | 0x80)?)?;
        let color_data = self.query(CMD_COLOR | 0x80, &[0])?;
        if color_data.len() < 3 { return Err("Couleur illisible".into()); }
        let color = [color_data[0], color_data[1], color_data[2]];
        let per_key = match self.value(CMD_PER_KEY | 0x80)? { 0 => false, 1 => true, _ => return Err("Mode par touche inconnu".into()) };
        Ok(Settings { firmware, effect, brightness, speed, color, per_key })
    }
    /// Validation complète avant toute écriture : une valeur hors limites ne part jamais sur le fil.
    fn command_for(key: &str, value: &Value) -> Result<(u8, Vec<u8>)> {
        let number = || value.as_u64().ok_or_else(|| "Valeur entière attendue".to_string());
        match key {
            "effect" => {
                let name = value.as_str().ok_or("Nom d'effet attendu")?;
                let code = EFFECTS.iter().position(|e| *e == name).ok_or("Effet inconnu")? as u8;
                Ok((CMD_EFFECT, vec![code]))
            }
            "brightness" => { let n = number()?; if n > 100 { return Err("Luminosité autorisée : 0 à 100".into()); } Ok((CMD_BRIGHTNESS, vec![n as u8])) }
            "speed" => { let n = number()?; if n > 3 { return Err("Vitesse autorisée : 0 à 3".into()); } Ok((CMD_SPEED, vec![speed_to_device(n as u8)])) }
            "color" => {
                let arr = value.as_array().ok_or("Couleur [r,g,b] attendue")?;
                if arr.len() != 3 { return Err("Couleur [r,g,b] attendue".into()); }
                let mut c = [0u8; 3];
                for (i, v) in arr.iter().enumerate() {
                    let n = v.as_u64().ok_or("Composante couleur entière attendue")?;
                    if n > 255 { return Err("Composante couleur hors limites (0 à 255)".into()); }
                    c[i] = n as u8;
                }
                Ok((CMD_COLOR, c.to_vec()))
            }
            "per_key" => { let on = value.as_bool().ok_or("Booléen attendu")?; Ok((CMD_PER_KEY, vec![on as u8])) }
            _ => Err("Réglage inconnu".into()),
        }
    }
    /// Une seule écriture, puis relecture complète : rien n'est annoncé réussi sans confirmation.
    /// Un ACK perdu n'est pas fatal si la valeur est relue telle quelle.
    pub fn apply(&mut self, key: &str, value: &Value) -> Result<Settings> {
        let (command, bytes) = Self::command_for(key, value)?;
        let sent = self.query(command, &bytes);
        let after = self.read().map_err(|e| format!("Enregistrement non confirmé : {e}. Actualise avant de réessayer."))?;
        let actual = serde_json::to_value(&after).map_err(|e| e.to_string())?;
        if actual[key] != *value {
            return Err(format!("Enregistrement non confirmé : {key} non relu tel quel (ACK {}). Actualise avant de réessayer.", if sent.is_ok() { "reçu" } else { "absent" }));
        }
        Ok(after)
    }
}

#[cfg(windows)]
pub struct Hid { device: hidapi::HidDevice }
#[cfg(windows)]
impl Hid {
    /// Canal vendor du clavier (usage page FF00 préféré, interface 1 du pilote Linux) ;
    /// l'interface clavier n'est jamais ouverte.
    pub fn open() -> Result<Self> {
        let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
        let mut devices: Vec<_> = api.device_list().filter(|d| d.vendor_id() == VID && d.product_id() == PID).collect();
        devices.sort_by_key(|d| (d.usage_page() != 0xFF00, d.interface_number() != 1));
        for info in devices {
            if let Ok(device) = info.open_device(&api) {
                if std::env::var_os("RUSTUDIO_HID_TRACE").is_some() { eprintln!("PID {:04x}, usage {:04x}, interface {}", info.product_id(), info.usage(), info.interface_number()); }
                return Ok(Self { device });
            }
        }
        Err("Vulcan 120 AIMO absent : branche son câble USB".into())
    }
}
#[cfg(windows)]
impl Transport for Hid {
    fn exchange(&mut self, request: &[u8; REPORT]) -> Result<Vec<u8>> {
        let mut report = [0; 65]; report[1..].copy_from_slice(request);
        // Les lectures perdues peuvent être renvoyées ; jamais les écritures.
        let attempts = if request[5] & 0x80 != 0 { 3 } else { 1 };
        for _ in 0..attempts {
            self.device.send_feature_report(&report).map_err(|_| "Écriture HID impossible".to_string())?;
            std::thread::sleep(std::time::Duration::from_millis(150));
            for _ in 0..8 {
                let mut response = [0; 65];
                let n = self.device.get_feature_report(&mut response).map_err(|_| "Lecture HID impossible".to_string())?;
                if payload(&response[..n], request).is_ok() { return Ok(response[..n].to_vec()); }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        Err(NO_RESPONSE.into())
    }
}

pub mod sim;
