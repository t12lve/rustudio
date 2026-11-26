# Module manettes (gamepad XInput)

**Module communautaire · Créé par t12lve.** Validé **sur le simulateur uniquement** : aucune manette
réelle (ni récepteur réel) n'a été branchée pendant le développement. Toute validation physique reste
à faire avant une confiance totale dans le backend XInput.

Ce module lit l'état des **quatre slots XInput** de Windows (boutons, sticks, gâchettes, batterie) et
pilote la **vibration**. Il ne fait que lire et vibrer : aucun remappage de boutons, aucun réglage
interne de la manette. Aucun pilote supplémentaire n'est installé : tout passe par `xinput1_4.dll`,
déjà présent dans Windows.

## Fonctions implémentées

- Lecture des 4 slots (0 à 3) : présence, masque de boutons, axes normalisés, batterie, charge.
- `refresh` : relit les 4 slots et renvoie toujours 4 entrées (`connected: false` pour un slot vide).
- `vibrate` : `{"index":0..3,"left":0..=65535,"right":0..=65535,"ms":50..=5000}` sur un slot connecté.
  Validation complète avant l'écriture, puis relecture du slot après la vibration.
- Le simulateur (`crates/gamepad/src/sim.rs`) reproduit les pièges du vrai matériel : slot vide,
  récepteur débranché, vibration refusée sans manette. Il simule **une manette sur le slot 0**
  (batterie 87 %, boutons/axes de démonstration) et **enregistre chaque vibration** au lieu de dormir.

## XInput et les manettes réelles derrière un récepteur

XInput ne voit que les manettes **compatibles XInput** : manettes Xbox 360/One/Series et la plupart des
manettes certifiées « XInput » par leur fabricant. Une manette branchée sur **son propre récepteur USB
(dongle 2,4 GHz propriétaire) apparaît comme une manette XInput standard** tant que le pilote du
fabricant expose XInput — Rustudio ne distingue pas le câble du récepteur, et XInput ne le dit pas.
En revanche, un récepteur tiers qui n'expose que du HID brut / DirectInput **n'est pas vu** par ce
module : Windows peut reconnaître la manette sans que Rustudio sache la lire ici.

`XInputGetState` renvoie `ERROR_DEVICE_NOT_CONNECTED` sur un slot vide : le module traduit ce code en
`connected: false`, et toute autre erreur du transport remonte telle quelle (aucun faux succès).

## Batterie

`XInputGetBatteryInformation` (type `GAMEPAD`, pas le casque) renvoie :

- **manette filaire (câble USB)** : type `WIRED` → batterie affichée à **100 %**, `charging: true` ;
- **sur batterie** : type pile alcaline/NiMH et un niveau parmi **quatre paliers seulement**
  (`EMPTY`, `LOW`, `MEDIUM`, `FULL`), convertis en 0 / 25 / 50 / 100 %. XInput ne donne **pas** de
  pourcentage fin sur batterie ;
- type `DISCONNECTED` ou `UNKNOWN`, ou lecture en échec : batterie inconnue (`null`), sans bloquer
  la lecture de l'état.

## Limites et précautions

- Slots 0 à 3 uniquement (`SLOTS = 4`) ; `poll`/`vibrate` refusent un index supérieur.
- Axes : sticks en i16 brut (-32768..32767), **sans zone morte appliquée** — c'est à l'application
  d'en appliquer une. Gâchettes LT/RT : 0..255 chez XInput, normalisées en i16 **0..32767**
  (`trigger_i16`).
- Vibration : moteurs gauche/droite 0..=65535 ; `apply` n'accepte qu'une durée de **50 à 5000 ms**
  (constantes `VIBRATE_MS_MIN`/`VIBRATE_MS_MAX`), refuse toute autre valeur **avant** d'écrire.
  Le backend réel fait `XInputSetState` → attend `ms` → `XInputSetState(0,0)` ; si l'arrêt échoue,
  l'erreur le dit (les moteurs peuvent rester actifs).
- **La vibration n'est pas relisible** : `XInputGetState` ne rapporte pas l'état des moteurs. Après une
  vibration, `apply` relit seulement le slot (toujours connecté ?) ; un échec de `XInputSetState`
  remonte, et un slot qui ne répond plus après l'écriture fait échouer la commande.
- Le transport réel (`Xinput`) n'existe que sous Windows (`cfg(windows)`, windows-sys 0.59, features
  `Win32_Foundation` + `Win32_UI_Input_XboxController`). Ailleurs, seul le simulateur compile.

## Code et tests

`crates/gamepad` contient le trait `Pad`, `Gamepad<T: Pad>` (`read`, `apply`), le backend `Xinput`
(Windows) et `sim::Sim`. Tests sur simulateur uniquement :

```
rustup run stable-x86_64-pc-windows-gnu cargo test -p rustudio-gamepad
```

API :

```rust
let mut gp = Gamepad::new(sim::Sim::default());
let slots: Vec<PadState> = gp.read()?;                 // toujours 4 entrées
gp.apply("vibrate", &serde_json::json!({"index": 0, "left": 1000, "right": 0, "ms": 200}))?;
gp.apply("refresh", &serde_json::json!({}))?;
```

`PadState` : `{index, connected, battery: Option<u8>, charging, buttons: u16, axes: [i16; 6]}`
(axe = LX, LY, RX, RY, LT, RT), sérialisable en JSON. Les bits de `buttons` sont exposés sous les
noms `BTN_A`, `BTN_DPAD_UP`, … (constantes du crate).
