use rustudio_roccat_vulcan::{frame, payload, sim::Sim, Keyboard, Transport, CMD_BRIGHTNESS, CMD_INFO, EFFECTS, REPORT};
use serde_json::json;

#[test]
fn lecture_complete_sur_simulateur() {
    let s = Keyboard::new(Sim::default()).read().unwrap();
    assert_eq!(s.firmware.as_deref(), Some("1.0.7"));
    assert_eq!(s.effect, "wave");
    assert_eq!(s.brightness, 80);
    assert_eq!(s.speed, 1);
    assert_eq!(s.color, [0, 0, 119]);
    assert!(!s.per_key);
}

#[test]
fn chaque_reglage_est_ecrit_puis_confirme_par_relecture() {
    let mut kb = Keyboard::new(Sim::default());
    for (key, value) in [("effect", json!("breathe")), ("brightness", json!(100)), ("speed", json!(3)),
        ("color", json!([10, 20, 30])), ("per_key", json!(true))] {
        let after = kb.apply(key, &value).unwrap();
        assert_eq!(serde_json::to_value(&after).unwrap()[key], value, "{key}");
    }
    for effect in EFFECTS {
        let after = kb.apply("effect", &json!(effect)).unwrap();
        assert_eq!(after.effect, *effect);
    }
    let s = kb.read().unwrap();
    assert_eq!((s.effect.as_str(), s.brightness, s.speed, s.color, s.per_key), ("custom", 100, 3, [10, 20, 30], true));
}

#[test]
fn valeurs_dangereuses_refusees_avant_toute_ecriture() {
    let mut sim = Sim::default();
    let before = sim.state.clone();
    let writes = sim.writes;
    let mut kb = Keyboard::new(&mut sim);
    for (key, value) in [("brightness", json!(101)), ("speed", json!(9)), ("color", json!([300, 0, 0])),
        ("color", json!([1, 2])), ("color", json!("rouge")), ("color", json!([-1, 0, 0])),
        ("brightness", json!(-1)), ("effect", json!("disco")), ("per_key", json!(1)),
        ("dpi", json!(800)), ("firmware", json!("9.9.9"))] {
        assert!(kb.apply(key, &value).is_err(), "{key} {value}");
    }
    assert_eq!(sim.writes, writes, "aucune écriture pour une valeur refusée");
    assert_eq!(sim.state, before, "l'état n'a pas bougé");
}

#[test]
fn trames_corrompues_ou_etrangeres_refusees() {
    let mut sim = Sim::default();
    let q = frame(0, CMD_INFO | 0x80, &[0]).unwrap();
    let r = sim.exchange(&q).unwrap();
    assert_eq!(payload(&r, &q).unwrap(), &[1, 0, 7]);
    assert_eq!(payload(&r[1..], &q).unwrap(), &[1, 0, 7], "report ID 0 retiré");
    for len in 0..9 { assert!(payload(&r[..len], &q).is_err(), "len {len}"); }
    for i in [1, 3, 5, 6] { let mut bad = r.clone(); bad[i] ^= 1; assert!(payload(&bad, &q).is_err(), "octet {i}"); }
    let other = frame(0, CMD_BRIGHTNESS | 0x80, &[0]).unwrap();
    assert!(payload(&r, &other).is_err(), "réponse d'une autre commande refusée");
    assert!(frame(0, 0, &[0; 59]).is_err(), "charge utile trop longue");
    // l'écho d'une écriture est validé pareil
    let w = frame(0, CMD_BRIGHTNESS, &[50]).unwrap();
    let rw = sim.exchange(&w).unwrap();
    assert_eq!(payload(&rw, &w).unwrap(), &[50]);
}

#[test]
fn clavier_debranche_ne_repond_plus() {
    let sim = Sim { disconnected: true, ..Sim::default() };
    let mut kb = Keyboard::new(sim);
    assert!(kb.read().is_err());
    assert!(kb.apply("brightness", &json!(42)).is_err());
}

#[test]
fn ecriture_ignoree_ne_donne_pas_de_faux_succes() {
    struct Ignore(Sim);
    impl Transport for Ignore {
        fn exchange(&mut self, b: &[u8; REPORT]) -> rustudio_roccat_vulcan::Result<Vec<u8>> {
            let old = self.0.state.clone();
            let r = self.0.exchange(b);
            if b[5] < 0x80 { self.0.state = old; } // l'écriture est perdue après son accusé
            r
        }
    }
    let mut kb = Keyboard::new(Ignore(Sim::default()));
    assert!(kb.apply("brightness", &json!(42)).is_err(), "non confirmé par relecture");
    assert_eq!(kb.read().unwrap().brightness, 80);
}

#[test]
fn accuse_perdu_mais_reglage_confirme_par_relecture() {
    struct LostAck(Sim);
    impl Transport for LostAck {
        fn exchange(&mut self, b: &[u8; REPORT]) -> rustudio_roccat_vulcan::Result<Vec<u8>> {
            let r = self.0.exchange(b);
            if b[5] < 0x80 { return Err("Accusé perdu après écriture".into()); }
            r
        }
    }
    let after = Keyboard::new(LostAck(Sim::default())).apply("speed", &json!(2)).unwrap();
    assert_eq!(after.speed, 2);
}
