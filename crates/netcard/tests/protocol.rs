use rustudio_netcard::sim::Sim;
use rustudio_netcard::{parse_adapters, parse_speed, Adapter, Net, Probe, Result};
use serde_json::json;

#[test]
fn read_sim_renvoie_trois_cartes_avec_mac_ip_dns() {
    let mut net = Net::new(Sim::default());
    let adapters = net.read().unwrap();
    assert_eq!(adapters.len(), 3);
    for a in &adapters {
        assert!(!a.name.is_empty() && !a.description.is_empty());
        assert_eq!(a.mac.split(':').count(), 6, "MAC de {}", a.name);
    }
    let qnap = &adapters[0];
    assert_eq!(qnap.name, "Qnap-10G");
    assert_eq!(qnap.mac, "24:5E:BE:69:E4:49");
    assert_eq!(qnap.status, "Up");
    assert_eq!(qnap.speed_bps, 10_000_000_000);
    assert_eq!(qnap.ips, ["192.168.1.31"]);
    assert_eq!(qnap.gateway, ["192.168.1.1"]);
    assert_eq!(qnap.dns, ["192.168.1.1", "1.1.1.1"]);
    assert!(qnap.dhcp);
    // toute carte « Up » a au moins une IP et un DNS
    for a in adapters.iter().filter(|a| a.status == "Up") {
        assert!(!a.ips.is_empty() && !a.dns.is_empty(), "{} sans IP/DNS", a.name);
    }
    // la carte débranchée n'a rien
    let eth = adapters.iter().find(|a| a.name == "Ethernet").unwrap();
    assert_eq!(eth.status, "Disconnected");
    assert!(eth.ips.is_empty() && eth.gateway.is_empty() && eth.dns.is_empty());
}

#[test]
fn enable_disable_renew_sont_confirmes_par_relecture() {
    let mut sim = Sim::default();
    let mut net = Net::new(&mut sim);
    net.read().unwrap();

    let r = net.apply("disable", &json!({"name": "Qnap-10G"})).unwrap();
    assert_eq!(r["action"], "disable");
    assert_eq!(r["status"], "Disconnected");
    assert_eq!(r["adapters"].as_array().unwrap().len(), 3);
    assert_eq!(net.read().unwrap().iter().find(|a| a.name == "Qnap-10G").unwrap().status, "Disconnected");

    let r = net.apply("enable", &json!({"name": "Qnap-10G"})).unwrap();
    assert_eq!(r["status"], "Up");
    assert_eq!(net.read().unwrap().iter().find(|a| a.name == "Qnap-10G").unwrap().status, "Up");

    let r = net.apply("renew", &json!({"name": "Qnap-10G"})).unwrap();
    assert_eq!(r["action"], "renew");
    assert_eq!(r["status"], "Up");

    let r = net.apply("refresh", &json!({})).unwrap();
    assert_eq!(r["adapters"].as_array().unwrap().len(), 3);

    assert_eq!(sim.log, [
        "netsh interface set interface name=Qnap-10G admin=disabled".to_string(),
        "netsh interface set interface name=Qnap-10G admin=enabled".to_string(),
        "ipconfig /renew Qnap-10G".to_string(),
    ]);
}

#[test]
fn apply_sans_read_prealable_fait_le_releve_d_abord() {
    let mut net = Net::new(Sim::default());
    let r = net.apply("renew", &json!({"name": "Qnap-10G"})).unwrap();
    assert_eq!(r["status"], "Up");
    let err = net.apply("enable", &json!({"name": "Inconnue"})).unwrap_err();
    assert!(err.contains("absente"), "{err}");
}

#[test]
fn nom_inconnu_refuse_sans_aucune_commande() {
    let mut sim = Sim::default();
    let mut net = Net::new(&mut sim);
    net.read().unwrap();
    let err = net.apply("enable", &json!({"name": "CarteFantome"})).unwrap_err();
    assert!(err.contains("absente"), "{err}");
    assert!(sim.log.is_empty(), "aucune commande exécutée pour un nom inconnu : {:?}", sim.log);
}

#[test]
fn netsh_en_echec_renvoie_une_erreur_sans_faux_succes() {
    let mut sim = Sim { fail_netsh: true, ..Sim::default() };
    let mut net = Net::new(&mut sim);
    net.read().unwrap();
    let err = net.apply("disable", &json!({"name": "Qnap-10G"})).unwrap_err();
    assert!(err.contains("échoué"), "{err}");
    // l'état réel n'a pas changé et aucun succès n'est annoncé
    assert_eq!(net.read().unwrap().iter().find(|a| a.name == "Qnap-10G").unwrap().status, "Up");
    assert!(sim.log.iter().any(|l| l.contains("admin=disabled")));
}

#[test]
fn commande_sans_effet_reel_est_refusee_par_la_relecture() {
    // netsh répond OK mais n'applique rien : la relecture doit refuser le succès
    struct Noop(Sim);
    impl Probe for Noop {
        fn adapters(&mut self) -> Result<Vec<Adapter>> { self.0.adapters() }
        fn exec(&mut self, program: &str, args: &[&str]) -> Result<String> {
            self.0.log.push(format!("{program} {}", args.join(" ")));
            Ok(String::new())
        }
    }
    let mut net = Net::new(Noop(Sim::default()));
    net.read().unwrap();
    let err = net.apply("disable", &json!({"name": "Qnap-10G"})).unwrap_err();
    assert!(err.contains("non confirmé"), "{err}");
}

#[test]
fn renew_sur_carte_desactivee_echoue() {
    let mut net = Net::new(Sim::default());
    net.read().unwrap();
    net.apply("disable", &json!({"name": "Wi-Fi"})).unwrap();
    let err = net.apply("renew", &json!({"name": "Wi-Fi"})).unwrap_err();
    assert!(err.contains("échoué") || err.contains("non confirmé"), "{err}");
}

#[test]
fn actions_et_noms_dangereux_sont_refuses() {
    let mut sim = Sim::default();
    let mut net = Net::new(&mut sim);
    net.read().unwrap();
    for (action, body) in [
        ("wipe", json!({"name": "Qnap-10G"})),
        ("enable", json!({})),
        ("enable", json!({"name": 42})),
        ("enable", json!("Qnap-10G")),
        ("enable", json!({"name": ""})),
        ("enable", json!({"name": "Qnap\"; del *"})),
        ("enable", json!({"name": "Q\nnap"})),
        ("enable", json!({"name": "x".repeat(200)})),
    ] {
        assert!(net.apply(action, &body).is_err(), "{action} {body}");
    }
    assert!(sim.log.is_empty(), "aucune commande partie pour des entrées refusées : {:?}", sim.log);
}

#[test]
fn parsing_json_powershell_tableau_ou_objet_unique() {
    // ConvertTo-Json déballe un élément unique en objet nu
    let un = r#"{"name":"Solo","description":"d","mac":"00:11:22:33:44:55","status":"Up","link_speed":"1 Gbps","if_type":"6","ips":["10.0.0.2"],"gateway":["10.0.0.1"],"dns":["10.0.0.1"],"dhcp":false}"#;
    let a = parse_adapters(un).unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].name, "Solo");
    assert_eq!(a[0].speed_bps, 1_000_000_000);
    assert!(!a[0].dhcp);

    let tableau = format!("[{un},{un}]");
    assert_eq!(parse_adapters(&tableau).unwrap().len(), 2);

    // BOM PowerShell, espaces, null et sortie vide
    assert_eq!(parse_adapters("\u{feff}  null ").unwrap().len(), 0);
    assert_eq!(parse_adapters("").unwrap().len(), 0);
    assert_eq!(parse_adapters("[]").unwrap().len(), 0);

    // champs absents : défauts sûrs, aucun panic
    let partiel = parse_adapters(r#"[{"name":"Min"}]"#).unwrap();
    assert_eq!(partiel[0].ips, Vec::<String>::new());
    assert_eq!(partiel[0].speed_bps, 0);
    assert_eq!(partiel[0].status, "");
    assert!(!partiel[0].dhcp);

    // liste donnée comme chaîne unique (ConvertTo-Json peut déballer)
    let chaine = parse_adapters(r#"[{"name":"S","ips":"10.0.0.9","dns":["1.1.1.1"]}]"#).unwrap();
    assert_eq!(chaine[0].ips, ["10.0.0.9"]);
    assert_eq!(chaine[0].dns, ["1.1.1.1"]);

    // speed_bps numérique direct, prioritaire sur link_speed
    let num = parse_adapters(r#"[{"name":"N","speed_bps":2500000000,"link_speed":"1 Gbps"}]"#).unwrap();
    assert_eq!(num[0].speed_bps, 2_500_000_000);

    // sorties illisibles refusées
    assert!(parse_adapters("pas du json").is_err());
    assert!(parse_adapters("[{\"name\":42}]").is_err());
    assert!(parse_adapters("[{}]").is_err());
    assert!(parse_adapters("\"hello\"").is_err());
    assert!(parse_adapters("42").is_err());
}

#[test]
fn vitesses_windows_converties_en_bps() {
    assert_eq!(parse_speed("10 Gbps"), Some(10_000_000_000));
    assert_eq!(parse_speed("2.5 Gbps"), Some(2_500_000_000));
    assert_eq!(parse_speed("2,5 Gbps"), Some(2_500_000_000));
    assert_eq!(parse_speed("100 Mbps"), Some(100_000_000));
    assert_eq!(parse_speed("1 Gbps"), Some(1_000_000_000));
    assert_eq!(parse_speed("5760 Mbps"), Some(5_760_000_000));
    assert_eq!(parse_speed("n/a"), None);
    assert_eq!(parse_speed(""), None);
}

#[test]
fn fixture_du_simulateur_est_coherente() {
    let sim = Sim::default();
    assert_eq!(sim.adapters.len(), 3);
    assert!(!sim.fail_netsh);
    assert!(sim.log.is_empty());
    let sim2 = Sim { fail_netsh: true, ..Sim::default() };
    assert!(sim2.adapters.iter().any(|a| a.name == "Wi-Fi" && a.if_type == "71"));
}
