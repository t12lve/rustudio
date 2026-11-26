//! Faux Vulcan 120 AIMO : mêmes enveloppes et relectures que le matériel, avec ses pièges
//! (trame invalide, commande inconnue, écriture hors limites refusée, clavier débranché).
use crate::*;

#[derive(Clone)]
pub struct Sim {
    pub state: Settings,
    pub disconnected: bool,
    pub writes: usize,
}
impl Default for Sim {
    fn default() -> Self {
        Self { state: Settings { firmware: Some("1.0.7".into()), effect: "wave".into(), brightness: 80, speed: 1, color: [0, 0, 119], per_key: false },
            disconnected: false, writes: 0 }
    }
}

/// Version de firmware de l'état vers les trois octets du rapport d'infos.
fn firmware_bytes(firmware: &Option<String>) -> Vec<u8> {
    match firmware.as_deref() {
        Some(f) => { let mut v: Vec<u8> = f.split('.').map(|p| p.parse().unwrap_or(0)).collect(); v.resize(3, 0); v }
        None => vec![0, 0, 0],
    }
}

impl Transport for Sim {
    fn exchange(&mut self, b: &[u8; REPORT]) -> Result<Vec<u8>> {
        if self.disconnected { return Err("Clavier simulé débranché".into()); }
        let (target, command) = (b[4], b[5]);
        if b[2] != 2 || b[3] > 58 { return Err("Trame invalide".into()); }
        let d = &b[6..6 + b[3] as usize];
        if command < 0x80 {
            // Écriture : une valeur hors limites est refusée sans rien écrire.
            match (target, command) {
                (0, CMD_EFFECT) if d.len() == 1 && (d[0] as usize) < EFFECTS.len() => self.state.effect = EFFECTS[d[0] as usize].into(),
                (0, CMD_BRIGHTNESS) if d.len() == 1 && d[0] <= 100 => self.state.brightness = d[0],
                (0, CMD_SPEED) if d.len() == 1 => self.state.speed = device_to_speed(d[0])?,
                (0, CMD_COLOR) if d.len() == 3 => self.state.color = [d[0], d[1], d[2]],
                (0, CMD_PER_KEY) if d.len() == 1 && d[0] <= 1 => self.state.per_key = d[0] == 1,
                _ => return Err("Commande non simulée".into()),
            }
            self.writes += 1;
        }
        let s = &self.state;
        let data = if command < 0x80 { d.to_vec() } else { match (target, command) {
            (0, c) if c == CMD_INFO | 0x80 => firmware_bytes(&s.firmware),
            (0, c) if c == CMD_EFFECT | 0x80 => vec![EFFECTS.iter().position(|e| *e == s.effect).ok_or("Effet actif inconnu")? as u8],
            (0, c) if c == CMD_BRIGHTNESS | 0x80 => vec![s.brightness],
            (0, c) if c == CMD_SPEED | 0x80 => vec![speed_to_device(s.speed)],
            (0, c) if c == CMD_COLOR | 0x80 => s.color.to_vec(),
            (0, c) if c == CMD_PER_KEY | 0x80 => vec![s.per_key as u8],
            _ => return Err("Lecture non simulée".into()),
        }};
        let mut r = vec![0; 65]; r[1] = 0xa1; r[3] = 2; r[4] = data.len() as u8; r[5] = target; r[6] = command;
        r[7..7 + data.len()].copy_from_slice(&data);
        Ok(r)
    }
}
