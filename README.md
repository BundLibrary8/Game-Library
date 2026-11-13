# GameLib

A Windows desktop app, written in Rust, that gathers every game you own
(Steam, Epic, GOG, Battle.net, Riot Client, EA app, loose folders, and
games you add by hand) into one
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

1. Press **Scan** (top right of the Library) to find your Steam, Epic,
   GOG, Battle.net, Riot Client and EA app games. Each can be switched off
   under **Settings → Where to look for games**.
2. For games that aren't in a launcher, use **+ Add game**, or add a
   folder under **Settings → Folders to scan** and use **Scan → Scan my
   game folders**.
3. Click a game to open its page. Press **Launch** to play; playtime is
   tracked automatically. GameLib stays open while you play.

Games you start some other way (from Steam, the Start menu, a desktop
shortcut) are noticed within about 15 seconds and their playtime is
tracked too. Switch this off under **Settings → Playing games**.

Artwork is found automatically, every time the app starts and after every
scan, with no key needed. Watch the bar at the bottom for progress. It
needs an internet connection the first time; if you are offline it tries
again next start.

The icons down the left are Profile, Home, Library, Discover, Assistant,
Franchises, Stats, Shop, Library health, Devices and Settings. The TV icon at the bottom opens
**Couch Mode**.

### What's where

- **Discover** (compass icon): Backlog Intelligence. A summary ("You own
  146 games. You've played 38, completed 12, and haven't launched 72."),
  what to play tonight for the time you have, what to finish next, games
  you've neglected, and short games you could beat in a weekend.
- **Assistant** (speech bubble): ask about your library in plain words:
  "What should I play tonight?", "Which games have I neglected?", "My five
  most played games", "Something like Elden Ring", "What should I complete
  next?". It answers from your library, offline. Add an Anthropic API key
  under **Settings → API keys** and questions go to Claude instead, which
  can answer anything (it is sent a summary of your library: titles,
  hours, status, ratings and genres; untick the box in Settings to stop
  that). Using Claude is billed to your Anthropic account.
- **A game's page**: status, rating, **hours to beat** (look it up on
  HowLongToBeat.com; without it GameLib guesses from the genre), your notes
  and review, and a **game process** name for games whose exe runs from
  somewhere other than their folder, so their playtime is still counted.
- **Library → More…**: smart collections, saved filters like "short and
  unplayed" or "rated 4+ and unfinished" that keep themselves up to date.
  Right-click one, or press **Edit rules**, to change it.
- **Stats**: when you play, your habits and streaks, top genres, and 20
  achievements to unlock. **Your Wrapped** looks back at a year.
- **Settings → Sync between PCs**: pick a folder that OneDrive, Dropbox or
  Google Drive syncs, and your library follows you between PCs.

### More on a game's page

- **Launch profiles**: named sets of launch options, like "Windowed"
  (`-windowed`). Tick one to make it the default; pick another with **▼**
  next to Launch. Works for games started from an exe and for Steam games
  (other launchers don't take options from outside).
- **Session journal**: every session, with a note you can add ("beat the
  second boss").
- **Playtime goal**: "play 20 hours of this". The weekly goal (or limit)
  for all games is in **Settings → Weekly playtime goal**; both show on
  Home.
- **Screenshots**: found in Steam's screenshot folder, the Xbox Game Bar's
  `Videos\Captures` folder, and a folder you choose. Click one to see it
  big; arrow keys step through them.

### Search and keyboard shortcuts

Press **Ctrl+K** to search everything: games, screens and actions. **F1**
lists every shortcut, including **Ctrl+F** (search the library),
**Ctrl+1…9** (screens), **F5** (scan), **Ctrl+Enter** (play the open game)
and **Ctrl +/-** (text size).

### Library health

The heartbeat icon checks every game: missing exes or folders, missing
covers, duplicates. It shows free space on each drive and, after **Measure
disk space**, how big each game is.

### Devices

The mouse icon lists everything connected to the PC, grouped by kind:
controllers (with battery level), keyboards, mice, audio, cameras,
monitors, drives and printers. A device with a problem is marked in red.
It refreshes by itself every 20 seconds.

The **Bluetooth** section lists the devices paired with this PC, whether
each is connected, and a **Connect** / **Disconnect** button. A device
has to be switched on and nearby to connect. New devices are paired in
Windows' own Bluetooth settings (there's a button for it). Bluetooth LE-only
devices, like some mice and fitness trackers, don't appear here; manage
them in Windows' settings.

### Backgrounds

Pick a background under **Settings → Appearance**; more are in the
**Shop**. Constellation, Ember and Firefly Forest are free. The others are
Aurora, Synthwave Grid, Deep Ocean, Sakura, Frost (600 Stardust each), and
Neon Rain and Cosmic Nebula (800). Every background moves; untick
**Animated backgrounds** to keep them still. The admin panel can set any
picture as the background instead.

### Other settings

- **Accessibility**: text size and high contrast.
- **Notifications and tray** (Windows): desktop notifications when a game
  closes, an achievement unlocks or a goal is reached while GameLib isn't in
  front; a system tray icon; and optionally keep running in the tray when
  the window is closed (right-click the icon → Quit to really close).
- **Export and import**: as well as the full JSON backup, **Export CSV** /
  **Import CSV** for spreadsheets. An import matches games by title and
  updates status, rating, hours played, hours to beat, favourite, tags and
  notes; rows with an `exe` column add games that aren't in the library.
- **Updates**: shows your version and checks GitHub for a newer release.

### Admin panel

**Settings → Admin panel** (or **Ctrl+Shift+A**) opens a password-protected
panel where you can:

- give Stardust (or take it away), and
- use any picture (PNG, JPEG or WebP) as the background behind every
  screen.

The password is kept in its own text file, `admin_password.txt`, in
GameLib's data folder:

```
%APPDATA%\gamelib\admin_password.txt
```

It's created the first time you try to unlock the panel, with the password
`admin`. Open it in Notepad, replace `admin` with your own password and save.
Lines starting with `#` are ignored. This keeps casual hands off the panel,
but anyone who can open the file can read the password.

GameLib shows a short loading animation when it starts. Untick **Settings →
Loading animation on startup** to skip it.

### Couch Mode

A full-screen view for playing on a TV with a controller. Open it with the
TV icon, **F11**, or the **Start** button on a controller; tick **Settings →
Open in Couch Mode** to start in it every time.

| Controller | Keyboard | Does |
|---|---|---|
| Left stick / D-pad | Arrow keys | Move through games |
| LB / RB | Q / E | Change tab (Home, Library, Collections, Search) |
| A | Enter | Play (or open a collection) |
| Y | I | Details |
| X | F | Favourite |
| B | Esc | Back, or leave Couch Mode |
| Start | F11 | Leave Couch Mode |

In Search, type on the keyboard or use the on-screen letters (up and down
switch between the letters and the results).

## If something goes wrong

| Problem | What to do |
|---|---|
| `cargo` is not recognised | Rust isn't installed, or PowerShell was open while it installed. Install it from <https://rustup.rs>, then open a **new** PowerShell window. |
| GameLib seems to close or vanish when a game starts | The game is in front of it: GameLib is still running (check the taskbar). If it really closed, send the end of `%APPDATA%\gamelib\gamelib.log`, which now records any crash. |
| The window doesn't open | Run `$env:GAMELIB_RENDERER = "glow"` and then `cargo run` again. |
| It still looks like an old version | Run the four commands under *Get the latest version*. Make sure you start it with `cargo run`, not an old `gamelib.exe`. |
| No covers appear | Check you are online, then restart the app or use **Settings → Download missing covers**. A game that isn't on Steam under the same name needs a free SteamGridDB key, entered in Settings. |
| A build error mentions `link.exe` | The Visual Studio C++ Build Tools are missing. Run the Rust installer again and accept them. |

## Optional settings

Run these in PowerShell before `cargo run`. They last until you close that
window.

| Command | What it does |
|---|---|
| `$env:GAMELIB_THEME = "aurora"` | Shows a look without owning it. Also `constellation`, `ember`, `synthwave`, `fireflies`, `ocean`, `sakura`, `rain`, `nebula` or `frost`. |
| `$env:GAMELIB_RENDERER = "glow"` | Draws with plain OpenGL, for PCs where the window won't open. |
| `$env:GAMELIB_DATA_DIR = "C:\temp\gamelib-test"` | Keeps the library somewhere else, for experimenting safely. |

Your library is normally kept in `%APPDATA%\gamelib\`.

## Releases

Releases are built automatically. To make one:

1. Add a section for the new version at the top of `CHANGELOG.md`.
2. Change `version` in `gamelib/Cargo.toml` to match.
3. Commit, then tag and push the tag:

```bash
git tag v0.2.0
```

```bash
git push origin v0.2.0
```

GitHub Actions then builds `gamelib.exe` on Windows, zips it with the
README and changelog, and publishes a GitHub release with that version's
notes. **Settings → Updates → Check for updates** looks at those releases.
Every push and pull request also runs the tests (`.github/workflows/ci.yml`).

## More

- [`gamelib/README.md`](gamelib/README.md): how the code is organised,
  and what has and hasn't been tested.
- [`plan/gamelib plan/`](plan/gamelib%20plan/00%20Home.md): the design
  notes (an Obsidian vault).
