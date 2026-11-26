# GoXLR EX — Rust natif

Fenêtre et contrôles egui/eframe, communication Rust avec le moteur GoXLR, typage des commandes par les enums IPC du moteur. Aucun HTML, JavaScript, navigateur, WebView ou hôte .NET à l'exécution. Les données de styles d'effets sont un fichier JSON embarqué ; la police et le logo sont inclus.

Le moteur GoXLR Rust et ses ressources par défaut sont embarqués dans le même exécutable de distribution, puis extraits dans `GoXLR-EX-Rust-data/runtime-1.2.4`. Le pilote audio officiel TC-Helicon doit déjà être installé sous Windows. L'application ne remplace pas ce pilote système propriétaire.

Compilation : `npm run build:rust`, qui produit `prod-rust/GoXLR-EX.exe`. Rust/Cargo et le compilateur MSVC sont nécessaires au build ; Node.js n'est pas nécessaire pour cette compilation ni à l'exécution. La commande npm n'est qu'un raccourci facultatif vers `scripts/build-rust.ps1`.

Sources du protocole : la copie locale du moteur dans `.portable-build/ipc`, `.portable-build/types` et leur espace Cargo parent. Ces sources sont celles des binaires matériels déjà compilés du projet ; elles doivent être présentes pour reconstruire. Le fichier `native-rust/Cargo.lock` verrouille les dépendances.

Tests : `rtk cargo test --manifest-path native-rust/Cargo.toml --locked`. `--smoke-test` lance le vrai moteur, charge son profil configuré, parcourt les vues Mini en lecture seule et enregistre les captures dans `GoXLR-EX-Rust-data/verification`. Il ne doit pas être confondu avec une simulation matérielle.

L'application réutilise un moteur compatible déjà présent au port local 14564. Si elle démarre son propre moteur, elle l'arrête à sa fermeture. Les profils et réglages sont conservés dans `GoXLR-EX-Rust-data` à côté de l'exécutable.
