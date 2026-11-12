---
tags: [gamelib, setup]
---
# 10 Setup Checklist

Back to [[00 Home]]

## Install
- [ ] **Rust**: download `rustup-init.exe` from **rustup.rs** and run it.
  - It will ask for the **Visual Studio C++ Build Tools** ("Desktop development with C++"). Say yes and let it install them. Rust on Windows needs them, and so does the `rusqlite` bundled SQLite.
  - Check in a new terminal: `rustc --version` and `cargo --version`
- [ ] **VS Code** + the **rust-analyzer** extension (gives errors, autocomplete and type hints as you type)
- [ ] Optional: **CodeLLDB** extension for step-through debugging
- [ ] **Git** + a **GitHub** repo called `gamelib`
- [ ] **SteamGridDB** account → API key (needed from Milestone 4)

## Create the project
```powershell
cd C:\dev            # see the warning below
cargo new gamelib
cd gamelib
cargo add eframe
cargo run            # first build is slow; later builds are much faster
```

> [!warning] Don't put the code in OneDrive
> Rust builds create a large `target\` folder with thousands of files. OneDrive will try to sync all of them, which is slow and can lock files mid-build. **Keep the code in something like `C:\dev\gamelib`** and use GitHub as the backup. This Obsidian vault can stay on OneDrive.

## Habits
- `cargo fmt`, `cargo clippy` and `cargo test` before each commit
- Small commits with clear messages ("Add Epic manifest parser")
- One [[13 Dev Log Template]] entry per session
