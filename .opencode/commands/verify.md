---
description: Formatea y valida Rust y frontend
---

Lee `AGENTS.md` y ejecuta la verificación normal del proyecto:

1. `cargo fmt --manifest-path src-tauri\Cargo.toml`
2. `cargo test --manifest-path src-tauri\Cargo.toml --lib`
3. `npm.cmd run build`

No generes instalador. Resume fallos con su causa y archivo; si todo pasa, indica
el número de tests y confirma la compilación TypeScript/Vite.

