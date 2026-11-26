//! Fausse carte réseau : mêmes lectures JSON et mêmes commandes netsh/ipconfig que le vrai Windows,
//! avec ses pièges (netsh qui échoue, ipconfig /renew sur carte non connectée, commande « réussie »
//! mais sans effet — la relecture de Net::apply refuse alors tout faux succès).
use crate::{parse_adapters, Adapter, Probe, Result};

/// Relevé JSON réaliste (style ConvertTo-Json -Compress) : Intel X710 10G « Qnap-10G » en DHCP,
/// Realtek RTL8125 débranchée, Wi-Fi 7 8922AE connectée.
pub const FIXTURE_JSON: &str = r#"[
  {
    "name": "Qnap-10G",
    "description": "Intel(R) Ethernet Controller X710 for 10GbE SFP+",
    "mac": "24:5E:BE:69:E4:49",
    "status": "Up",
    "link_speed": "10 Gbps",
    "if_type": "6",
    "ips": ["192.168.1.31"],
    "gateway": ["192.168.1.1"],
    "dns": ["192.168.1.1", "1.1.1.1"],
    "dhcp": true
  },
  {
    "name": "Ethernet",
    "description": "Realtek PCIe 2.5GbE Family Controller (RTL8125)",
    "mac": "2C:F0:5D:12:8A:3B",
    "status": "Disconnected",
    "link_speed": "2.5 Gbps",
    "if_type": "6",
    "ips": [],
    "gateway": [],
    "dns": [],
    "dhcp": true
  },
  {
    "name": "Wi-Fi",
    "description": "Wi-Fi 7 8922AE 802.11be",
    "mac": "D8:BB:C1:45:6E:20",
    "status": "Up",
    "link_speed": "2.4 Gbps",
    "if_type": "71",
    "ips": ["192.168.1.55"],
    "gateway": ["192.168.1.1"],
    "dns": ["192.168.1.1", "8.8.8.8"],
    "dhcp": true
  }
]"#;

/// Simulateur : état des cartes, échec simulé de netsh, et journal de chaque commande reçue.
#[derive(Clone)]
pub struct Sim {
    pub adapters: Vec<Adapter>,
    pub fail_netsh: bool,
    pub log: Vec<String>,
}

impl Default for Sim {
    fn default() -> Self {
        Self { adapters: parse_adapters(FIXTURE_JSON).expect("fixture netcard"), fail_netsh: false, log: Vec::new() }
    }
}

impl Probe for Sim {
    fn adapters(&mut self) -> Result<Vec<Adapter>> { Ok(self.adapters.clone()) }

    fn exec(&mut self, program: &str, args: &[&str]) -> Result<String> {
        self.log.push(format!("{} {}", program, args.join(" ")));
        match (program, args) {
            ("netsh", ["interface", "set", "interface", name_arg, admin_arg]) => {
                let Some(name) = name_arg.strip_prefix("name=") else {
                    return Err(format!("netsh : argument inattendu {name_arg:?}"));
                };
                let Some(admin) = admin_arg.strip_prefix("admin=") else {
                    return Err(format!("netsh : argument inattendu {admin_arg:?}"));
                };
                if self.fail_netsh { return Err("netsh a échoué (code 1, échec simulé)".into()); }
                let Some(adapter) = self.adapters.iter_mut().find(|a| a.name == name) else {
                    return Err(format!("netsh : aucune interface « {name} »"));
                };
                adapter.status = match admin {
                    "enabled" => "Up".to_owned(),
                    "disabled" => "Disconnected".to_owned(),
                    other => return Err(format!("netsh : admin={other} non supporté")),
                };
                Ok(String::new())
            }
            ("ipconfig", ["/renew", name]) => {
                let Some(adapter) = self.adapters.iter().find(|a| a.name == *name) else {
                    return Err(format!("ipconfig : aucune interface « {name} »"));
                };
                // comme le vrai ipconfig : renouveler une carte non connectée échoue
                if adapter.status != "Up" {
                    return Err(format!("ipconfig /renew « {name} » : carte non connectée, renouvellement impossible"));
                }
                Ok(String::new())
            }
            _ => Err(format!("Programme non autorisé dans le simulateur : {program}")),
        }
    }
}
