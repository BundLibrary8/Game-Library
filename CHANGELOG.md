# Changelog

Every release of GameLib, newest first. The release workflow copies the
section for a version into its GitHub release notes, so add a section here
before tagging a release.

## [0.2.0]

- Covers for games that aren't on Steam: Epic games from the Epic
  launcher's catalogue, everything else (Valorant, League of Legends…) from
  Wikipedia. Games without a cover are looked up again.
- Discord: Rich Presence ("Playing …" on your profile) and posting session
  summaries to a channel through a webhook.
- Mod library: every game's mods, switch on and off, install from zip, file
  or folder, remove.
- Game Room: a virtual gaming room built from your library.
- Achievement tracker: Steam achievements (with rarity) and your own
  checklists for any game.
- Friends: what your Steam friends own and play, what's popular among them,
  games in common, and which friends have a game.
- GameLib stays open when it starts a game: games run as separate
  programs, the Windows build no longer opens a console window (which
  closed GameLib if it was closed), and a problem while drawing the window
  is shown as an error instead of closing it. Crashes are written to
  gamelib.log.
- Games started outside GameLib (from Steam, shortcuts…) are noticed and
  their playtime tracked. Crash reporters, updaters and anti-cheat helpers
  left running in a game's folder no longer count as playing.
- Play starts straight away, even during a long artwork download or scan.
- Faster: the library is reloaded only when something changed (and at most
  every couple of seconds while artwork arrives); the Stats, Library
  health and Home screens no longer redo their database and file work on
  every frame; tag counts take one query instead of one per tag.
- Six new animated backgrounds: Firefly Forest (free), Deep Ocean, Sakura,
  Neon Rain, Cosmic Nebula and Frost.
- Fixed the ••• and ▼ buttons, which showed broken symbols on Windows.
- Devices page: connected peripherals grouped by kind, controller
  batteries, and paired Bluetooth devices with Connect / Disconnect.
- Launch profiles: named sets of launch options per game (including Steam
  games), picked with ▼ next to Launch.
- Library health: install checks, disk space per drive, the biggest
  games, and a list of things to fix.
- Session journal (a note per session) and playtime goals (weekly, and
  per game).
- Screenshot galleries from Steam, the Xbox Game Bar and a folder you
  choose.
- Ctrl+K search for everything, and keyboard shortcuts (F1 lists them).
- Accessibility: text size and high contrast.
- CSV export and import.
- Desktop notifications and a system tray icon (Windows).
- Update checking against GitHub releases, and this changelog.
- Startup loading animation; admin panel for Stardust and backgrounds.
- Backlog Intelligence, the assistant, smart collections, Wrapped,
  achievements and sync (from the previous update).

## [0.1.0]

- The first version: Steam, Epic, GOG, Battle.net, Riot, EA and folder
  games; playtime tracking; covers; stats; profile; Stardust shop; Couch
  Mode.
