//! Souris Logitech G derrière un récepteur LIGHTSPEED/Unifying (VID 046D, PID C547) : protocole HID++ 2.0/4.5
//! sur rapports feature HID de 64 octets (report ID 0x10 court / 0x11 long).
//! Documentation du protocole : docs/LOGITECH-G.md. Aucun firmware n'est écrit.
//! Module communautaire (auteur t12lve), validé sur simulateur uniquement : aucun essai sur matériel réel.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Result<T> = std::result::Result<T, String>;
pub const VID: u16 = 0x046D;
/// Récepteurs LIGHTSPEED/Unifying pris en charge (interface HID++ uniquement, jamais les interfaces souris/clavier).
pub const RECEIVER_PIDS: [u16; 1] = [0xC547];
/// Récepteur présent mais souris muette : veille, liaison perdue ou souris absente du récepteur.
pub const NO_RESPONSE: &str = "La souris Logitech G ne répond pas (récepteur débranché, souris en veille ou liaison perdue)";

/// Rapport HID++ court : 3 octets de paramètres.
pub const SHORT: u8 = 0x10;
/// Rapport HID++ long : 16 octets de paramètres.
pub const LONG: u8 = 0x11;
/// Software ID émis dans chaque requête (4 bits bas de l'octet fonction) et exigé en écho.
const SWID: u8 = 0x01;

// Features HID++ utilisées (la feature racine 0x0000 a toujours l'index 0).
const F_FEATURE_SET: u16 = 0x0001; // featureSet : getCount, getFeatureID
const F_FRIENDLY_NAME: u16 = 0x0005; // deviceFriendlyName : getName, setName
const F_BATTERY_UNIFIED: u16 = 0x1001; // unifiedBattery : getStatus
const F_BATTERY_LEGACY: u16 = 0x1000; // batterie héritée (repli) : getStatus
const F_DPI: u16 = 0x2201; // adjustableDpi : getSensorDpiList, getSensorDpi, setSensorDpi
const F_REPORT_RATE: u16 = 0x8060; // adjustableReportRate : getReportRateList, getReportRate, setReportRate

/// Fréquences d'interrogation acceptées, en Hz.
pub const POLLING_RATES: [u16; 4] = [125, 250, 500, 1000];

pub trait Transport {
    /// Envoie un rapport feature de 64 octets (report ID en tête) et renvoie la réponse reçue.
    fn exchange(&mut self, request: &[u8; 64]) -> Result<Vec<u8>>;
}
impl<T: Transport> Transport for &mut T {
    fn exchange(&mut self, request: &[u8; 64]) -> Result<Vec<u8>> { (**self).exchange(request) }
}
impl<T: Transport + ?Sized> Transport for Box<T> {
    fn exchange(&mut self, request: &[u8; 64]) -> Result<Vec<u8>> { (**self).exchange(request) }
}

/// Trame HID++ de 64 octets : [report ID][device index][feature index][fonction<<4 | swid][paramètres…][zéros].
pub fn frame(report: u8, device: u8, feature: u8, function: u8, swid: u8, params: &[u8]) -> Result<[u8; 64]> {
    let max = match report {
        SHORT => 3,
        LONG => 16,
        _ => return Err(format!("Report ID {report:02x} inconnu (0x10 court ou 0x11 long)")),
    };
    if function > 0x0F { return Err("Numéro de fonction sur 4 bits (0 à 15)".into()); }
    if swid > 0x0F { return Err("Software ID sur 4 bits (0 à 15)".into()); }
    if params.len() > max { return Err(format!("Commande HID++ trop longue : {} octets pour un rapport {report:02x}", params.len())); }
    let mut b = [0u8; 64];
    b[0] = report;
    b[1] = device;
    b[2] = feature;
    b[3] = (function << 4) | swid;
    b[4..4 + params.len()].copy_from_slice(params);
    Ok(b)
}

/// Normalise la réponse (Windows peut préfixer le report ID 0) et valide l'écho strict de l'en-tête :
/// report ID, device index, feature, fonction et swid doivent être identiques à la requête.
/// Renvoie la zone de paramètres (3 octets en rapport court, 16 en rapport long).
pub fn payload<'a>(reply: &'a [u8], request: &[u8; 64]) -> Result<&'a [u8]> {
    let r = if reply.len() == 65 && reply[0] == 0 { &reply[1..] } else { reply };
    if r.len() < 4 { return Err("Réponse HID++ tronquée".into()); }
    if r[0] != request[0] || (r[0] != SHORT && r[0] != LONG) { return Err("Report ID de la réponse inattendu".into()); }
    if r[1] != request[1] { return Err("Réponse d'un autre appareil (device index différent)".into()); }
    if r[2] != request[2] { return Err("Réponse d'une autre feature HID++".into()); }
    if r[3] != request[3] { return Err("Réponse d'une autre fonction ou d'un autre software ID".into()); }
    let n = if r[0] == SHORT { 3 } else { 16 };
    if r.len() < 4 + n { return Err("Réponse HID++ tronquée".into()); }
    Ok(&r[4..4 + n])
}

/// Message d'erreur HID++ : feature racine (0x00), fonction 2, paramètres [feature demandée, fonction demandée, code].
fn hidpp_error(reply: &[u8], request: &[u8; 64]) -> Option<String> {
    let r = if reply.len() == 65 && reply[0] == 0 { &reply[1..] } else { reply };
    if r.len() < 7 || r[0] != request[0] || r[1] != request[1] || r[2] != 0 || (r[3] & 0xF0) != 0x20 || (r[3] & 0x0F) != (request[3] & 0x0F) {
        return None;
    }
    if request[2] == 0 && (request[3] & 0xF0) == 0x20 { return None; }
    Some(format!("Erreur HID++ {:02x} du périphérique (feature {:02x}, fonction {:02x})", r[6], r[4], r[5]))
}
/// La réponse est-elle un écho valide de la requête (ou un message d'erreur HID++ reconnaissable) ?
#[cfg(windows)]
fn echo_ok(reply: &[u8], request: &[u8; 64]) -> bool {
    payload(reply, request).is_ok() || hidpp_error(reply, request).is_some()
}

/// Intervalle de rapport (ms) → fréquence (Hz) : 1 ms = 1 000 Hz, 2 ms = 500 Hz, 4 ms = 250 Hz, 8 ms = 125 Hz.
fn interval_to_hz(interval: u8) -> Result<u16> {
    match interval {
        1 => Ok(1000),
        2 => Ok(500),
        4 => Ok(250),
        8 => Ok(125),
        _ => Err(format!("Intervalle de rapport inconnu : {interval} ms")),
    }
}
fn hz_to_interval(hz: u16) -> Option<u8> {
    match hz {
        1000 => Some(1),
        500 => Some(2),
        250 => Some(4),
        125 => Some(8),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub name: String,
    /// Version du protocole HID++ négociée par le ping racine (ex. « HID++ 4.5 ») ; le firmware de la souris
    /// (feature 0x0003) n'est pas lu.
    pub firmware: Option<String>,
    pub dpi: u16,
    pub dpi_list: Vec<u16>,
    pub dpi_max: u16,
    pub polling_hz: u16,
    pub battery: Option<u8>,
    pub charging: bool,
}

pub struct Mouse<T: Transport> {
    transport: T,
    /// Device index de la souris appairée (0 = pas encore détecté).
    device: u8,
    /// feature ID → index HID++ de la feature, découverte à chaque lecture.
    features: std::collections::BTreeMap<u16, u8>,
}

impl<T: Transport> Mouse<T> {
    pub fn new(transport: T) -> Self {
        Self { transport, device: 0, features: Default::default() }
    }

    fn call(&mut self, report: u8, feature: u8, function: u8, params: &[u8]) -> Result<Vec<u8>> {
        let request = frame(report, self.device, feature, function, SWID, params)?;
        let reply = self.transport.exchange(&request).map_err(|e| format!("{e} (appareil {}, feature {feature:02x}, fonction {function:02x})", self.device))?;
        if let Some(err) = hidpp_error(&reply, &request) { return Err(err); }
        Ok(payload(&reply, &request)?.to_vec())
    }

    /// Sonde les device index 1 à 6 (souris appairées) puis 0xFF (récepteur lui-même) par un ping racine.
    fn detect(&mut self) -> Result<u8> {
        let mut receiver = false;
        for idx in (1..=6u8).chain([0xFF]) {
            let req = frame(SHORT, idx, 0, 0, SWID, &[4, 0, 0])?;
            if let Ok(reply) = self.transport.exchange(&req) {
                if payload(&reply, &req).is_ok() {
                    if idx == 0xFF { receiver = true; } else { return Ok(idx); }
                }
            }
        }
        if receiver { Err("Le récepteur Logitech répond mais aucune souris n'y est appairée".into()) } else { Err(NO_RESPONSE.into()) }
    }

    /// Indexe les features via featureSet (0x0001) : getCount puis getFeatureID pour chaque index.
    fn discover(&mut self) -> Result<std::collections::BTreeMap<u16, u8>> {
        let fs = match self.call(SHORT, 0, 1, &[(F_FEATURE_SET >> 8) as u8, F_FEATURE_SET as u8, 0]) {
            Ok(r) => *r.first().ok_or("Index featureSet illisible")?,
            Err(_) => return Err("FeatureSet (0x0001) absente : ce n'est pas un périphérique HID++ 2.0".into()),
        };
        let count = self.call(SHORT, fs, 0, &[0, 0, 0])?.first().copied().ok_or("Nombre de features illisible")? as usize;
        let mut map = std::collections::BTreeMap::new();
        for i in 0..count.min(255) {
            if let Ok(r) = self.call(SHORT, fs, 1, &[i as u8, 0, 0]) {
                if r.len() >= 2 { map.insert(u16::from_be_bytes([r[0], r[1]]), i as u8); }
            }
        }
        Ok(map)
    }

    fn read_name(&mut self, feature: u8) -> Result<String> {
        let r = self.call(LONG, feature, 0, &[0; 16])?;
        let len = *r.first().ok_or("Nom illisible")? as usize;
        if len > 15 || r.len() < 1 + len { return Err("Nom du périphérique tronqué".into()); }
        String::from_utf8(r[1..1 + len].to_vec()).map_err(|_| "Nom du périphérique en UTF-8 invalide".to_string())
    }

    /// Batterie via unifiedBattery (0x1001), avec repli sur la batterie héritée (0x1000) si la feature
    /// est absente ou muette. getStatus → [flags (bit 0 = charge en cours), niveau 0 à 100].
    fn read_battery(&mut self) -> Result<(Option<u8>, bool)> {
        for fid in [F_BATTERY_UNIFIED, F_BATTERY_LEGACY] {
            if let Some(&f) = self.features.get(&fid) {
                if let Ok(r) = self.call(SHORT, f, 0, &[0, 0, 0]) {
                    if r.len() >= 2 {
                        let charging = r[0] & 0x01 != 0;
                        let level = r[1];
                        return Ok((if level <= 100 { Some(level) } else { None }, charging));
                    }
                }
            }
        }
        Ok((None, false))
    }

    pub fn read(&mut self) -> Result<Settings> {
        if self.device == 0 { self.device = self.detect()?; }
        // Ping racine : la réponse donne la version mineure du protocole pour la majeure demandée (4 → 4.5).
        let ping = self.call(SHORT, 0, 0, &[4, 0, 0])?;
        let minor = *ping.first().ok_or("Réponse ping vide")?;
        let firmware = (minor > 0).then(|| format!("HID++ 4.{minor}"));

        self.features = self.discover()?;

        let name = match self.features.get(&F_FRIENDLY_NAME) {
            Some(&f) => self.read_name(f)?,
            None => String::new(),
        };

        let dpi_feature = *self.features.get(&F_DPI).ok_or("Feature adjustableDpi (0x2201) absente : souris non prise en charge")?;
        let raw = self.call(LONG, dpi_feature, 0, &[0; 16])?;
        let mut dpi_list = Vec::new();
        for c in raw.as_chunks::<2>().0 {
            let v = u16::from_be_bytes(*c);
            if v == 0 || v == 0xFFFF { break; } // sentinelle de fin de liste
            dpi_list.push(v);
        }
        if dpi_list.is_empty() { return Err("Liste DPI vide".into()); }
        let dpi_max = *dpi_list.iter().max().unwrap();
        let cur = self.call(SHORT, dpi_feature, 1, &[0, 0, 0])?;
        if cur.len() < 3 || cur[0] != 0 { return Err("Capteur DPI illisible".into()); }
        let dpi = u16::from_be_bytes([cur[1], cur[2]]);

        let rate_feature = *self.features.get(&F_REPORT_RATE).ok_or("Feature adjustableReportRate (0x8060) absente : souris non prise en charge")?;
        let interval = *self.call(SHORT, rate_feature, 1, &[0, 0, 0])?.first().ok_or("Fréquence illisible")?;
        let polling_hz = interval_to_hz(interval)?;

        let (battery, charging) = self.read_battery()?;

        Ok(Settings { name, firmware, dpi, dpi_list, dpi_max, polling_hz, battery, charging })
    }

    /// Applique un réglage ("dpi" ou "polling_hz") : validation complète avant écriture, une seule écriture,
    /// puis relecture complète et confirmation de la valeur. Jamais de succès sans confirmation.
    pub fn apply(&mut self, key: &str, value: &Value) -> Result<Settings> {
        let before = self.read()?;
        let (feature, params, expected): (u8, Vec<u8>, u64) = match key {
            "dpi" => {
                let n = value.as_u64().ok_or("DPI : valeur entière attendue")?;
                let dpi = u16::try_from(n).map_err(|_| format!("DPI {n} hors plage (0 à 65535)"))?;
                if dpi == 0 || dpi > before.dpi_max || !before.dpi_list.contains(&dpi) {
                    return Err(format!("DPI {dpi} refusé : choisis une valeur de la liste DPI (jusqu'à {})", before.dpi_max));
                }
                let f = *self.features.get(&F_DPI).ok_or("Feature adjustableDpi (0x2201) absente")?;
                (f, vec![0, (dpi >> 8) as u8, dpi as u8], n)
            }
            "polling_hz" => {
                let n = value.as_u64().ok_or("Fréquence : valeur entière attendue")?;
                let hz = u16::try_from(n).map_err(|_| format!("Fréquence {n} hors plage (0 à 65535)"))?;
                let interval = hz_to_interval(hz).ok_or("Fréquence refusée : 125, 250, 500 ou 1000 Hz uniquement")?;
                let f = *self.features.get(&F_REPORT_RATE).ok_or("Feature adjustableReportRate (0x8060) absente")?;
                (f, vec![interval, 0, 0], n)
            }
            _ => return Err(format!("Réglage inconnu : {key} (acceptés : dpi, polling_hz)")),
        };
        // Écriture (fonction 2 : setSensorDpi / setReportRate), envoyée une seule fois.
        self.call(SHORT, feature, 2, &params)?;
        let after = self.read()?;
        let confirmed = match key {
            "dpi" => after.dpi as u64 == expected,
            _ => after.polling_hz as u64 == expected,
        };
        if !confirmed {
            let lu = if key == "dpi" { after.dpi.to_string() } else { after.polling_hz.to_string() };
            return Err(format!("Écriture de « {key} » non confirmée par relecture (valeur lue : {lu})"));
        }
        Ok(after)
    }
}

#[cfg(windows)]
pub struct Hid {
    device: hidapi::HidDevice,
}
#[cfg(windows)]
impl Hid {
    /// Interface HID++ du récepteur (usage page FF00) ; les interfaces souris, clavier et notification du
    /// récepteur ne sont jamais ouvertes. Aucun pilote supplémentaire n'est installé.
    pub fn open() -> Result<Self> {
        let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
        let mut devices: Vec<_> = api.device_list()
            .filter(|d| d.vendor_id() == VID && RECEIVER_PIDS.contains(&d.product_id()) && d.usage_page() == 0xFF00)
            .collect();
        devices.sort_by_key(|d| d.interface_number());
        for info in devices {
            if let Ok(device) = info.open_device(&api) {
                if std::env::var_os("RUSTUDIO_HID_TRACE").is_some() {
                    eprintln!("Récepteur PID {:04x}, usage {:04x}, interface {}", info.product_id(), info.usage(), info.interface_number());
                }
                return Ok(Self { device });
            }
        }
        Err("Récepteur Logitech (LIGHTSPEED/Unifying) absent : branche-le sur un port USB".into())
    }
}
#[cfg(windows)]
impl Transport for Hid {
    fn exchange(&mut self, request: &[u8; 64]) -> Result<Vec<u8>> {
        // hidapi Windows veut le report ID en tête du tampon : [report ID][64 octets de rapport].
        let mut out = [0u8; 65];
        out[0] = request[0];
        out[1..64].copy_from_slice(&request[1..]);
        // Les fonctions 2 et plus sont des écritures : jamais renvoyées automatiquement.
        let is_write = request[3] >> 4 >= 2;
        let attempts = if is_write { 1 } else { 3 };
        for _ in 0..attempts {
            self.device.send_feature_report(&out).map_err(|_| "Écriture HID impossible".to_string())?;
            std::thread::sleep(std::time::Duration::from_millis(30));
            for _ in 0..8 {
                let mut resp = [0u8; 65];
                resp[0] = request[0];
                let n = self.device.get_feature_report(&mut resp).map_err(|_| "Lecture HID impossible".to_string())?;
                if n >= 4 && echo_ok(&resp[..n], request) { return Ok(resp[..n].to_vec()); }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        Err(NO_RESPONSE.into())
    }
}

pub mod sim;
