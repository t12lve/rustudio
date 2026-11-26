use rustudio_logitech_g::{frame, payload, sim::Sim, Mouse, Transport, LONG, NO_RESPONSE, RECEIVER_PIDS, SHORT, VID};
use serde_json::json;

#[test]
fn lecture_complete_sur_simulateur() {
    let s = Mouse::new(Sim::default()).read().unwrap();
    assert_eq!(s.name, "G Pro X");
    assert_eq!(s.firmware.as_deref(), Some("HID++ 4.5"));
    assert_eq!(s.dpi, 1600);
    assert_eq!(s.dpi_list, vec![400, 800, 1600, 3200, 6400, 12800, 25600]);
    assert_eq!(s.dpi_max, 25600);
    assert_eq!(s.polling_hz, 1000);
    assert_eq!(s.battery, Some(87));
    assert!(!s.charging);
    // Settings aller-retour JSON (sérialisation du moteur)
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(serde_json::from_value::<rustudio_logitech_g::Settings>(v.clone()).unwrap(), s);
    assert_eq!(v["dpi"], json!(1600));
    assert_eq!(VID, 0x046D);
    assert_eq!(RECEIVER_PIDS, [0xC547]);
}

#[test]
fn apply_ecrit_une_fois_et_confirme_par_relecture() {
    let mut sim = Sim::default();
    {
        let mut mouse = Mouse::new(&mut sim);
        for (key, value) in [("dpi", json!(3200)), ("polling_hz", json!(500)), ("dpi", json!(25600)), ("polling_hz", json!(125)), ("dpi", json!(1600))] {
            let after = mouse.apply(key, &value).unwrap();
            assert_eq!(serde_json::to_value(&after).unwrap()[key], value, "{key}");
        }
        assert_eq!(mouse.read().unwrap().dpi, 1600, "dernier DPI appliqué = 1600");
        assert_eq!(mouse.read().unwrap().polling_hz, 125);
    }
    assert_eq!(sim.writes, 5, "une seule écriture par apply");
    assert_eq!(sim.state.dpi, 1600);
    assert_eq!(sim.state.polling_hz, 125);
}

#[test]
fn valeurs_dangereuses_et_cles_inconnues_refusees_sans_rien_ecrire() {
    let mut sim = Sim::default();
    let before;
    {
        let mut mouse = Mouse::new(&mut sim);
        before = mouse.read().unwrap();
        for (key, value) in [
            ("dpi", json!(30000)), ("dpi", json!(25601)), ("dpi", json!(0)), ("dpi", json!(1234)), ("dpi", json!(-1)),
            ("dpi", json!("1600")), ("dpi", json!(1.5)), ("dpi", json!(70000)),
            ("polling_hz", json!(2000)), ("polling_hz", json!(8000)), ("polling_hz", json!(0)), ("polling_hz", json!(100)),
            ("polling_hz", json!("1000")), ("polling_hz", json!(true)),
            ("name", json!("pirate")), ("firmware", json!(1)), ("battery", json!(50)), ("charging", json!(true)), ("", json!(1)),
        ] {
            assert!(mouse.apply(key, &value).is_err(), "{key} = {value} aurait dû être refusé");
        }
        assert_eq!(mouse.read().unwrap(), before, "état inchangé après les refus");
    }
    assert_eq!(sim.writes, 0, "aucune écriture pour des valeurs refusées");
}

#[test]
fn trames_tronquees_ou_corrompues_refusees() {
    let mut sim = Sim::default();
    let q = frame(SHORT, 1, 0, 0, 1, &[4, 0, 0]).unwrap(); // ping racine
    let r = sim.exchange(&q).unwrap();
    assert_eq!(r.len(), 64);
    assert_eq!(payload(&r, &q).unwrap(), &[5, 0, 0]);
    // Windows peut préfixer le report ID 0 à la réponse
    let mut r65 = vec![0u8];
    r65.extend_from_slice(&r);
    assert_eq!(payload(&r65, &q).unwrap(), &[5, 0, 0]);
    for len in 0..7 { assert!(payload(&r[..len], &q).is_err(), "réponse tronquée à {len} octets"); }
    // en-tête corrompu : report ID, device index, feature, fonction/swid
    for i in [0, 1, 2, 3] {
        let mut bad = r.clone();
        bad[i] ^= 1;
        assert!(payload(&bad, &q).is_err(), "octet d'en-tête {i} corrompu");
    }
    // réponse à une autre requête (rapport long, autre feature/fonction)
    let other = frame(LONG, 1, 5, 1, 1, &[0, 0, 0]).unwrap();
    let r2 = sim.exchange(&other).unwrap();
    assert!(payload(&r2, &q).is_err());
    // construction : report ID inconnu, trop de paramètres, fonction/swid hors 4 bits
    assert!(frame(0x12, 1, 0, 0, 1, &[0; 3]).is_err());
    assert!(frame(SHORT, 1, 0, 0, 1, &[0; 4]).is_err(), "4 octets de paramètres en rapport court");
    assert!(frame(LONG, 1, 0, 0, 1, &[0; 17]).is_err(), "17 octets de paramètres en rapport long");
    assert!(frame(SHORT, 1, 0, 16, 1, &[0; 3]).is_err(), "fonction sur 4 bits");
    assert!(frame(SHORT, 1, 0, 0, 16, &[0; 3]).is_err(), "swid sur 4 bits");
    assert!(frame(LONG, 1, 0, 0, 1, &[0; 16]).is_ok());
    // un message d'erreur HID++ (feature 0, fonction 2) n'est pas une réponse valide pour payload
    let bad_feature = frame(SHORT, 1, 42, 0, 1, &[0, 0, 0]).unwrap();
    let err = sim.exchange(&bad_feature).unwrap();
    assert_eq!((err[2], err[3] >> 4), (0, 2), "le sim répond une erreur HID++ pour une feature inconnue");
    assert!(payload(&err, &bad_feature).is_err());
}

#[test]
fn sim_debranchee_renvoie_une_erreur() {
    let mut sim = Sim { disconnected: true, ..Sim::default() };
    let q = frame(SHORT, 1, 0, 0, 1, &[4, 0, 0]).unwrap();
    assert_eq!(sim.exchange(&q).unwrap_err(), NO_RESPONSE);
    assert_eq!(Mouse::new(sim).read().unwrap_err(), NO_RESPONSE);
}

#[test]
fn ecriture_ignoree_ne_donne_pas_de_faux_succes() {
    struct Ignore(Sim);
    impl Transport for Ignore {
        fn exchange(&mut self, b: &[u8; 64]) -> rustudio_logitech_g::Result<Vec<u8>> {
            let write = b[3] >> 4 >= 2; // fonctions 2+ = écritures
            let old = self.0.state.clone();
            let r = self.0.exchange(b);
            if write { self.0.state = old; } // le matériel accuse mais ignore l'écriture
            r
        }
    }
    let mut t = Ignore(Sim::default());
    let mut mouse = Mouse::new(&mut t);
    let err = mouse.apply("dpi", &json!(3200)).unwrap_err();
    assert!(err.contains("non confirmée"), "{err}");
    assert!(mouse.apply("polling_hz", &json!(500)).is_err());
    assert_eq!(mouse.read().unwrap().dpi, 1600, "la valeur lue n'a pas changé");
    assert_eq!(mouse.read().unwrap().polling_hz, 1000);
}

#[test]
fn batterie_heritee_0x1000_quand_unified_0x1001_ne_repond_plus() {
    // Index HID++ de la feature 0x1001 dans le simulateur, lu via root getFeature.
    let mut probe = Sim::default();
    let q = frame(SHORT, 1, 0, 1, 1, &[0x10, 0x01, 0]).unwrap();
    let unified_idx = payload(&probe.exchange(&q).unwrap(), &q).unwrap()[0];
    struct SansUnified { sim: Sim, unified_idx: u8 }
    impl Transport for SansUnified {
        fn exchange(&mut self, b: &[u8; 64]) -> rustudio_logitech_g::Result<Vec<u8>> {
            if b[1] == 1 && b[2] == self.unified_idx { return Err("feature 0x1001 muette".into()); }
            self.sim.exchange(b)
        }
    }
    let mut sim = Sim::default();
    sim.state.charging = true;
    let mut t = SansUnified { sim, unified_idx };
    let s = Mouse::new(&mut t).read().unwrap();
    assert_eq!(s.battery, Some(87), "repli sur la batterie héritée 0x1000");
    assert!(s.charging);
}

#[test]
fn ping_racine_et_index_0xff_du_recepteur() {
    let mut sim = Sim::default();
    // le récepteur lui-même répond au ping racine sur 0xFF
    let q = frame(SHORT, 0xFF, 0, 0, 1, &[4, 0, 0]).unwrap();
    assert_eq!(payload(&sim.exchange(&q).unwrap(), &q).unwrap(), &[5, 0, 0]);
    // mais aucune feature ne répond sur 0xFF, et aucun autre appareil que l'index 1
    let q = frame(SHORT, 0xFF, 5, 1, 1, &[0, 0, 0]).unwrap();
    assert!(sim.exchange(&q).is_err());
    let q = frame(SHORT, 2, 0, 0, 1, &[4, 0, 0]).unwrap();
    assert_eq!(sim.exchange(&q).unwrap_err(), NO_RESPONSE);
    // ping de la version 2.0 : mineure 0
    let q = frame(SHORT, 1, 0, 0, 1, &[2, 0, 0]).unwrap();
    assert_eq!(payload(&sim.exchange(&q).unwrap(), &q).unwrap(), &[0, 0, 0]);
}
