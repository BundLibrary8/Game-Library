# GameLib

A Windows desktop app, written in Rust, that gathers every game you own
(Steam, Epic, GOG, loose folders, and games you add by hand) into one
library. It launches them, tracks how long you play, and shows covers,
stats, recommendations and more.

## What you need

- **Windows 10 or 11**
- **Rust.** Download `rustup-init.exe` from <https://rustup.rs> and run it.
  When it asks for the *Visual Studio C++ Build Tools*, say yes.
- **Git**, from <https://git-scm.com>

To check they are installed, open PowerShell and run:

```powershell
cargo --version
```

```powershell
git --version
```

Each should print a version number.

## Run it for the first time

In PowerShell:

```powershell
git clone https://github.com/BundLibrary8/Game-Library
```

```powershell
cd Game-Library\gamelib
```

```powershell
cargo run
```

The first build takes several minutes, because it compiles everything the
app uses. Later runs start in seconds.

> Keep the folder somewhere like `C:\dev`, **not inside OneDrive**. A Rust
> build creates thousands of files, and OneDrive will try to sync them all.

## Get the latest version

Inside your `Game-Library` folder:

```powershell
git checkout main
```

```powershell
git pull
```

```powershell
cd gamelib
```

```powershell
cargo run
```

## First steps in the app

1. Press **Scan** (top right of the Library) to find your Steam, Epic and
   GOG games. Covers and icons are fetched straight afterwards.
2. For games that aren't in a launcher, use **+ Add game**, or add a
   folder under **Settings → Folders to scan** and use **Scan → Scan my
   game folders**.
3. Click a game to open its page. Press **Launch** to play; playtime is
   tracked automatically.

The icons down the left are Profile, Home, Library, Franchises, Stats,
Shop and Settings.

## If something goes wrong

| Problem | What to do |
|---|---|
| `cargo` is not recognised | Rust isn't installed, or PowerShell was open while it installed. Install it from <https://rustup.rs>, then open a **new** PowerShell window. |
| The window doesn't open | Run `$env:GAMELIB_RENDERER = "glow"` and then `cargo run` again. |
| It still looks like an old version | Run the four commands under *Get the latest version*. Make sure you start it with `cargo run`, not an old `gamelib.exe`. |
| No covers appear | Press **Scan**, or **Settings → Download missing covers**. Covers for non-Steam games need a free SteamGridDB key, entered in Settings. |
| A build error mentions `link.exe` | The Visual Studio C++ Build Tools are missing. Run the Rust installer again and accept them. |

## Optional settings

Run these in PowerShell before `cargo run`. They last until you close that
window.

| Command | What it does |
|---|---|
| `$env:GAMELIB_THEME = "aurora"` | Shows a look without owning it. Also `constellation`, `ember` or `synthwave`. |
| `$env:GAMELIB_RENDERER = "glow"` | Draws with plain OpenGL, for PCs where the window won't open. |
| `$env:GAMELIB_DATA_DIR = "C:\temp\gamelib-test"` | Keeps the library somewhere else, for experimenting safely. |

Your library is normally kept in `%APPDATA%\gamelib\`.

## More

- [`gamelib/README.md`](gamelib/README.md): how the code is organised,
  and what has and hasn't been tested.
- [`plan/gamelib plan/`](plan/gamelib%20plan/00%20Home.md): the design
  notes (an Obsidian vault).
