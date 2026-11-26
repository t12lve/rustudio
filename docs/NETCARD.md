# Module Cartes réseau (netcard)

Module **communautaire**, créé par **t12lve** (`crates/netcard`, package `rustudio-netcard`).
**Validé sur simulateur uniquement** : aucune lecture ni action n'a encore été exécutée sur une vraie
carte réseau Windows dans le cadre de ce module. Les commandes réels (PowerShell, netsh, ipconfig)
sont documentés ci-dessous, mais leur comportement exact sur machine reste à confirmer.

Rustudio lit l'état des cartes réseau de Windows en lecture seule et propose trois actions :
**activer**, **désactiver** une carte, et **renouveler** son bail DHCP. Aucun pilote n'est installé,
aucun firmware n'est touché, aucune commande n'est fabriquée hors d'une liste blanche stricte.

## Champs exposés (lecture)

Chaque carte réseau est un objet `Adapter` :

| Champ | Contenu | Source Windows |
| --- | --- | --- |
| `name` | Alias de l'interface (« Qnap-10G », « Ethernet », « Wi-Fi »…) | `Get-NetAdapter.Name` |
| `description` | Description du contrôleur | `Get-NetAdapter.InterfaceDescription` |
| `mac` | Adresse MAC formatée `AA:BB:CC:DD:EE:FF` | `Get-NetAdapter.MacAddress` |
| `status` | « Up » ou « Disconnected » | `Get-NetAdapter.Status` |
| `speed_bps` | Débit de la liaison en bits/s (ex. 10 Gbps → 10 000 000 000) | `LinkSpeed` parsé (« 2,5 Gbps », « 100 Mbps »…) |
| `if_type` | Type d'interface IANA en chaîne (« 6 » = Ethernet, « 71 » = Wi-Fi) | `Get-NetAdapter.ifType` |
| `ips` | Adresses IPv4 de la carte | `Get-NetIPAddress -AddressFamily IPv4` |
| `gateway` | Passerelle(s) IPv4 | `Get-NetIPConfiguration.IPv4DefaultGateway` |
| `dns` | Serveurs DNS configurés | `Get-NetIPConfiguration.DNSServer` |
| `dhcp` | `true` si le bail IPv4 est en DHCP | `Get-NetIPConfiguration.NetIPv4Interface.DHCP` |

Une carte débranchée renvoie des listes vides (`ips`, `gateway`, `dns`), jamais une valeur fictive.

## Actions

`Net::apply(action, body)` n'accepte que la liste blanche suivante ; toute autre action est refusée
avant toute exécution :

| Action | Corps | Commande exécutée | Confirmation |
| --- | --- | --- | --- |
| `enable` | `{"name": "Qnap-10G"}` | `netsh interface set interface name="Qnap-10G" admin=enabled` | relecture, statut « Up » exigé |
| `disable` | `{"name": "Qnap-10G"}` | `netsh interface set interface name="Qnap-10G" admin=disabled` | relecture, statut « Disconnected » exigé |
| `renew` | `{"name": "Qnap-10G"}` | `ipconfig /renew "Qnap-10G"` | relecture, carte toujours « Up » exigée |
| `refresh` | ignoré | aucune (simple relecture) | — |

Règles de sécurité :

- Le `name` doit exister **dans le dernier relevé** (`read`) ; un nom inconnu est refusé sans
  exécuter la moindre commande.
- Le nom est validé (non vide, ≤ 128 caractères, pas de guillemet ni de caractère de contrôle) et
  transmis comme **argument séparé** via `std::process::Command` : aucun shell n'est invoqué, donc
  pas d'injection de commande. Seuls `netsh` et `ipconfig` peuvent être lancés, avec des arguments
  entièrement issus de la liste blanche ci-dessus.
- Après chaque action, l'état est **relu et confirmé** : si le statut relu ne correspond pas
  (ou si la carte a disparu), `apply` renvoie une erreur. Un `netsh`/`ipconfig` en échec (code de
  retour non nul) est une erreur : **aucun faux succès**.
- `ipconfig /renew` sur une carte non connectée échoue côté Windows comme côté simulateur.

## Transport

- **Lectures** (`Probe::adapters`) : `powershell -NoProfile -NonInteractive -Command <script>` qui
  enchaîne `Get-NetAdapter`, `Get-NetIPConfiguration` et `Get-NetIPAddress`, aplatis en objets puis
  sérialisés par `ConvertTo-Json -Compress -Depth 6`. Le parseur accepte un tableau, un objet unique
  (ConvertTo-Json déballe un élément unique), `null`, une sortie vide ou précédée d'un BOM.
- **Actions** (`Probe::exec`) : `std::process::Command` sur `netsh` / `ipconfig`, arguments séparés,
  stdin fermée, code de retour vérifié, stderr remonté dans l'erreur.
- Le transport Windows (`PowerShell`) n'utilise **aucune dépendance `windows-sys`** : uniquement la
  bibliothèque standard.

## Simulateur

`rustudio_netcard::sim::Sim` implémente le même trait `Probe` avec un relevé JSON réaliste :
Intel X710 10G « Qnap-10G » (`24:5E:BE:69:E4:49`, DHCP, 192.168.1.31), Realtek RTL8125 débranchée,
Wi-Fi 7 8922AE connectée. Champs publics : `adapters`, `fail_netsh` (netsh échoue, code 1),
`log` (commande reçue, ex. `netsh interface set interface name=Qnap-10G admin=disabled`).
Le simulateur reproduit les pièges du vrai système : échec de netsh, `ipconfig /renew` sur carte
non connectée, et commandes « réussies » sans effet — la relecture de `Net::apply` les refuse.

## Vérifications

`rustup run stable-x86_64-pc-windows-gnu cargo test -p rustudio-netcard` (11 tests) :
relevé du simulateur (3 cartes, MAC/IP/DNS), enable/disable/renew confirmés par relecture, nom
inconnu refusé sans commande, netsh en échec = erreur sans faux succès, commande sans effet refusée,
actions/noms dangereux refusés, parsing JSON PowerShell (tableau vs objet unique, BOM, champs
absents, vitesses « Gbps/Mbps »).

## Incertitudes sur le vrai matériel

- `Get-NetAdapter.ifType` peut être vide selon les versions de Windows ; le champ `if_type` est alors une chaîne vide.
- Le format exact de `LinkSpeed` (« 10 Gbps », « 2,5 Gbps ») est parsé souple, mais une unité
  inconnue donne `speed_bps = 0` plutôt qu'une erreur.
- L'encodage de sortie de `powershell.exe` peut mangler les caractères accentués des descriptions
  selon la configuration ; le script force UTF-8, à vérifier sur une machine réelle.
- Après `netsh ... admin=enabled`, le statut peut mettre un court instant à passer « Up » ;
  la relecture est immédiate (une temporisation serait à ajouter si besoin sur du vrai matériel).
- `ipconfig /renew` renvoie un code non nul sur carte déconnectée, mais son comportement exact
  (et sa sortie) varie selon les versions de Windows.
- Les noms d'interface contenant des espaces ou des accents sont transmis tels quels en argument
  séparé ; netsh les accepte entre guillemets dans un shell, le passage par argv n'en a pas besoin.
