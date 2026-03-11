# AlfaChat 🦀

Retour aux sources. Un chat minimaliste style années 90.

## Features

- Interface terminal old-school (fond noir, texte vert)
- WebSocket pour le temps réel
- Historique des 100 derniers messages (en mémoire)
- Pas d'auth, juste un pseudo
- ~100 lignes de Rust

## Run

```bash
cargo run --release
```

Ouvrir http://localhost:3333

## Stack

- Rust + Tokio
- Axum (WebSocket + static files)
- HTML/CSS/JS vanilla (une seule page)

## Screenshot

```
[ AlfaChat ]
retour aux sources • v0.1.0
─────────────────────────────────
[14:20:01] <alfu> yo
[14:20:05] <georges> salut chef
[14:20:12] <alfu> ça marche ce truc
─────────────────────────────────
[pseudo____] [message...______] [SEND]
```

---
Built with 🦀 by La Tribu
