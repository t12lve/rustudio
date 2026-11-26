//! Fausse manette XInput : une manette sur le slot 0 (batterie 87 %), et les pièges du vrai matériel
//! (slot vide, récepteur débranché, vibration refusée sur un slot sans manette).
use crate::*;

/// Une vibration appliquée, enregistrée par le simulateur.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vibration {

    pub index: u32,
    pub left: u16,
    pub right: u16,
    pub ms: u32,
}

#[derive(Clone)]
pub struct Sim {

    /// État de la manette du slot 0.
    pub pad: PadState,
    /// Récepteur débranché : plus aucune manette ne répond, comme ERROR_DEVICE_NOT_CONNECTED.
    pub disconnected: bool,
    /// Vibrations appliquées, dans l'ordre. Le simulateur ne dort pas : il enregistre seulement.
    pub vibrations: Vec<Vibration>,
}

impl Default for Sim {

    fn default() -> Self {

        Self {

            pad: PadState {

                index: 0,
                connected: true,
                battery: Some(87),
                charging: false,
                buttons: BTN_A | BTN_DPAD_UP,
                // LX, LY, RX, RY, LT, RT : stick gauche penché, gâchette gauche à fond.
                axes: [12000, -8000, 0, 0, 32767, 0],
            },
            disconnected: false,
            vibrations: Vec::new(),
        }
    }
}

impl Pad for Sim {

    fn poll(&mut self, index: u32) -> Result<Option<PadState>> {

        if index >= SLOTS {

            return Err(format!("Slot invalide : 0 à {}", SLOTS - 1));
        }
        if self.disconnected || index != 0 {

            return Ok(None);
        }
        Ok(Some(self.pad.clone()))
    }

    fn vibrate(&mut self, index: u32, left: u16, right: u16, ms: u32) -> Result<()> {

        if index >= SLOTS {

            return Err(format!("Slot invalide : 0 à {}", SLOTS - 1));
        }
        // Comme XInputSetState sur un slot vide : ERROR_DEVICE_NOT_CONNECTED.
        if self.disconnected || index != 0 {

            return Err(format!("Aucune manette sur le slot {index} (ERROR_DEVICE_NOT_CONNECTED)"));
        }
        self.vibrations.push(Vibration { index, left, right, ms });
        Ok(())
    }
}
