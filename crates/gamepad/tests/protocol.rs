use rustudio_gamepad::sim::{Sim, Vibration};
use rustudio_gamepad::{battery_level_percent, trigger_i16, Gamepad, Pad, PadState, Result, SLOTS};
use serde_json::json;

#[test]
fn read_donne_toujours_quatre_slots_dont_un_connecte() {

    let mut sim = Sim::default();
    let pads = Gamepad::new(&mut sim).read().unwrap();
    assert_eq!(pads.len(), SLOTS as usize);
    assert_eq!(pads[0], sim.pad.clone());
    assert!(pads[0].connected);
    assert_eq!(pads[0].battery, Some(87));
    assert!(!pads[0].charging);
    assert_eq!(pads[0].axes.len(), 6);
    for p in &pads[1..] {

        assert_eq!(*p, PadState { index: p.index, connected: false, battery: None, charging: false, buttons: 0, axes: [0; 6] });
    }
}

#[test]
fn refresh_renvoie_les_quatre_slots_en_json() {

    let mut sim = Sim::default();
    let v = Gamepad::new(&mut sim).apply("refresh", &json!({})).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 4);
    assert_eq!(arr[0]["connected"], true);
    assert_eq!(arr[0]["battery"], 87);
    assert_eq!(arr[0]["buttons"], 0x1001);
    assert_eq!(arr[0]["axes"].as_array().unwrap().len(), 6);
    assert_eq!(arr[3]["connected"], false);
    assert_eq!(arr[3]["battery"], serde_json::Value::Null);
}

#[test]
fn vibrate_valide_est_enregistree_par_le_simulateur() {

    let mut sim = Sim::default();
    {
        let mut gp = Gamepad::new(&mut sim);
        let r = gp.apply("vibrate", &json!({"index": 0, "left": 1000, "right": 65535, "ms": 50})).unwrap();
        assert_eq!(r["ok"], true);
        assert_eq!(r["ms"], 50);
        // Bornes acceptées.
        assert!(gp.apply("vibrate", &json!({"index": 0, "left": 0, "right": 0, "ms": 5000})).is_ok());
    }
    assert_eq!(sim.vibrations, vec![Vibration { index: 0, left: 1000, right: 65535, ms: 50 }, Vibration { index: 0, left: 0, right: 0, ms: 5000 }]);
}

#[test]
fn valeurs_dangereuses_refusees_avant_toute_ecriture() {

    let mut sim = Sim::default();
    let before = sim.pad.clone();
    {
        let mut gp = Gamepad::new(&mut sim);
        for body in [
        json!({"index": 0, "left": 0, "right": 0, "ms": 10}),       // ms trop court
        json!({"index": 0, "left": 0, "right": 0, "ms": 99999}),    // ms trop long
        json!({"index": 0, "left": 0, "right": 0, "ms": 49}),
        json!({"index": 0, "left": 0, "right": 0, "ms": 5001}),
        json!({"index": 9, "left": 0, "right": 0, "ms": 50}),       // slot inexistant
        json!({"index": 4, "left": 0, "right": 0, "ms": 50}),
        json!({"index": 1, "left": 0, "right": 0, "ms": 50}),       // slot vide
        json!({"index": 3, "left": 1, "right": 1, "ms": 100}),      // slot vide
        json!({"index": 0, "left": 65536, "right": 0, "ms": 50}),   // moteur hors plage
        json!({"index": 0, "left": 0, "right": -1, "ms": 50}),
        json!({"index": 0, "left": 0, "right": 0}),                 // ms absent
        json!({"left": 0, "right": 0, "ms": 50}),                   // index absent
        json!({"index": 0.5, "left": 0, "right": 0, "ms": 50}),
        json!({"index": 0, "left": 0, "right": 0, "ms": 50, "vitesse": 9}), // champ inconnu
        json!([1, 2]),                                              // corps pas un objet
    ] {

        assert!(gp.apply("vibrate", &body).is_err(), "{body}");
        }
        assert!(gp.apply("explose", &json!({})).is_err());
        // L'état n'a pas bougé non plus.
        assert_eq!(gp.read().unwrap()[0], before);
    }
    assert!(sim.vibrations.is_empty(), "aucune vibration écrite sur des demandes refusées");
}

#[test]
fn sim_deconnectee_plus_aucune_manette_ni_vibration() {

    let mut sim = Sim { disconnected: true, ..Sim::default() };
    {
        let mut gp = Gamepad::new(&mut sim);
        let pads = gp.read().unwrap();
        assert_eq!(pads.len(), 4);
        assert!(pads.iter().all(|p| !p.connected && p.battery.is_none()));
        assert!(gp.apply("vibrate", &json!({"index": 0, "left": 1, "right": 1, "ms": 50})).is_err());
    }
    assert!(sim.vibrations.is_empty());
}

#[test]
fn slot_hors_plage_refuse_par_le_transport() {

    let mut sim = Sim::default();
    assert!(sim.poll(SLOTS).is_err());
    assert!(sim.vibrate(SLOTS, 0, 0, 50).is_err());
    assert!(sim.poll(SLOTS - 1).unwrap().is_none(), "seul le slot 0 est peuplé");
}

#[test]
fn echec_du_transport_pas_de_faux_succes() {

    // XInputSetState en panne : l'erreur remonte, aucun succès.
    struct SetStateEnPanne(Sim);
    impl Pad for SetStateEnPanne {

        fn poll(&mut self, index: u32) -> Result<Option<PadState>> { self.0.poll(index) }
        fn vibrate(&mut self, _index: u32, _left: u16, _right: u16, _ms: u32) -> Result<()> {

            Err("XInputSetState a échoué (code 1167)".into())
        }
    }
    let mut gp = Gamepad::new(SetStateEnPanne(Sim::default()));
    assert!(gp.apply("vibrate", &json!({"index": 0, "left": 1, "right": 1, "ms": 50})).is_err());

    // Le récepteur lâche pendant la vibration : l'écriture a eu lieu mais n'est pas confirmée.
    struct DebranchePendantVibration<'a>(&'a mut Sim);
    impl Pad for DebranchePendantVibration<'_> {

        fn poll(&mut self, index: u32) -> Result<Option<PadState>> { self.0.poll(index) }
        fn vibrate(&mut self, index: u32, left: u16, right: u16, ms: u32) -> Result<()> {

            self.0.vibrate(index, left, right, ms)?;
            self.0.disconnected = true;
            Ok(())
        }
    }
    let mut sim = Sim::default();
    {
        let mut gp = Gamepad::new(DebranchePendantVibration(&mut sim));
        assert!(gp.apply("vibrate", &json!({"index": 0, "left": 500, "right": 500, "ms": 50})).is_err(), "pas de succès sans relecture conforme");
    }
    assert_eq!(sim.vibrations.len(), 1, "l'écriture a bien été reçue par le simulateur");
    assert!(sim.disconnected);
}

#[test]
fn normalisations_et_serde() {

    assert_eq!(trigger_i16(0), 0);
    assert_eq!(trigger_i16(255), 32767);
    assert!(trigger_i16(200) > trigger_i16(100));
    assert_eq!(battery_level_percent(0), Some(0));
    assert_eq!(battery_level_percent(1), Some(25));
    assert_eq!(battery_level_percent(2), Some(50));
    assert_eq!(battery_level_percent(3), Some(100));
    assert_eq!(battery_level_percent(9), None);

    let s = Sim::default().pad.clone();
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(serde_json::from_value::<PadState>(v.clone()).unwrap(), s);
    assert_eq!(v["index"], 0);
    assert_eq!(v["axes"][4], 32767, "LT normalisée 0..32767");
}
