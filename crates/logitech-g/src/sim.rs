//! Fausse souris Logitech G derrière un récepteur : mêmes rapports 0x10/0x11 de 64 octets, même découverte
//! featureSet, mêmes messages d'erreur HID++ qu'un vrai récepteur. La souris simulée est appairée à l'index 1 ;
//! l'index 0xFF ne répond qu'au ping racine (le récepteur lui-même), tout autre index reste muet.
use crate::*;

/// Features du récepteur simulé : la position dans le tableau est l'index HID++ de la feature.
const FEATURES: [u16; 7] = [0x0000, 0x0001, 0x0005, 0x1001, 0x1000, 0x2201, 0x8060];

#[derive(Clone)]
pub struct Sim {
    pub state: Settings,
    pub disconnected: bool,
    pub writes: usize,
}
impl Default for Sim {
    fn default() -> Self {
        Self {
            state: Settings {
                name: "G Pro X".into(),
                firmware: Some("HID++ 4.5".into()),
                dpi: 1600,
                dpi_list: vec![400, 800, 1600, 3200, 6400, 12800, 25600],
                dpi_max: 25600,
                polling_hz: 1000,
                battery: Some(87),
                charging: false,
            },
            disconnected: false,
            writes: 0,
        }
    }
}

impl Sim {
    /// Réponse de 64 octets qui répercute l'en-tête de la requête, paramètres complétés de zéros.
    fn respond(b: &[u8; 64], params: &[u8]) -> Vec<u8> {
        let mut r = [0u8; 64];
        r[0] = b[0];
        r[1] = b[1];
        r[2] = b[2];
        r[3] = b[3];
        let n = if b[0] == SHORT { 3 } else { 16 };
        let m = params.len().min(n);
        r[4..4 + m].copy_from_slice(&params[..m]);
        r.to_vec()
    }
    /// Message d'erreur HID++ : feature racine (0x00), fonction 2, paramètres [feature demandée, fonction demandée, code].
    fn error(b: &[u8; 64], code: u8) -> Vec<u8> {
        let mut r = Self::respond(b, &[b[2], b[3], code]);
        r[2] = 0;
        r[3] = 0x20 | (b[3] & 0x0F);
        r
    }
}

impl Transport for Sim {
    fn exchange(&mut self, b: &[u8; 64]) -> Result<Vec<u8>> {
        if self.disconnected { return Err(NO_RESPONSE.into()); }
        if b[0] != SHORT && b[0] != LONG { return Err("Report ID invalide".into()); }
        let (device, feature, fs) = (b[1], b[2], b[3]);
        let function = fs >> 4;
        let plen = if b[0] == SHORT { 3 } else { 16 };
        if b[4 + plen..].iter().any(|&x| x != 0) { return Err("Trame invalide : octets après la zone de paramètres non nuls".into()); }
        let params = &b[4..4 + plen];
        if device != 1 {
            // Seul le ping racine répond sur 0xFF (le récepteur) ; ailleurs, silence radio.
            if device == 0xFF && feature == 0 && function == 0 { return Ok(Self::respond(b, &[5, 0, 0])); }
            return Err(NO_RESPONSE.into());
        }
        if feature as usize >= FEATURES.len() { return Ok(Self::error(b, 1)); }
        match (feature, function) {
            (0, 0) => {
                // ping racine : la réponse donne la version mineure pour la majeure demandée
                let minor = match params[0] {
                    4 => 5,
                    2 => 0,
                    _ => return Ok(Self::error(b, 3)),
                };
                Ok(Self::respond(b, &[minor, 0, 0]))
            }
            (0, 1) => {
                // getFeature : index de la feature demandée
                let fid = u16::from_be_bytes([params[0], params[1]]);
                match FEATURES.iter().position(|&f| f == fid) {
                    Some(i) => Ok(Self::respond(b, &[i as u8, 0, 0])),
                    None => Ok(Self::error(b, 1)),
                }
            }
            (1, 0) => Ok(Self::respond(b, &[FEATURES.len() as u8, 0, 0])), // featureSet getCount
            (1, 1) => {
                // featureSet getFeatureID
                let i = params[0] as usize;
                if i < FEATURES.len() {
                    let id = FEATURES[i];
                    Ok(Self::respond(b, &[(id >> 8) as u8, id as u8, 0]))
                } else {
                    Ok(Self::error(b, 3))
                }
            }
            (2, 0) => {
                // deviceFriendlyName getName → [nombre de caractères, caractères UTF-8…]
                let name = self.state.name.as_bytes();
                let mut p = vec![name.len() as u8];
                p.extend_from_slice(name);
                Ok(Self::respond(b, &p))
            }
            (2, 1) => {
                // setName : caractères suivis de zéros
                let end = params.iter().position(|&c| c == 0).unwrap_or(params.len());
                if end > 15 { return Ok(Self::error(b, 3)); }
                let name = std::str::from_utf8(&params[..end]).map_err(|_| "Nom non UTF-8".to_string())?;
                self.state.name = name.to_string();
                self.writes += 1;
                Ok(Self::respond(b, params))
            }
            (3, 0) | (4, 0) => {
                // unifiedBattery / batterie héritée getStatus → [flags (bit 0 = charge), niveau]
                let level = self.state.battery.unwrap_or(0xFF);
                Ok(Self::respond(b, &[self.state.charging as u8, level, 0]))
            }
            (5, 0) => {
                // adjustableDpi getSensorDpiList → u16 BE, sentinelle 0xFFFF (7 valeurs max dans 16 octets)
                let mut p = Vec::new();
                for v in self.state.dpi_list.iter().take(7) { p.extend_from_slice(&v.to_be_bytes()); }
                p.extend_from_slice(&0xFFFFu16.to_be_bytes());
                Ok(Self::respond(b, &p))
            }
            (5, 1) => {
                // getSensorDpi → [capteur, MSB, LSB]
                if params[0] != 0 { return Ok(Self::error(b, 3)); }
                let d = self.state.dpi.to_be_bytes();
                Ok(Self::respond(b, &[0, d[0], d[1]]))
            }
            (5, 2) => {
                // setSensorDpi [capteur, MSB, LSB]
                if params[0] != 0 { return Ok(Self::error(b, 3)); }
                self.state.dpi = u16::from_be_bytes([params[1], params[2]]);
                self.writes += 1;
                Ok(Self::respond(b, params))
            }
            (6, 0) => {
                // adjustableReportRate getReportRateList → intervalles en ms, terminés par 0
                Ok(Self::respond(b, &[1, 2, 4, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]))
            }
            (6, 1) => {
                // getReportRate → [intervalle en ms]
                let interval = hz_to_interval(self.state.polling_hz).ok_or("Fréquence inconnue")?;
                Ok(Self::respond(b, &[interval, 0, 0]))
            }
            (6, 2) => {
                // setReportRate [intervalle en ms]
                let hz = interval_to_hz(params[0]).map_err(|_| "Intervalle refusé".to_string())?;
                self.state.polling_hz = hz;
                self.writes += 1;
                Ok(Self::respond(b, params))
            }
            _ => Ok(Self::error(b, 2)), // fonction inconnue
        }
    }
}
