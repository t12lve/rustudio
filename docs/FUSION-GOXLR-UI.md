# Fusion goxlr-ui → rustudio (branche `fusion/goxlr-ui`)

Ce dépôt fusionne deux projets :

| Projet | Apport conservé | Où |
| --- | --- | --- |
| **rustudio** (samirmedani, base du fork) | Tout le travail existant, **notamment la partie firmware de la GoXLR** : `crates/goxlr` (winusb.rs, dll.rs, sim.rs), les `FirmwareKind` (`official`, `golden`, `lab`, `audio`) dans `crates/core/src/modules.rs`, la détection firmware dans `crates/engine/src/modules.rs`, l'installeur/mise à jour (`crates/setup`, `crates/engine/src/updater.rs`) — **aucun de ces fichiers n'est modifié** | inchangé |
| **goxlr-ui** (GoXLR-on-Linux + travail local `native-rust`) | L'application GoXLR EX 100 % Rust native (egui/eframe, sans navigateur ni WebView ni .NET) et les types IPC typés du daemon GoXLR Utility | `crates/goxlr-ex` + `vendor/goxlr-types`, `vendor/goxlr-ipc` |

## Ce qui a été intégré depuis goxlr-ui

- `crates/goxlr-ex` : copie de `goxlr-ui/native-rust` (src egui + assets), compilée comme membre du workspace rustudio. Les chemins `include_bytes!` ont été corrigés (`prod/*` → `crates/goxlr-ex/vendor-assets/*`, `../../LICENSE` → `../../../LICENSE`). Build : `cargo build -p goxlr-ex --release` produit `GoXLR-EX.exe`.
- `vendor/goxlr-types` et `vendor/goxlr-ipc` : vendored depuis `goxlr-ui/.portable-build/{types,ipc}`. Le crate `ipc` est réduit à `lib.rs` + `device.rs` (les clients async tokio/reqwest/interprocess sont exclus pour alléger l'arbre de dépendances) ; `goxlr-ex` n'utilise que `DaemonCommand`, `DaemonRequest`, `DaemonResponse`, `DaemonStatus`, `GoXLRCommand`.

## Nouveaux modules matériels (communautaires, auteur t12lve)

Détectés sur le PC de l'utilisateur (WMI/PnP) :

| Appareil | Identifiant | Crate | Doc |
| --- | --- | --- | --- |
| Souris Logitech G sur récepteur LIGHTSPEED/Unifying | USB `046D:C547` | `crates/logitech-g` | `docs/LOGITECH-G.md` |
| Clavier ROCCAT Vulcan 120 AIMO | USB `1E7D:3098` | `crates/roccat-vulcan` | `docs/ROCCAT-VULCAN.md` |
| Manette XInput | bus synthétique XBOX | `crates/gamepad` | `docs/GAMEPAD.md` |
| Cartes réseau (Intel X710 10G, Realtek RTL8125 2,5G, Realtek 8922AE Wi-Fi 7) | PnP PCI | `crates/netcard` | `docs/NETCARD.md` |

Chaque module suit `docs/MODULES.md` : fiche dans `library/index.json`, descripteur dans `crates/core/src/modules.rs` (`INSTALLED`), crate protocole + simulateur + tests, hub dans `crates/engine/src/` (`logitech.rs`, `gamepad.rs`, `netcard.rs`, `roccat.rs`), dispatch dans `crates/engine/src/modules.rs`. Les modules sont **validés sur simulateur uniquement** pour l'instant ; la portée des essais matériels reste à préciser.

## Note de build sur cette machine

Pas de MSVC link.exe ni de Windows SDK : utiliser la toolchain GNU livrée par rustup :

```sh
rustup run stable-x86_64-pc-windows-gnu cargo test
```
