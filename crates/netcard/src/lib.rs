//! Cartes réseau Windows : état en lecture seule riche (Get-NetAdapter + Get-NetIPConfiguration + Get-NetIPAddress,
//! convertis en JSON par PowerShell) et actions activer / désactiver / renouveler le DHCP via netsh et ipconfig.
//! Documentation : docs/NETCARD.md. Aucune commande n'est fabriquée hors de la liste blanche enable/disable/renew :
//! chaque action est relue et confirmée, un échec ne produit jamais de faux succès.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub type Result<T> = std::result::Result<T, String>;

/// Une carte réseau telle que Windows la décrit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Adapter {
    pub name: String,
    pub description: String,
    pub mac: String,
    pub status: String,
    pub speed_bps: u64,
    pub if_type: String,
    pub ips: Vec<String>,
    pub gateway: Vec<String>,
    pub dns: Vec<String>,
    pub dhcp: bool,
}

/// Source de vérité du module : relevé des cartes et exécution de programmes (powershell, netsh, ipconfig).
pub trait Probe: Send {
    /// Relevé complet des cartes réseau.
    fn adapters(&mut self) -> Result<Vec<Adapter>>;
    /// Exécute un programme avec ses arguments, renvoie sa sortie standard ; code de retour non nul = erreur.
    fn exec(&mut self, program: &str, args: &[&str]) -> Result<String>;
}

impl<T: Probe> Probe for &mut T {
    fn adapters(&mut self) -> Result<Vec<Adapter>> { (**self).adapters() }
    fn exec(&mut self, program: &str, args: &[&str]) -> Result<String> { (**self).exec(program, args) }
}
impl<T: Probe + ?Sized> Probe for Box<T> {
    fn adapters(&mut self) -> Result<Vec<Adapter>> { (**self).adapters() }
    fn exec(&mut self, program: &str, args: &[&str]) -> Result<String> { (**self).exec(program, args) }
}

/// Convertit un débit affiché par Windows (« 10 Gbps », « 2,5 Gbps », « 100 Mbps »…) en bits par seconde.
pub fn parse_speed(text: &str) -> Option<u64> {
    let s = text.trim().to_lowercase().replace(',', ".");
    let cut = s.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let (num, unit) = s.split_at(cut);
    let factor: u64 = match unit.trim() {
        "gbps" | "gb/s" | "gbit/s" | "gb" => 1_000_000_000,
        "mbps" | "mb/s" | "mbit/s" | "mb" => 1_000_000,
        "kbps" | "kb/s" | "kbit/s" | "kb" => 1_000,
        "bps" | "b/s" | "bit/s" | "b" => 1,
        _ => return None,
    };
    let value: f64 = num.trim().parse().ok()?;
    if !value.is_finite() || value < 0.0 { return None; }
    Some((value * factor as f64).round() as u64)
}

/// Liste de chaînes : tableau JSON, ou chaîne unique (ConvertTo-Json déballe parfois un élément unique).
fn str_list(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect(),
        Value::String(s) => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn parse_adapter(item: &Value) -> Result<Adapter> {
    let obj = item.as_object().ok_or("Carte réseau illisible (objet JSON attendu)")?;
    let name = obj.get("name").and_then(Value::as_str).filter(|s| !s.is_empty())
        .ok_or("Carte réseau sans nom dans la sortie PowerShell")?;
    let str_field = |key: &str| obj.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
    let speed_bps = match obj.get("speed_bps").and_then(Value::as_u64) {
        Some(n) => n,
        None => parse_speed(obj.get("link_speed").and_then(Value::as_str).unwrap_or("")).unwrap_or(0),
    };
    Ok(Adapter {
        name: name.to_owned(),
        description: str_field("description"),
        mac: str_field("mac"),
        status: str_field("status"),
        speed_bps,
        if_type: str_field("if_type"),
        ips: obj.get("ips").map(str_list).unwrap_or_default(),
        gateway: obj.get("gateway").map(str_list).unwrap_or_default(),
        dns: obj.get("dns").map(str_list).unwrap_or_default(),
        dhcp: obj.get("dhcp").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Transforme la sortie de PowerShell (ConvertTo-Json -Compress) en cartes réseau.
/// Accepte un tableau, un objet unique (ConvertTo-Json déballe un élément unique), « null » ou une sortie vide.
pub fn parse_adapters(output: &str) -> Result<Vec<Adapter>> {
    let text = output.trim_start_matches('\u{feff}').trim();
    if text.is_empty() || text == "null" { return Ok(Vec::new()); }
    let parsed: Value = serde_json::from_str(text)
        .map_err(|e| format!("Sortie PowerShell illisible (JSON invalide) : {e}"))?;
    let items: Vec<&Value> = match &parsed {
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![&parsed],
        _ => return Err("Sortie PowerShell inattendue (ni tableau ni objet)".into()),
    };
    items.iter().map(|item| parse_adapter(item)).collect()
}

/// Module réseau : lectures riches et actions whitelist. Toute action exige un nom présent dans le
/// dernier relevé, passe par une commande whitelist, puis est relue et confirmée avant le moindre succès.
pub struct Net<T: Probe> {
    probe: T,
    last: Vec<Adapter>,
}

impl<T: Probe> Net<T> {
    pub fn new(t: T) -> Self { Self { probe: t, last: Vec::new() } }

    /// Relevé complet des cartes ; mémorisé comme dernier relevé pour valider les noms des actions.
    pub fn read(&mut self) -> Result<Vec<Adapter>> {
        let adapters = self.probe.adapters().map_err(|e| format!("Lecture des cartes réseau impossible : {e}"))?;
        self.last = adapters.clone();
        Ok(adapters)
    }

    /// Actions whitelist : "enable" / "disable" / "renew" avec {"name": "..."}, "refresh" = read.
    /// Après enable/disable/renew, l'état est relu et le statut confirmé ; sinon c'est une erreur.
    pub fn apply(&mut self, action: &str, body: &Value) -> Result<Value> {
        match action {
            "refresh" => return self.read().map(|adapters| json!({ "adapters": adapters })),
            "enable" | "disable" | "renew" => {}
            other => return Err(format!("Action « {other} » inconnue : liste blanche enable, disable, renew, refresh")),
        }
        let name = body.get("name").and_then(Value::as_str)
            .ok_or("Corps attendu : {\"name\": \"nom de la carte\"}")?;
        // Nom transmis en argument séparé (jamais de shell) ; on refuse pourtant guillemets et
        // caractères de contrôle pour ne pas casser l'analyse propre de netsh.
        if name.is_empty() || name.len() > 128 || name.chars().any(|c| c == '"' || c.is_control()) {
            return Err(format!("Nom de carte refusé : {name:?}"));
        }
        if self.last.is_empty() { self.read()?; }
        if !self.last.iter().any(|a| a.name == name) {
            return Err(format!("Carte « {name} » absente du dernier relevé ({} carte(s) connue(s))", self.last.len()));
        }
        let name_arg = format!("name={name}");
        let (program, args): (&str, Vec<&str>) = match action {
            "enable" => ("netsh", vec!["interface", "set", "interface", &name_arg, "admin=enabled"]),
            "disable" => ("netsh", vec!["interface", "set", "interface", &name_arg, "admin=disabled"]),
            _ => ("ipconfig", vec!["/renew", name]),
        };
        self.probe.exec(program, &args).map_err(|e| format!("{action} « {name} » a échoué : {e}"))?;
        let after = self.read().map_err(|e| format!("Relecture après {action} impossible : {e}"))?;
        let adapter = after.iter().find(|a| a.name == name)
            .ok_or_else(|| format!("« {name} » a disparu du relevé après {action}"))?;
        let attendu = if action == "disable" { "Disconnected" } else { "Up" };
        if adapter.status != attendu {
            return Err(format!(
                "{action} « {name} » non confirmé : statut relu « {} » au lieu de « {attendu} » ; aucun succès annoncé",
                adapter.status
            ));
        }
        Ok(json!({ "action": action, "name": name, "status": adapter.status, "adapters": after }))
    }
}

/// Transport réel Windows : PowerShell pour les lectures, netsh / ipconfig pour les actions.
/// Uniquement std::process::Command : aucune dépendance windows-sys.
#[cfg(windows)]
#[derive(Default)]
pub struct PowerShell;

#[cfg(windows)]
impl PowerShell {
    pub fn new() -> Self { PowerShell }
}

#[cfg(windows)]
impl Probe for PowerShell {
    fn adapters(&mut self) -> Result<Vec<Adapter>> {
        let output = self.exec("powershell", &["-NoProfile", "-NonInteractive", "-Command", PS_ADAPTERS])?;
        parse_adapters(&output)
    }
    fn exec(&mut self, program: &str, args: &[&str]) -> Result<String> {
        use std::process::{Command, Stdio};
        let output = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Impossible de lancer {program} : {e}"))?;
        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let mut msg = format!("{program} a échoué (code {code})");
            if !stderr.is_empty() { msg.push_str(": "); msg.push_str(&stderr); }
            return Err(msg);
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Relevé des cartes réseau en JSON : Get-NetAdapter + Get-NetIPConfiguration + Get-NetIPAddress,
/// aplati en objets {name, description, mac, status, link_speed, if_type, ips, gateway, dns, dhcp}.
#[cfg(windows)]
const PS_ADAPTERS: &str = r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$out = @()
foreach ($a in @(Get-NetAdapter)) {
    $cfg = Get-NetIPConfiguration -InterfaceAlias $a.Name -ErrorAction SilentlyContinue
    $ips = @(); $gw = @(); $dns = @(); $dhcp = $false
    if ($null -ne $cfg) {
        $ips = @(Get-NetIPAddress -InterfaceAlias $a.Name -AddressFamily IPv4 -ErrorAction SilentlyContinue | ForEach-Object { "$($_.IPAddress)" })
        if ($null -ne $cfg.IPv4DefaultGateway) { $gw = @("$($cfg.IPv4DefaultGateway.NextHop)") }
        if ($null -ne $cfg.DNSServer) { $dns = @($cfg.DNSServer | ForEach-Object { "$($_.Address)" }) }
        if ($null -ne $cfg.NetIPv4Interface) { $dhcp = ($cfg.NetIPv4Interface.DHCP -eq 'Enabled') }
    }
    $out += [pscustomobject]@{
        name = "$($a.Name)"; description = "$($a.InterfaceDescription)"; mac = "$($a.MacAddress)"
        status = "$($a.Status)"; link_speed = "$($a.LinkSpeed)"; if_type = "$($a.ifType)"
        ips = $ips; gateway = $gw; dns = $dns; dhcp = $dhcp
    }
}
ConvertTo-Json -InputObject $out -Compress -Depth 6"#;

pub mod sim;
