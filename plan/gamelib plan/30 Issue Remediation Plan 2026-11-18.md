---
tags: [gamelib, issues, plan]
date: 2026-11-18
status: plan
---
# 30 Issue Remediation Plan (18 Nov 2026)

Back to [[00 Home]]

This plan covers the 19 findings in the *Gamelib Initial Application Issue Register* (GAM-001 to GAM-019, kept at [[Gamelib Issue Register]]). Every finding below was traced to the code, but **no code has been changed and none of these fixes has been tested.** "Cause" means confirmed in the code unless it says *hypothesis*. A hypothesis needs `gamelib.log` or a repro on the user's PC before anyone fixes it.

## Shared dependency: a small backend for keyed APIs

GAM-004 (part), GAM-006, GAM-007 and GAM-014 all hit the same problem: Steam Web API, SteamGridDB, IGDB and Anthropic each need a **private key**, and a private key can't ship in `GameLib.exe`. GameLib already has a Supabase project (`cloud.rs`), so the fix is the same for all of them:

- Add **Supabase Edge Functions** (`supabase/functions/*`). Each one keeps its key as a Supabase secret, requires the user's Supabase JWT, applies a per-user rate limit and caches responses.
- The client calls these functions with its existing session (`cloud::fresh_session`) and never sees a private key.
- The Supabase *publishable* key (`DEFAULT_ANON_KEY`) is designed to be public, so it can stay built in.
- **Decision needed:** who pays for and owns these provider accounts, and do their terms allow it? This is a product and account question, not a code one.

---

## A. Startup and first run

### GAM-001: Missing exe icon (Low)
- **Cause:** there is no `build.rs` and no Windows resource, so the exe has no icon. The window uses eframe's default icon (`ui/mod.rs:562`, no `with_icon`). The tray draws its own star (`notify.rs` `tray_picture`).
- **Fix:**
  1. Add `assets/gamelib.ico` (16–256 px).
  2. Add a `build.rs` that uses the `winresource` crate, gated on Windows.
  3. Set `ViewportBuilder::with_icon` from the same artwork, and make the tray use it too.
- **Test:** after a clean `cargo build --release`, check the icon in Explorer, on a pinned shortcut and in the taskbar. The release workflow needs no changes.

### GAM-002: Returning users land on the login page (High)
- **Cause:** this is deliberate. Commit `19fcb81` added it, and `account.rs:69` sets `login_screen = ready` even when a session exists. `remembered_card` then waits for "Continue as".
- **Fix:**
  1. In `start_cloud`, show the splash or "Signing you in…" while `CloudJob::Check` runs.
  2. On `CloudResult::Account(Some)`, set `login_screen = false`.
  3. On a failed refresh, show the login form with a "Your session expired" message. `fresh_session` already clears revoked tokens.
  4. If the check fails because the network is down, keep the user signed in and work offline. Don't loop back to the login screen.
  5. Remove `remembered_card`, or keep it only behind "Switch account".
- **Security follow-up:** the refresh token is stored in plain text in SQLite (`cloud.session`). Move it to Windows Credential Manager with the `keyring` crate.

### GAM-003: Library is the landing page (Medium)
- **Cause:** `ui/mod.rs:549` hardcodes `Screen::Library`. A Home screen already exists (`ui/home.rs`).
- **Fix:**
  1. Make `Home` the default start screen. The crash fallback at `mod.rs:1356` can keep Library.
  2. Rework Home into: Continue playing (the 3 or 4 most recent games), Recently added, Recommended (see GAM-007), a "Go to library" link, and an empty state for new users with a single Scan / Link Steam call to action.
  3. Show nothing that is purely decorative.

## B. Game data and integrations

### GAM-004: No playtime for Fortnite or Valorant (High)
- **Evidence (from `%APPDATA%\gamelib\gamelib.log` and `library.db`, 18 Nov):**
  - `play_sessions` holds only 3 sessions in total: Scritchy Scratchy and Wallpaper Engine. Fortnite (id 12) and VALORANT (id 11) have `last_played = NULL` and no sessions.
  - The log has no "started outside GameLib" line for either game. It does have one for Wallpaper Engine, so the watcher itself works.
  - The Steam hours in the database were not read from Steam. Nothing in the code imports Steam playtime, so they came from a backup or CSV import (`backup.rs:346`, `csvio.rs:190`).
- **Cause 1 (confirmed):** no source imports playtime history for Epic or Riot. Only time GameLib tracks itself counts, so games played before GameLib (or while it was closed) show 0.
- **Cause 2 (still unproven):** `playtime::running_exes` keeps only processes whose `exe()` path GameLib can read. Anti-cheat (Vanguard for Valorant, EAC for Fortnite) may hide that path, so `Watch::matches` never fires. The install paths themselves are fine: both folders pass `is_specific_folder`.
  - **Live test: skipped (18 Nov).** Build the process-name fallback (Fix 1) anyway. It's cheap, and it covers cause 2 if cause 2 turns out to be real. When the fix is tested, play each game with GameLib open and look for "started outside GameLib" in the log. To check cause 2 by hand, run `Get-Process VALORANT*,FortniteClient* | Select Name, Path` while the game is running: an empty `Path` confirms it.
- **Fix:**
  1. Fall back to the process **name**, which `sysinfo` gives even when the path is hidden. Match it against a built-in table of known names (`VALORANT-Win64-Shipping.exe`, `FortniteClient-Win64-Shipping.exe`, `LeagueClient.exe`, …) and against `extras.process_name`.
  2. Add an "Unknown playtime" state, shown as "—" with a tooltip "Not tracked yet", instead of `0h`.
  3. Label hours as "Tracked by GameLib" or "From Steam".
- **External sources:** Riot has no consumer playtime API. Epic only has an unofficial library-service endpoint that needs Epic OAuth with the launcher's credentials. It is a terms-of-service grey area, so don't use it unless the product owner signs off.
- **Test:** play each game for 5 minutes, restart GameLib mid-session (crash recovery already exists: `recover_unfinished_sessions`), and run two copies of GameLib (the watcher must not count the same session twice).

### GAM-005: Valorant has no cover art (Medium)
- **What exists:** the fallback chain is Steam, then the Epic catalogue, then SteamGridDB (only with a key), then the Steam banner, then Wikipedia (`worker.rs:967`, `covers_extra::wikipedia_cover_url`). Valorant should reach Wikipedia.
- **Cause (confirmed):**
  - The log says: `Wikipedia picture download failed for VALORANT: … The image format could not be determined`.
  - The same API query returns `Valorant_logo_-_pink_color_version.svg` for the Valorant page. It's an SVG logo, which the `image` crate can't decode. Even if it could, it's a logo, not a cover.
  - Valorant isn't on Steam or Epic, so no earlier step matches it, and SteamGridDB is skipped because there's no key.
  - Riot isn't actually unreachable. GameLib never asks Riot for art, because Riot has no public artwork API.
- **Fix:**
  1. Skip `.svg` results. The URL has a `?utm_…` query on the end, so check the path, not the end of the string.
  2. Never accept a Wikipedia picture whose aspect ratio is wider than about 0.8. That rejects logos.
  3. Have "Download missing covers" bypass the 7-day backoff (`retry_all`); confirm it does.
  3. Add a small built-in table of known cover URLs or SteamGridDB IDs for popular non-Steam games (Valorant, League, Fortnite, Overwatch, Minecraft).
  4. Move SteamGridDB behind the backend so every user gets it without a key.
  5. Placeholder covers already exist, so keep them, and add a "Change cover…" option for wrong matches.
  6. Once IGDB is added (GAM-007), use its `cover` field as the main non-Steam source. It covers Valorant, League and the rest.

### GAM-020 (new): Riot Client is listed as a game (Medium)
- **Cause (confirmed):**
  - `library.db` row 10 is `Riot Client`, with `source_id = Riot_Client.live`.
  - `sources/riot.rs` skips the client with `product.starts_with("riot_client")`. That check is case-sensitive, and the registry says `Riot_Client`, so the client gets through.
  - The log also shows `no cover found for Riot Client` on every cover pass.
- **Fix:**
  1. Make the check ignore case.
  2. As you asked, add a **launcher** category instead of dropping the client altogether. Add `kind: Game | Launcher | App` to `games` (a schema migration), and set it from the source: Riot Client, Epic Games Launcher, Battle.net, EA app.
  3. Launchers don't count as games. Hide them from the library grid, stats, recommendations, cover lookup and playtime tracking. Show them under a "Launchers" filter (or in the Library sidebar), where they can still be opened.
  4. Add a right-click "Mark as game / launcher / app" option so the user can correct the label.
- **Related, also found in the database:** **Wallpaper Engine** (Steam 431960) is stored as a game. The watcher tracks it whenever it runs, which is most of the time, so it has 115 h and is picking up more. `steam::is_tool` doesn't catch it. Label Steam apps whose store genres include *Utilities* or *Software* (from `appdetails`, which is already fetched) as `App`, and don't track `App` playtime by default.

### GAM-006: Steam needs a manual key and finds only installed games (High)
- **Cause:** `sources/steam.rs` reads only local `appmanifest` files, which list installed games. Owned games need `GetOwnedGames`, which `worker.rs:1327` calls with the user's own `steam_api_key` and a SteamID typed into their profile.
- **Fix:**
  1. **Link Steam** through Steam OpenID 2.0 (`steamcommunity.com/openid/login`). Open the browser with a `return_to` pointing at a loopback listener (`http://127.0.0.1:<port>`), confirm the result with `check_authentication`, and read the SteamID64 from `claimed_id`. **No key is needed for this step.**
  2. **Owned games:** an Edge Function `steam-owned-games` calls `GetOwnedGames(include_appinfo, include_played_free_games)` with the server's key. As a bonus this imports Steam playtime for every owned game.
  3. Store owned-but-not-installed games with `installed = false` and show an "Install" button that opens `steam://install/<appid>`. `Game.installed` already exists. Add a library filter for "Installed" and "All owned".
  4. If the user's Steam "Game details" are not public, `GetOwnedGames` returns nothing. Detect this and say "Your Steam game list is private. Make *Game details* public in Steam privacy settings, then sync again." Link to the settings page.
  5. Keep the user-key path only under Advanced (GAM-018).
- **Dependencies:** a Steam Web API key owned by the project (Steam's terms forbid sharing a key, but proxying it for your own users is normal), plus the backend above.

## C. Discovery

### GAM-007: Recommendations only show games already owned (Medium)
- **Cause:** `insights::recommend` scores only `games` already in the library. The text on Home even says "Games you own but have hardly played" (`home.rs:136`).
- **Fix:**
  1. Split the section into **"From your backlog"**, which keeps the current logic, and **"You might like"**, which covers unowned games.
  2. For unowned games, call IGDB through an Edge Function (`similar_games`, genres, themes), seeded by the user's 5–10 most-played games. Leave out anything already owned.
  3. Cache results for 24 hours.
  4. Show unowned cards with a "Not in library" badge and a "View on store" button (Steam store page or IGDB URL). Don't fetch prices for now.
- **Decided (18 Nov):** use IGDB. GameLib is personal and non-commercial, which IGDB's free terms allow. If GameLib ever goes commercial, look at this again.
- **Dependency:** IGDB requires a Twitch app with a client ID and secret. The secret must still stay server-side, because releases are published publicly on GitHub, so it goes in the Edge Function.

## D. Couch mode

### GAM-008 and GAM-009: Tabs, search and mouse don't work (High)
- **Cause, confirmed:**
  - The top tabs are drawn with `painter.text` only (`couch.rs:646–669`). There is no `interact`, so clicking them does nothing.
  - In Search, `typing = true` turns Q and E off (they type letters), which leaves PageUp/PageDown/Tab as the only way out. Nothing on screen says so.
  - The keys of the on-screen keyboard aren't clickable. Only the Play and Details buttons and the cover row (`couch.rs:765, 944`) respond to the mouse.
  - There's no visible Back or Exit control.
- **Fix:**
  1. Make each tab's rect `ui.interact(rect, id, Sense::click())`. A click sets `couch.tab`, using the same code as `lb`/`rb`. Refactor that into `fn switch_tab(&mut Couch, Tab, time)`.
  2. Make each on-screen key clickable. Add a real search field that takes focus on click, so hardware keyboard typing is explicit.
  3. Add hover highlights and a pointer cursor.
  4. Add visible hint chips ("LB/Q ◀ ▶ E/RB", "B/Esc Back"). In Search they switch to "PgUp/PgDn" and are clickable.
  5. Add a clickable "Exit Couch Mode" button in the top bar.
  6. Clicking sets the same selection index that the controller uses, so the input methods never drift apart.
- **Test:** do the full journey once each with mouse only, keyboard only and controller only: Home → Library → Collections (open and back) → Search (type, pick, play, exit). Keep the existing `Input`/`Repeater` tests and add one for `switch_tab`.

## E. Readability and visual design

### GAM-010: Text looked too small (Medium, mostly explained)
- **Findings:**
  - The default zoom is 1.0, because nothing is saved until the user changes it (`mod.rs:682`). So the default isn't 75%.
  - egui's Ctrl+- shortcut changes the zoom and `notify.rs:294` saves it, which may be how the setting reached 75%.
  - Body text is 15 pt, but secondary text is often 11.5–13 pt with `muted` colour.
- **Fix:**
  1. Set a minimum of 13 pt for secondary text.
  2. Check contrast of `muted` against panels (WCAG AA 4.5:1).
  3. Narrow the zoom range to 90–150% (see GAM-016).
  4. Show the current text size in Settings with a Reset button.

### GAM-011 and GAM-012: Typography and polish (Medium)
- **Findings:**
  - About **30 different hardcoded font sizes** are spread across `ui/*.rs` (`bold(17.0)` ×48, `16.0` ×20, `32.0` ×15, `14.5`, `15.5`, `13.5`, …), plus `.size(..)` overrides. There's no type scale.
  - The font is Inter in three weights (`theme.rs:392`), which is fine to keep.
- **Fix:**
  1. Add a type scale in `theme.rs`: `display 32 / title 24 / heading 19 / subheading 16 / body 15 / caption 13`, with set weights and line heights. Use `TextStyle::Name` entries so zoom still applies.
  2. Replace the literal sizes screen by screen.
  3. Add spacing tokens (4/8/12/16/24/32) and one card and panel style.
  4. Standard empty, loading and error components.
  5. Then bring couch mode in line. It sizes text by screen height (`h * 0.02`) on purpose for TVs, so map it to the same scale ratios.
- Do this **with** the GAM-013 settings rework, so the settings screen is built on the new tokens.

## F. Settings and accessibility

### GAM-013: Settings are cluttered (High)
- **Cause:** one long scroll of 15 panels (`settings.rs`). It mixes everyday options with API keys, Discord app IDs, webhooks, an admin panel and the Supabase key, and has long instruction paragraphs.
- **Fix:** add a left-hand category list:
  - **General:** start screen, startup scan, tray, notifications.
  - **Library:** sources, folders.
  - **Appearance:** theme, animations.
  - **Accessibility.**
  - **Accounts:** GameLib, Steam, Discord.
  - **Playtime and goals.**
  - **Data:** export, import, sync.
  - **About and updates.**
  - **Advanced**, collapsed by default (see GAM-018).

  Give each setting a one-line label and, only where needed, a ≤1-line hint. Move setup instructions into "?" tooltips. Toggles save as soon as they're changed, so the "Save keys" style buttons can go.

### GAM-014: API keys in ordinary settings (High)
- **Inventory:**

| Field | Where it's used | Plan |
|---|---|---|
| SteamGridDB key | covers (`worker.rs:897`) | Backend proxy. Field moves to Advanced as an override. |
| Steam Web API key | friends, achievements, owned games | Backend proxy plus OpenID linking (GAM-006). Field moves to Advanced. |
| Anthropic key | Assistant | **Decided:** keep the Assistant, off by default. Settings → Advanced gets an "Enable Assistant" toggle with the key field under it, and the rail icon only shows when it's on. The user brings their own key, which is fine for personal use. Store the key in `keyring`. |
| Supabase anon key | `settings.rs:353`, `account.rs:161` | **Remove.** It's built in and public by design. Keep the env override for developers. |
| Discord Application ID | Rich Presence | Removed along with Rich Presence (GAM-017). |
| Discord webhook URL | session posts | Keep under Advanced, or remove with GAM-017 if it's unused. |

- **Storage:** user-entered keys are currently plain-text settings in SQLite. Move them to `keyring` together with the GAM-002 token.

### GAM-015: The text-size slider glitches (High)
- **Cause, confirmed:**
  - The slider calls `ctx.set_zoom_factor` on **every change while dragging** (`settings.rs:300–305`). Each step re-scales the whole UI, including the slider, so the handle moves out from under the cursor and the value jumps.
  - `notify.rs:294` also writes the zoom to SQLite every frame it changes.
- **Fix:**
  1. Keep a `pending_zoom` in app state and let the slider edit only that. Better still, replace the slider with preset buttons: 90 / 100 / 115 / 130 / 150%.
  2. Show a fixed-size preview box that renders sample text at `pending_zoom`, scaling the font sizes rather than the context.
  3. **Apply** calls `set_zoom_factor` and saves once. **Cancel** throws `pending_zoom` away. **Reset** sets it back to 100%.
  4. Debounce the Ctrl+/- save in `notify.rs`.
- **Test:** keyboard arrows on the control, mouse drag, Apply then restart (the setting should persist), and Cancel (nothing should change).

### GAM-016: Sidebar doesn't fit at large text sizes (Medium)
- **Cause:** the rail is a fixed 98 pt column with **13 icons and the profile orb** (about 900 pt tall) inside a scroll area whose scrollbar is hidden (`mod.rs:1160`). At 130% or more on a 1080p screen the lower icons are cut off with no sign they're there. The minimum window size (980×620 px) also shrinks to about 560×354 pt at 175%.
- **Fix:**
  1. Cut the rail to around 7 primary items (see GAM-018: Home, Library, Discover, Friends, Stats, Shop, Settings), with a "More" menu for the rest.
  2. Show a fade or chevron when the rail overflows.
  3. Cap the zoom at 150%.
  4. Make `min_inner_size` follow the zoom, or drop the cap at small window sizes.
- **Test:** every zoom preset at 1366×768 and 1920×1080.

## G. Discord

### GAM-017: Is Rich Presence worth it? (Low)
- **What it adds over Discord's own detection:**
  - It works for games Discord doesn't recognise: folder-scanned and loose exes, emulators, and some Epic and Riot titles.
  - It can show GameLib cover art.
  - The webhook "session posts" feature is separate and is not something Discord does on its own.
- **Costs:**
  - Every user must currently create their own Discord application. That's unusable for consumers.
  - It runs a background IPC thread.
  - Profiles may show a duplicate "Playing" when Discord also detects the game.
- **Leaning (18 Nov):** remove it. The user doesn't see any value in it.
- **Removal plan:**
  1. Delete the Rich Presence half of `discord.rs`: the IPC client, `start_discord`, `DISCORD_PRESENCE_KEY`, `DISCORD_APP_KEY` and the settings UI.
  2. Keep the webhook half only if it's wanted. Otherwise delete the whole module.
  3. Drop the old setting keys with a migration.
- **Before deleting:** check whether you play any game Discord doesn't detect (folder-scanned games, emulators). If not, nothing is lost.

## H. Consumer-first experience

### GAM-018 and GAM-019: Too much technical surface (High)
- **Items to move or hide:**
  - **Rail:** Mods, Health, Devices, Franchises and Assistant move under "More". Leave the default rail at about 7 items.
  - **Admin panel (decided: not in production builds):** it grants Stardust and is protected by a plain-text password file whose default is `admin` (`admin.rs`).
    1. Add a Cargo feature `admin`, off by default, and put `admin.rs`, `ui/admin.rs`, `Screen::Admin`, the Settings panel and the Ctrl+Shift+A shortcut behind `#[cfg(feature = "admin")]`. That way the code isn't compiled into `GameLib.exe` at all.
    2. `release.yml` builds without the feature. Developers run `cargo run --features admin`.
    3. CI check: build release and assert the binary doesn't contain the string `admin_password.txt`.
  - **"This PC"** specs and sync folder internals move to Advanced/Diagnostics.
  - **Long "how to" text** moves into tooltips.
- **Three tiers:**
  - **Basic:** the default view.
  - **Advanced:** a collapsed section in Settings with key overrides, the webhook and folder sources.
  - **Developer/Diagnostics:** debug builds or an env flag only. Holds the admin panel, logs and the provider test buttons.
- **Acceptance walkthrough (GAM-019):** a fresh install should get from install, through sign-in, Link Steam, Scan, browse, play and making a collection, without ever seeing the words "API", "key", "anon", "webhook" or "Application ID".

---

## Suggested order

| Phase | Issues | Why first | Needs |
|---|---|---|---|
| 1. Quick, contained bugs | GAM-002, GAM-008/009, GAM-015, GAM-003 | They break core flows and are pure client fixes | None |
| 2. Hide and remove | GAM-014 (Supabase field, Discord ID), the admin-panel part of GAM-018, GAM-016 rail cut | Cheap and they reduce clutter right away | Built-in Discord app ID |
| 3. Design system and settings | GAM-010/011/012, GAM-013, rest of GAM-018 | One pass over every screen | Phase 2 done |
| 4. Data, client side | GAM-020 launcher kind, GAM-004 name fallback and unknown state, GAM-005 SVG and logo filter, GAM-001 icon | No backend needed | None |
| 5. Backend and integrations | GAM-006, GAM-007, rest of GAM-014 | Most work, plus external accounts | Steam, IGDB, SGDB keys; Edge Functions; terms review |
| 6. Review | GAM-017, GAM-019 walkthrough | Product calls | — |

## Decisions (18 Nov)
- **IGDB:** yes. Personal, non-commercial use.
- **Assistant:** kept, as an opt-in setting under Advanced, with the user's own key.
- **Discord Rich Presence:** probably removed (GAM-017).
- **Admin panel:** compiled out of production releases.
- **Riot Client and other launchers:** tagged as launchers, not games (GAM-020).

## Still open
1. Is the Epic unofficial playtime endpoint acceptable? (Recommended: no.)
2. Keep or remove the Discord webhook?
3. Who owns the server-side Steam, SteamGridDB and IGDB keys? They're free, but someone has to register them.
