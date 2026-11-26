# Module ROCCAT Vulcan 120 AIMO

Module **communautaire** (auteur : t12lve), livré dans `crates/roccat-vulcan`. Il est **validé sur simulateur
uniquement** : aucune lecture ni écriture n'a encore été vérifiée sur un vrai Vulcan 120 AIMO. Voir la dernière
section pour le détail des incertitudes. Aucun firmware n'est modifié, aucun pilote supplémentaire n'est installé.

Clavier filaire uniquement : le Vulcan 120 AIMO n'a pas de récepteur 2,4 GHz. Le Vulcan 100 AIMO
(PID `307A`) partage le même pilote Linux mais n'est pas pris en charge ici.

## USB et transport

- VID `1E7D`, PID `3098` (Vulcan 120 AIMO).
- HID brut : rapports de 64 octets précédés du report ID `00` dans les appels Windows, soit **65 octets sur le fil**.
- Le backend Windows (`Hid`, `cfg(windows)`) ouvre le **canal vendor** via `hidapi` (`windows-native`) :
  usage page `FF00` préféré, interface 1 préférée (c'est le canal « ctrl » du pilote Linux), et jamais
  l'interface clavier. Les lectures perdues sont renvoyées au maximum trois fois ; **les écritures ne sont
  jamais renvoyées**. Une réponse dont l'enveloppe ne correspond pas à la requête est ignorée ; à défaut de
  réponse valide, l'erreur est `NO_RESPONSE`.

## Protocole connu du matériel (source : pilote Linux duncanthrax/roccat-vulcan)

Le pilote Linux distingue deux interfaces :

- **Interface 1 « ctrl »** : rapports *feature* dont le report ID est la commande — `0x0F` (infos, 8 octets),
  `0x15`, `0x05`, `0x07`, `0x0A`, `0x0B`, `0x06`, `0x09`, `0x0D` (443 octets, effet « wave » ; octet 5 et octet 441
  = vitesse, plage 1..=11, 1 = lent, 6 = moyen, 11 = rapide), `0x13` (8 octets ; octet 2 = mode : 0 = effet
  matériel, 1 = effet piloté par l'hôte). Une séquence d'init envoie `0x15`, `0x05`, `0x07`, `0x0A`, `0x0B`,
  `0x06`, `0x09`, `0x0D`, `0x13` en attendant un accusé `0x04…01` entre chaque.
- **Interface 3 « LED »** : rapports de sortie de 65 octets pour la carte par touche — première trame
  `[00 A1 01 01 B4 …60 octets…]`, puis 6 trames de 64 octets ; la carte fait 444 octets pour 144 touches,
  offset `(k/12)*36 + (k%12)`, plans R/V/B aux offsets +0/+12/+24.

Aucune ligne de code du pilote n'est reprise ; seuls ces faits de protocole sont utilisés.

## Enveloppe retenue par le crate (miroir de rustudio-attack-shark)

Requête de 64 octets : octet 2 = `02`, octet 3 = longueur de la charge, octet 4 = cible (`0` = clavier),
octet 5 = commande, charge à partir de l'octet 6 (58 octets au plus). La commande avec le bit `0x80` est la
lecture ; sans ce bit, c'est l'écriture.

Réponse : report ID `0` facultatif, puis marqueur `A1` (ou `02`), écho des octets 2/4/5 de la requête, longueur
en octet 3, données à partir de l'octet 6. `payload()` valide strictement cet écho : une réponse tronquée, d'une
autre commande ou d'un autre appareil est refusée.

| Octet 5 (écriture) | Lecture | Réglage | Charge |
| --- | --- | --- | --- |
| `0x05` | `0x85` | `brightness` 0..=100 | 1 octet |
| `0x0A` | `0x8A` | `color` [r,g,b], chaque composante 0..=255 | 3 octets |
| `0x0D` | `0x8D` | `speed` 0..=3, voir mapping ci-dessous | 1 octet |
| `0x13` | `0x93` | `effect`, code dans le tableau ci-dessous | 1 octet |
| `0x15` | `0x95` | `per_key` (mode par touche) | 1 octet 0/1 |
| — | `0x8F` | infos / `firmware` (lecture seule) | 3 octets `maj.min.patch` |

Effets (`EFFECTS`, codes sur le fil) : `0` = `off`, `1` = `wave`, `2` = `breathe`, `3` = `reactive`,
`4` = `custom` (carte par touche).

Mapping de la vitesse : l'API expose 0..=3, le rapport « wave » réel prend 1..=11 ; le crate envoie
`0→1, 1→4, 2→8, 3→11` (`speed_to_device`) et refuse toute valeur lue hors de cette table.

## API et garanties

`crates/roccat-vulcan` expose `Transport` (`exchange` sur rapports de 64 octets), `frame`/`payload`,
`Settings { firmware: Option<String>, effect: String, brightness: u8, speed: u8, color: [u8;3], per_key: bool }`,
`Keyboard::new/read/apply`, le simulateur `sim::Sim { state, disconnected, writes }`, et `Hid` sous Windows.

`apply(key, value)` pour les clés `effect`, `brightness` (0..=100), `speed` (0..=3), `color` ([r,g,b] 0..=255)
et `per_key` (bool) :

1. **validation complète avant toute écriture** — une valeur hors limites (`brightness` 101, `speed` 9,
   `color` [300,0,0], effet inconnu, clé inconnue…) est refusée sans rien envoyer ;
2. **une seule écriture** sur le fil ;
3. **relecture complète** puis confirmation : si la valeur relue ne correspond pas, l'appel échoue, même si
   l'accusé de l'écriture était arrivé. Un accusé perdu n'est pas fatal : si la relecture confirme la valeur,
   le réglage est annoncé réussi.

Aucun faux succès : simulateur débranché, écriture ignorée par le matériel ou trame corrompue → erreur.

## Simulateur et tests

`sim::Sim` reproduit les pièges du vrai clavier : trame invalide (`octet 2 != 02` ou longueur > 58) refusée,
commande inconnue refusée, écriture hors limites refusée **sans** incrémenter `writes`, `disconnected` →
erreur « débranché », réponses toujours enveloppées comme le matériel (65 octets, marqueur `A1`, écho strict).

```
rustup run stable-x86_64-pc-windows-gnu cargo test -p rustudio-roccat-vulcan
```

`tests/protocol.rs` couvre : lecture complète sur simulateur ; `apply` + confirmation pour chaque clé ;
valeurs dangereuses refusées avant toute écriture (état et compteur d'écritures inchangés) ; trames corrompues
ou étrangères refusées par `payload` ; simulateur débranché ; écriture ignorée = erreur ; accusé perdu mais
réglage confirmé par relecture = succès.

## Validé sur simulateur uniquement : incertitudes sur le protocole réel

- Les ID de commande `0x05` (luminosité), `0x0A` (couleur) et `0x15` (mode par touche) sont **empruntés** à la
  séquence d'init du pilote Linux ; leur sémantique exacte pour ces réglages n'a pas été confirmée sur le
  matériel. Seuls `0x0F` (infos) et `0x13`/`0x0D` (mode/vitesse de l'effet « wave ») ont une sémantique documentée.
- Les codes d'effet `0..=4` et le mapping de vitesse 0..=3 → 1/4/8/11 sont des choix du crate, pas des valeurs
  lues sur un vrai clavier.
- Le crate cause sur un seul canal (rapports 64 octets, enveloppe type attack-shark) ; sur le vrai clavier, le
  canal « ctrl » (interface 1) utilise des rapports *feature* de longueur variable avec report ID = commande, et
  le canal « LED » (interface 3) des rapports de sortie à en-tête `A1 01 01 B4`. Le choix de l'interface
  ouverte par `Hid` (usage page `FF00`, interface 1 préférée) n'a pas été testé physiquement.
- Le vrai clavier n'a pas été observé en train de **relire** l'état par ces commandes ; la confirmation par
  relecture suppose qu'il le fasse.
- La carte LED par touche complète (444 octets en 7 trames `0xB4`) n'est **pas** implémentée : seul le drapeau
  `per_key` est géré, avec la couleur unique `color`.
- La version de firmware est lue via `0x8F` (3 octets) ; son format exact n'est pas confirmé.
- Aucun test sur Windows avec `Hid` contre un vrai clavier : toute la validation matérielle reste à faire.
