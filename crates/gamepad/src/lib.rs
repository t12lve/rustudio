//! Manettes XInput (Windows) : état des quatre slots, batterie et vibration, plus un simulateur.
//! Module communautaire (auteur : t12lve), validé sur le simulateur uniquement : aucune manette
//! réelle n'a été branchée pendant le développement. Documentation : docs/GAMEPAD.md.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Result<T> = std::result::Result<T, String>;

/// Les quatre slots XInput (0 à 3).
pub const SLOTS: u32 = 4;
/// Bornes de la durée de vibration acceptée par [`Gamepad::apply`].
pub const VIBRATE_MS_MIN: u32 = 50;
pub const VIBRATE_MS_MAX: u32 = 5000;

// Bits du masque XINPUT_GAMEPAD.wButtons (valeurs XInput, déclarées ici pour rester lisibles hors Windows).
pub const BTN_DPAD_UP: u16 = 0x0001;
pub const BTN_DPAD_DOWN: u16 = 0x0002;
pub const BTN_DPAD_LEFT: u16 = 0x0004;
pub const BTN_DPAD_RIGHT: u16 = 0x0008;
pub const BTN_START: u16 = 0x0010;
pub const BTN_BACK: u16 = 0x0020;
pub const BTN_LEFT_THUMB: u16 = 0x0040;
pub const BTN_RIGHT_THUMB: u16 = 0x0080;
pub const BTN_LEFT_SHOULDER: u16 = 0x0100;
pub const BTN_RIGHT_SHOULDER: u16 = 0x0200;
pub const BTN_A: u16 = 0x1000;
pub const BTN_B: u16 = 0x2000;
pub const BTN_X: u16 = 0x4000;
pub const BTN_Y: u16 = 0x8000;

/// Gâchette XInput (0..255) normalisée en i16 (0..32767), comme les sticks.
pub fn trigger_i16(b: u8) -> i16 {

    ((b as i32 * 32767 + 127) / 255) as i16
}

/// Niveau de batterie XInput (0..3 : vide, faible, moyen, plein) converti en pourcentage affiché.
/// XInput ne donne que quatre paliers ; `None` si le niveau est inconnu.
pub fn battery_level_percent(level: u8) -> Option<u8> {

    match level {

        0 => Some(0),
        1 => Some(25),
        2 => Some(50),
        3 => Some(100),
        _ => None,
    }
}

/// État d'un slot de manette.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PadState {

    pub index: u32,
    pub connected: bool,
    /// Batterie en pourcentage (0..=100) ; `None` si inconnu (pas de batterie interrogée).
    pub battery: Option<u8>,
    /// Câble USB branché : batterie pleine et en charge.
    pub charging: bool,
    /// Masque des boutons (voir les constantes `BTN_*`).
    pub buttons: u16,
    /// LX, LY, RX, RY, LT, RT normalisés en i16 ; LT/RT de 0 à 32767.
    pub axes: [i16; 6],
}

/// Transport manette : le vrai backend XInput, ou le simulateur.
pub trait Pad: Send {

    /// État du slot `index` : `Ok(Some(..))` si une manette répond, `Ok(None)` si le slot est vide,
    /// `Err` si le transport lui-même échoue.
    fn poll(&mut self, index: u32) -> Result<Option<PadState>>;
    /// Vibre `ms` millisecondes sur le slot, moteurs gauche/droite à 0..=65535.
    fn vibrate(&mut self, index: u32, left: u16, right: u16, ms: u32) -> Result<()>;
}

impl<T: Pad> Pad for &mut T {

    fn poll(&mut self, index: u32) -> Result<Option<PadState>> { (**self).poll(index) }
    fn vibrate(&mut self, index: u32, left: u16, right: u16, ms: u32) -> Result<()> { (**self).vibrate(index, left, right, ms) }
}
impl<T: Pad + ?Sized> Pad for Box<T> {

    fn poll(&mut self, index: u32) -> Result<Option<PadState>> { (**self).poll(index) }
    fn vibrate(&mut self, index: u32, left: u16, right: u16, ms: u32) -> Result<()> { (**self).vibrate(index, left, right, ms) }
}

pub struct Gamepad<T: Pad> {

    pad: T,
}

impl<T: Pad> Gamepad<T> {

    pub fn new(t: T) -> Self {

        Self { pad: t }
    }

    /// Les quatre slots dans l'ordre ; un slot vide donne `connected: false`, batterie et axes à zéro.
    pub fn read(&mut self) -> Result<Vec<PadState>> {

        let mut out = Vec::with_capacity(SLOTS as usize);
        for index in 0..SLOTS {

            match self.pad.poll(index)? {

                Some(mut s) => {

                    s.index = index;
                    out.push(s);
                }
                None => out.push(PadState { index, connected: false, battery: None, charging: false, buttons: 0, axes: [0; 6] }),
            }
        }
        Ok(out)
    }

    /// `refresh` renvoie les quatre slots en JSON ; `vibrate` attend
    /// `{"index":0..3,"left":0..=65535,"right":0..=65535,"ms":50..=5000}` sur un slot connecté.
    /// Toute la demande est validée avant d'écrire, et le slot est relu après la vibration :
    /// aucun succès annoncé sans confirmation.
    pub fn apply(&mut self, action: &str, body: &Value) -> Result<Value> {

        match action {

            "refresh" => serde_json::to_value(self.read()?).map_err(|e| e.to_string()),
            "vibrate" => {

                let obj = body.as_object().ok_or("Corps JSON attendu pour « vibrate »")?;
                for key in obj.keys() {

                    if !["index", "left", "right", "ms"].contains(&key.as_str()) {

                        return Err(format!("Champ inconnu « {key} » pour « vibrate »"));
                    }
                }
                let number = |name: &str| obj.get(name).and_then(Value::as_u64).ok_or_else(|| format!("Champ « {name} » : entier attendu"));
                let index = number("index")?;
                if index >= SLOTS as u64 {

                    return Err(format!("Slot invalide : 0 à {}", SLOTS - 1));
                }
                let left = u16::try_from(number("left")?).map_err(|_| "« left » : 0 à 65535 attendu".to_string())?;
                let right = u16::try_from(number("right")?).map_err(|_| "« right » : 0 à 65535 attendu".to_string())?;
                let ms = number("ms")?;
                if !(VIBRATE_MS_MIN as u64..=VIBRATE_MS_MAX as u64).contains(&ms) {

                    return Err(format!("« ms » : durée de {VIBRATE_MS_MIN} à {VIBRATE_MS_MAX} ms attendue"));
                }
                // Validation complète avant toute écriture : le slot doit être occupé.
                match self.pad.poll(index as u32)? {

                    Some(s) if s.connected => {}
                    _ => return Err(format!("Aucune manette connectée sur le slot {index}")),
                }
                self.pad.vibrate(index as u32, left, right, ms as u32)?;
                // Confirmation : la manette répond toujours après l'écriture. XInput ne permet pas de
                // relire l'état des moteurs, la confirmation porte donc sur la réponse du slot.
                match self.pad.poll(index as u32)? {

                    Some(s) if s.connected => {}
                    _ => return Err(format!("Vibration non confirmée : le slot {index} ne répond plus. Réessaie.")),
                }
                Ok(serde_json::json!({ "ok": true, "index": index, "left": left, "right": right, "ms": ms }))
            }
            _ => Err(format!("Action inconnue : « {action} » (refresh, vibrate)")),
        }
    }
}

/// Backend réel : XInput via `xinput1_4.dll` (windows-sys), disponible seulement sous Windows.
#[cfg(windows)]
pub struct Xinput;

#[cfg(windows)]
impl Xinput {

    pub fn new() -> Self {

        Self
    }
}

#[cfg(windows)]
impl Default for Xinput {

    fn default() -> Self {

        Self::new()
    }
}

#[cfg(windows)]
impl Pad for Xinput {

    fn poll(&mut self, index: u32) -> Result<Option<PadState>> {

        use windows_sys::Win32::Foundation::{ERROR_DEVICE_NOT_CONNECTED, ERROR_SUCCESS};
        use windows_sys::Win32::UI::Input::XboxController as xi;
        if index >= SLOTS {

            return Err(format!("Slot invalide : 0 à {}", SLOTS - 1));
        }
        let mut state: xi::XINPUT_STATE = unsafe { std::mem::zeroed() };
        let code = unsafe { xi::XInputGetState(index, &mut state) };
        if code == ERROR_DEVICE_NOT_CONNECTED {

            return Ok(None);
        }
        if code != ERROR_SUCCESS {

            return Err(format!("XInputGetState a échoué sur le slot {index} (code {code})"));
        }
        let g = &state.Gamepad;
        // Batterie : un câble USB donne 100 % et « en charge » ; sur batterie, quatre paliers 0..3.
        // Un échec de lecture de batterie ne bloque pas l'état : batterie inconnue.
        let mut batt: xi::XINPUT_BATTERY_INFORMATION = unsafe { std::mem::zeroed() };
        let (battery, charging) = if unsafe { xi::XInputGetBatteryInformation(index, xi::BATTERY_DEVTYPE_GAMEPAD, &mut batt) } == ERROR_SUCCESS {

            match batt.BatteryType {

                xi::BATTERY_TYPE_WIRED => (Some(100), true),
                xi::BATTERY_TYPE_DISCONNECTED | xi::BATTERY_TYPE_UNKNOWN => (None, false),
                _ => (battery_level_percent(batt.BatteryLevel), false),
            }
        } else {

            (None, false)
        };
        Ok(Some(PadState {

            index,
            connected: true,
            battery,
            charging,
            buttons: g.wButtons,
            axes: [g.sThumbLX, g.sThumbLY, g.sThumbRX, g.sThumbRY, trigger_i16(g.bLeftTrigger), trigger_i16(g.bRightTrigger)],
        }))
    }

    fn vibrate(&mut self, index: u32, left: u16, right: u16, ms: u32) -> Result<()> {

        use windows_sys::Win32::Foundation::{ERROR_DEVICE_NOT_CONNECTED, ERROR_SUCCESS};
        use windows_sys::Win32::UI::Input::XboxController as xi;
        if index >= SLOTS {

            return Err(format!("Slot invalide : 0 à {}", SLOTS - 1));
        }
        let on = xi::XINPUT_VIBRATION { wLeftMotorSpeed: left, wRightMotorSpeed: right };
        let code = unsafe { xi::XInputSetState(index, &on) };
        if code == ERROR_DEVICE_NOT_CONNECTED {

            return Err(format!("Aucune manette sur le slot {index}"));
        }
        if code != ERROR_SUCCESS {

            return Err(format!("XInputSetState a échoué sur le slot {index} (code {code})"));
        }
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
        let off = xi::XINPUT_VIBRATION { wLeftMotorSpeed: 0, wRightMotorSpeed: 0 };
        let code = unsafe { xi::XInputSetState(index, &off) };
        if code != ERROR_SUCCESS {

            return Err(format!("Arrêt de la vibration impossible (code {code}) : les moteurs du slot {index} peuvent rester actifs"));
        }
        Ok(())
    }
}

pub mod sim;
