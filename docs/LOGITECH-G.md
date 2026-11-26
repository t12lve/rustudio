# Module souris Logitech G (récepteur LIGHTSPEED/Unifying)

Module **communautaire** (auteur **t12lve**), déclaré dans `crates/core/src/modules.rs` sous l'id `logitech-g`.
**Validé sur simulateur uniquement** : aucun essai n'a été mené sur un récepteur réel. Le protocole décrit ci-dessous
suit la documentation publique de HID++ 2.0/4.5, mais les points incertains sont listés à la fin de ce document.

Le module pilote une souris Logitech G **à travers son récepteur USB** (LIGHTSPEED ou Unifying) : DPI, fréquence
d'interrogation, batterie/charge et nom de la souris. Aucun firmware n'est modifié, aucun pilote supplémentaire
n'est installé, et aucune commande brute HID n'est exposée.

## Transport HID

- VID `046D` (Logitech), PID `C547` (récepteur LIGHTSPEED/Unifying) — constante `RECEIVER_PIDS`.
- Une seule interface est ouverte : celle de **usage page `FF00`** (canal HID++ du récepteur). Les interfaces
  souris, clavier et notification du même récepteur ne sont jamais ouvertes.
- Les échanges sont des **rapports feature HID de 64 octets**, avec deux report ID HID++ :
  - `0x10` : rapport **court**, 3 octets de paramètres ;
  - `0x11` : rapport **long**, 16 octets de paramètres.
- Sous Windows, `hidapi` (`windows-native`) attend le report ID en tête du tampon : le crate envoie un tampon de
  65 octets `[report ID][64 octets de rapport]` et lit la réponse dans un tampon de même taille, avec le report ID
  demandé en octet 0. Variable d'environnement `RUSTUDIO_HID_TRACE` : trace la PID, l'usage et l'interface ouvertes.
- Côté liaison radio, les lectures (fonctions 0 et 1) sont renvoyées au maximum 3 fois ; **les écritures
  (fonctions 2 et plus) ne sont jamais renvoyées automatiquement**. Une réponse est acceptée seulement si son
  en-tête est un écho strict de la requête (voir ci-dessous) ; sinon `NO_RESPONSE`.

## Trame HID++ (64 octets)

```
octet 0      report ID : 0x10 (court) ou 0x11 (long)
octet 1      device index : 1 à 6 = souris appairée, 0xFF = récepteur lui-même
octet 2      index de la feature (0 = racine)
octet 3      fonction (4 bits hauts) << 4 | software ID (4 bits bas, `0x01` ici)
octets 4..   paramètres (3 en court, 16 en long), complétés de zéros jusqu'à 64 octets
```

`frame(report, device, feature, fonction, swid, params)` construit la trame et refuse tout report ID inconnu,
toute fonction ou swid hors 4 bits, et toute charge trop longue. `payload(reply, request)` normalise la réponse
(Windows peut préfixer un report ID 0) et exige que **le report ID, le device index, la feature, la fonction et le
swid de la réponse soient identiques à la requête**, avec une longueur suffisante ; il renvoie la zone de paramètres.
Une réponse tronquée, corrompue, ou venue d'une autre requête est refusée.

**Messages d'erreur HID++** : feature `0x00`, fonction `2`, paramètres `[feature demandée, fonction demandée,
code d'erreur]`. Ils sont détectés par `Mouse` et transformés en erreur lisible ; `payload()` les refuse comme
toute réponse d'en-tête différent.

## Découverte et features utilisées

1. **Ping racine** (feature `0x0000`, fonction 0, paramètre `[4]`) sur les device index 1 à 6, puis `0xFF` :
   la première réponse identifie la souris appairée. La réponse donne la version mineure du protocole pour la
   majeure demandée (4 → 5, soit HID++ 4.5), reprise dans `Settings.firmware` — ce n'est **pas** le firmware de la
   souris (feature `0x0003`, non lue).
2. **featureSet `0x0001`** : `getFeature(0x0001)` sur la racine donne son index, puis `getCount` (fonction 0) et
   `getFeatureID` (fonction 1, paramètre `[i]`) indexent toutes les features (feature ID → index).

| Feature | Fonctions utilisées | Lecture / écriture |
| --- | --- | --- |
| `0x0000` racine | 0 = ping, 1 = getFeature | ping `[majeure]` → `[mineure]` ; getFeature `[id MSB, id LSB]` → `[index]` |
| `0x0001` featureSet | 0 = getCount, 1 = getFeatureID | count → `[nombre]` ; getFeatureID `[i]` → `[id MSB, id LSB]` |
| `0x0005` deviceFriendlyName | 0 = getName, 1 = setName | getName → `[nb caractères, UTF-8…]` ; setName `[UTF-8…]` (non exposé par `apply`) |
| `0x1001` unifiedBattery | 0 = getStatus | → `[flags, niveau]` ; bit 0 des flags = charge en cours, niveau 0 à 100 |
| `0x1000` batterie héritée (repli) | 0 = getStatus | même layout que `0x1001`, utilisée si `0x1001` est absente ou muette |
| `0x2201` adjustableDpi | 0 = getSensorDpiList, 1 = getSensorDpi, 2 = setSensorDpi | liste → u16 BE suivis de la sentinelle `0xFFFF` ; get `[capteur]` → `[capteur, MSB, LSB]` ; set `[capteur, MSB, LSB]` |
| `0x8060` adjustableReportRate | 0 = getReportRateList, 1 = getReportRate, 2 = setReportRate | liste → intervalles en ms terminés par `0` ; get `[0]` → `[intervalle ms]` ; set `[intervalle ms]` |

Fréquence = `1000 / intervalle ms` : 1 ms = 1 000 Hz, 2 ms = 500 Hz, 4 ms = 250 Hz, 8 ms = 125 Hz.

## API du crate (`crates/logitech-g`)

- `Result<T>`, `NO_RESPONSE`, `VID`, `RECEIVER_PIDS`, `SHORT`/`LONG`.
- `trait Transport { fn exchange(&mut self, request: &[u8; 64]) -> Result<Vec<u8>>; }` (implémenté aussi pour
  `&mut T` et `Box<T>`), `frame(...)`, `payload(...)`.
- `Settings { name, firmware, dpi, dpi_list, dpi_max, polling_hz, battery, charging }` (sérialisable JSON).
- `Mouse<T: Transport>` : `new(t)`, `read() -> Result<Settings>`,
  `apply(key, value) -> Result<Settings>` avec les clés **« dpi »** et **« polling_hz »** uniquement.
  - `dpi` doit être une valeur de `dpi_list` et inférieure ou égale à `dpi_max`, sinon l'écriture est refusée
    avant tout envoi ;
  - `polling_hz` doit être 125, 250, 500 ou 1 000 Hz, sinon refus ;
  - toute autre clé, ou une valeur non entière, est refusée sans rien écrire ;
  - l'écriture est envoyée **une seule fois**, puis l'état complet est relu et la valeur **confirmée** ; si la
    relecture ne correspond pas, `apply` renvoie une erreur (jamais de faux succès).
- `sim::Sim { state, disconnected, writes }` : simulateur fidèle (découverte featureSet, erreurs HID++,
  device index 1, silence radio ailleurs, `NO_RESPONSE` si débranchée).
- `#[cfg(windows)] Hid::open()` : ouvre l'interface usage page `FF00` du récepteur via `hidapi`.

Les tests de `crates/logitech-g/tests/protocol.rs` couvrent la lecture sur simulateur, l'écriture confirmée par
relecture, le refus des valeurs dangereuses sans aucune écriture, le refus des trames tronquées ou corrompues,
la simulateur débranchée, l'écriture ignorée par le « matériel » (erreur, pas de faux succès) et le repli batterie
`0x1000`.

## Validation et incertitudes

**Validé sur simulateur uniquement** : aucun récepteur C547 ni aucune souris G réelle n'a été testé. Le
simulateur reproduit la structure du protocole (trames 64 octets, écho strict, découverte featureSet, erreurs
HID++, repli batterie), pas nécessairement le comportement exact du matériel. Points à vérifier sur un vrai
récepteur (avec `RUSTUDIO_HID_TRACE=1`) :

- les **numéros de fonction exacts**, notamment `0x0005` (getName fonction 0 vs 1, présence d'un octet de
  longueur en setName) et `0x8060` (getReportRate fonction 1, paramètre du set) ;
- le **codage des listes** : sentinelle `0xFFFF` pour la liste DPI, `0x00` pour la liste de fréquences, et le
  nombre maximal de valeurs DPI dans un rapport long de 16 octets de paramètres ;
- les **flags de la batterie** (bit 0 = charge) et le niveau `0xFF` = inconnu, sur `0x1001` comme sur `0x1000` ;
- la **version du protocole renvoyée par le ping racine** (paramètre `[4]` → réponse `[5]`) ;
- le **device index** de la souris (1 à 6 sont sondés ; `0xFF` = récepteur) ;
- la **taille exacte du tampon Windows** pour `send_feature_report` / `get_feature_report` (65 octets avec le
  report ID en tête) et le délai de scrutation des réponses.

Bluetooth, remappage des boutons, éclairage, profils embarqués et flash du firmware ne sont pas implémentés.
