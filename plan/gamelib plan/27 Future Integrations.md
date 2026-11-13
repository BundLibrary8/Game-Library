---
tags: [gamelib, research, integrations]
status: research
created: 2026-11-13
---
# 27 Future Integrations

Back to [[00 Home]] · Today's sources: [[04 Game Sources]] · How they plug in: [[21 Provider Architecture]] · Mods: [[22 Modding Architecture]]

> [!question] Status: research catalogue
> What each integration could give, **how sure we are**, and the restrictions. Researched 13 Nov 2026. **Re-check official docs before building any of these**; launcher file formats and API terms change without notice.

**Confidence key:** ✅ built and tested · 🟢 official/documented · 🟡 community-documented, works in other open-source tools · 🔬 unverified, needs a spike on a real PC · ⛔ avoid (against terms, needs credentials, or scraping)

## Stores and libraries
| Store | Installed games (local) | Owned games (account) | Launch | Playtime / achievements | Notes |
|---|---|---|---|---|---|
| **Steam** | ✅ `steamlocate` reads `libraryfolders.vdf` + `appmanifest_*.acf` | 🟢 Steam Web API `IPlayerService/GetOwnedGames` (user's key + SteamID; profile game details must be public unless it's the key owner's account) | ✅ `steam://rungameid/<id>` | 🟢 `playtime_forever` from GetOwnedGames; 🟢 `ISteamUserStats/GetPlayerAchievements` + `GetSchemaForGame` | Terms of Use: 100,000 calls/day per key. Best-supported store by far |
| **Epic Games** | ✅ `.item` manifests in `ProgramData\Epic\EpicGamesLauncher\Data\Manifests`; ✅ **fallback**: `.egstore` build manifests in library folders + launcher ini for launch ids (uncommitted work, 13 Nov) | ⛔ No official user-library API. Legendary/Heroic use Epic's private launcher API with the user's login: **don't** | ✅ `com.epicgames.launcher://apps/<ns>%3A<id>%3A<app>?action=launch&silent=true` | None official | Keep to local detection |
| **GOG** | ✅ Registry `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\` (still to verify live, see [[09 Milestones]]) | 🟡 GOG Galaxy 2.0's local database (`ProgramData\GOG.com\Galaxy\storage\galaxy-2.0.db`, SQLite) holds owned games **from all its integrations**; undocumented format 🔬. ⛔ GOG web endpoints that need login cookies | ✅ exe direct (DRM-free) | 🔬 Galaxy DB has playtime/achievements internally | Galaxy DB is attractive (one read = many stores) but fragile; read-only, optional, with clear "experimental" label |
| **Ubisoft Connect** | 🟡 Registry `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\<gameId>` with an install-dir value (community write-ups; Playnite has an integration) 🔬 verify on current client | ⛔ none official | 🟡 `uplay://launch/<gameId>/0` (used by Lutris installers and Steam launch options) | none | Title names aren't in the registry key: get them from the game folder name or metadata 🔬 |
| **EA app** | 🔬 `ProgramData\EA Desktop\InstallData\…` holds per-game data; registry "InstallLocation" isn't reliable (empty for Steam-installed EA games per EA forums) | ⛔ none official | 🔬 `origin2://game/launch?offerIds=…` reported, with failures; EA app may need its own URI | none | **Lowest confidence.** Fallback: treat as exe launch from the install folder (EA app still starts for DRM) or leave to folder scan + manual |
| **Xbox / Microsoft Store (PC Game Pass)** | 🔬 Packaged apps (MSIX) enumerated with Windows' `PackageManager` API (WinRT, via the `windows` crate); newer titles install to a user-chosen `XboxGames` folder. Telling games apart from apps is the hard part | 🔬 Xbox Live APIs need Microsoft sign-in; third-party wrappers exist | 🔬 `shell:AppsFolder\<PackageFamilyName>!<AppId>` | 🔬 tracking: exes under `WindowsApps` may not expose paths to `sysinfo`; `XboxGames` installs might | Do a spike before promising this. Heavy dependency → Cargo feature flag |
| **Battle.net** | 🔬 Agent's `product.db` (protobuf) | ⛔ | 🔬 `battlenet://<productCode>` | none | Later |
| **Amazon Games** | 🔬 local SQLite (`%LOCALAPPDATA%\Amazon Games\Data\Games\Sql\…`) | ⛔ | 🔬 `amazon-games://play/<id>` | none | Later |
| **itch.io** | 🔬 itch app's local database | 🟢 itch.io has an OAuth API for owned keys 🔬 | exe | none | Later; indie-friendly |
| **Generic Windows** | 🟡 Uninstall registry entries + Start Menu shortcuts (catches launcher-less games) | — | exe | — | Feeds the existing **folder-scan import review** ✅ rather than auto-adding |
| **Emulators / ROMs** | 🔭 | — | emulator exe + args (`LaunchTarget::Exe` already supports args ✅) | 🟢 RetroAchievements API exists 🔬 | Future possibility only |

## Metadata and artwork
| Source | Gives | Access | Restrictions | Plan |
|---|---|---|---|---|
| **SteamGridDB** ✅ | Covers (built), heroes, logos, icons | Free API key | Be polite; attribution 🔬 | Keep; add hero/logo for game page banners |
| **Steam Store `appdetails`** ✅ | Description, genres, year, price, screenshots | No key | Unofficial-but-public; rate-limit yourself | Keep for Steam games |
| **IGDB** (Twitch) | Descriptions, genres, themes, release dates, platforms, **external store IDs**, similar games, time-to-beat 🔬 | Twitch developer app → client-credentials token | **4 requests/second**, up to 8 open requests (per client ID); commercial use reportedly needs a partnership; attribution expected 🔬 read the terms | **Primary metadata for non-Steam games (Phase 1)**. While personal: the user's own Twitch app keys. Public release: small caching proxy so the secret isn't in the exe ([[24 Backend and Cloud Infrastructure]]) |
| **PCGamingWiki** | Save-game locations, config paths, fixes, which store versions exist | Public MediaWiki/Cargo API 🔬 | Content licence (CC BY-NC-SA 🔬) | Nice "Where are my saves?" panel (Phase 1+), also useful for mod adapters |
| **RAWG** | Metadata, screenshots | Free key with attribution 🔬 limits | 🔬 | Fallback only |
| HowLongToBeat | Completion times | ⛔ no official API | — | Use IGDB time-to-beat instead |

## Mod repositories
See [[22 Modding Architecture#Provider facts and restrictions (researched 13 Nov 2026)]] for detail. Summary:
- **Nexus Mods** 🟢: 20k requests/day then 500/hour; direct download links **Premium-only**; **public apps must be registered** (SSO/OAuth); personal keys not for public-facing apps.
- **Modrinth** 🟢: open read API, 300 requests/min, unique User-Agent required.
- **Thunderstore** 🟢: per-community package listing API; simplest native install target.
- **CurseForge** 🔬: approved API key; respect authors' distribution opt-out.
- **mod.io** 🔬, **Steam Workshop** 🔬: link out.

## Prices
- **IsThereAnyDeal API v2** 🟢: free key via app registration; lookup by Steam app ID; some endpoints need approval.
- **CheapShark** 🟡: free, no key.
See [[25 Game Discovery#Price data sources (researched 13 Nov 2026)]].

## Social and presence
| Integration | What | Confidence | Phase |
|---|---|---|---|
| **Steam friends** ✅ | `GetFriendList` + `GetPlayerSummaries` (built, cached, 5-min refresh limit) | ✅ code, 🔬 live call untested | built |
| **Sign in through Steam** | OpenID 2.0, proves SteamID ownership | 🟢 | 4 |
| **Discord Rich Presence** | Show "Playing X" in Discord via the local Discord client | 🟢 Discord's local RPC; Rust crates exist 🔬 | 3+ (small, fun) |
| Xbox / PSN friends | — | ⛔/🔬 no open APIs | not planned |

## Sources consulted (13 Nov 2026)
- Nexus Mods help: [rate limits](https://help.nexusmods.com/article/105-i-have-reached-a-daily-or-hourly-limit-api-requests-have-been-consumed-rate-limit-exceeded-what-does-this-mean), [API Acceptable Use Policy](https://help.nexusmods.com/article/114-api-acceptable-use-policy); [community OAuth guide](https://modding.wiki/en/api/oauth2-guide); [2018 forum thread on premium-only download links](https://forums.nexusmods.com/topic/7189046-new-error-message/)
- [Modrinth API docs](https://docs.modrinth.com/api/)
- [Thunderstore API docs](https://thunderstore.io/api/docs/)
- [IsThereAnyDeal API docs](https://docs.isthereanydeal.com/)
- [Steam Web API Terms of Use](https://steamcommunity.com/dev/apiterms), [Valve developer wiki: Steam Web API](https://developer.valvesoftware.com/wiki/Steam_Web_API)
- IGDB rate limits: [Twitch developer forum](https://discuss.dev.twitch.com/t/response-headers-oauth-access-tokens-getting-rate-limited/46963) (official docs at api-docs.igdb.com still to be read)
- Ubisoft: [DEV write-up of the registry key](https://dev.to/sakshatshinde/a-simple-way-to-find-installed-game-s-uplay-ids-with-python-3jf6), [GamingOnLinux on uplay://launch](https://www.gamingonlinux.com/articles/20760/)
- EA: [EA forum thread on launch commands](https://forums.ea.com/discussions/ea-play-general-discussion-en/launch-ea-game-using-command/7575680), [EA forum on detection](https://forums.ea.com/discussions/ea-app-technical-issues-en/detecting-already-installed-games/7640949)
- [Playnite docs](https://api.playnite.link/docs/manual/gettingStarted/gettingStartedOverview.html)
- Supabase free-tier figures: third-party summaries, e.g. [automationatlas.io](https://automationatlas.io/answers/supabase-free-tier-limits-2026/) (confirm on supabase.com/pricing)
