---
tags: [gamelib, issues, summary]
date: 2026-11-18
status: plan
---
# 31 Issue Plan: The Short Version

Back to [[00 Home]] · Full technical plan: [[30 Issue Remediation Plan 2026-11-18]] · Original findings: [[Gamelib Issue Register]]

This is a plain-English summary of the problems found when testing `GameLib.exe` on 18 Nov 2026, and how we plan to fix them. **Nothing has been fixed yet.**

---

## What's wrong, and why

### Biggest problems
| Problem | Why it happens | The fix |
|---|---|---|
| You have to press "Continue as…" every time you open GameLib | An earlier change made GameLib always open on the login page | If you're still signed in, go straight into the app |
| Couch mode tabs (Library, Collections, Search) can't be clicked | The tabs are drawn on screen but were never made clickable | Make every tab and button clickable, and add a visible Exit button |
| You get stuck in couch mode Search | Q and E type letters in Search, so they can't switch tabs | Clickable tabs, plus on-screen hints for the other keys |
| The text-size slider jumps around | Dragging it resizes the whole window, slider included, on every step | Preview the size first, then press **Apply** or **Cancel** |
| Settings are confusing | One very long page that mixes everyday options with developer ones | Split into clear categories, and hide the technical ones under **Advanced** |
| Settings asks for API keys | Several features need private keys that can't be built into the app | Move those keys to a small online service (Supabase, which GameLib already uses) |

### Game library problems
| Problem | Why it happens | The fix |
|---|---|---|
| No playtime for Fortnite or Valorant | Epic and Riot don't share past playtime. GameLib only counts time it sees itself, and neither game has been seen yet | Also recognise the games by their program name. Show "Not tracked yet" instead of 0h |
| Valorant has no cover | Its only match is a Wikipedia logo in a format GameLib can't open | Skip logos, and get covers from IGDB, a large game database |
| Riot Client shows up as a game | A small spelling check (capital letters) lets it through | Label it as a **launcher**, kept separate from your games |
| Wallpaper Engine counts as playtime | It's a Steam app, but GameLib treats it as a game | Label apps separately and don't track their time |
| Steam only shows installed games | GameLib reads only what's on your PC | Add a **Link Steam** button (Steam's own sign-in) to show every game you own |
| Recommendations only show games you already own | They're picked from your library | Add a "You might like" row of new games from IGDB |

### Looks and layout
| Problem | The fix |
|---|---|
| Text looked small | Mostly because the setting was at 75%. Keep the default at 100% and make small grey text a little bigger |
| Fonts feel messy | There are about 30 different text sizes. Cut them to 6 that are used everywhere |
| Sidebar cut off at big text sizes | Cut it from 13 icons to about 7, with a **More** menu for the rest |
| Opens on the library | Open on **Home** instead (recent games, recommendations) |
| No icon on the .exe | Add a proper GameLib icon to the exe, the window and the tray |

---

## Decisions made
- ✅ **IGDB:** use it (fine for personal, non-commercial use)
- ✅ **AI Assistant:** keep it, as an optional setting you turn on
- ✅ **Admin panel:** removed from public releases entirely
- ✅ **Riot Client and other launchers:** labelled "launcher", not "game"
- 🟡 **Discord Rich Presence:** probably removing it

## Still to decide
- Keep the Discord "post my sessions to a channel" feature?
- Use Epic's unofficial playtime data? (Recommended: no)
- Who registers the free Steam, SteamGridDB and IGDB developer keys for the online service?

---

## Order of work
1. **Fix what's broken:** login, couch mode, text slider, open on Home
2. **Tidy up:** remove the admin panel and unneeded key fields, shorten the sidebar
3. **New look:** consistent text sizes and spacing, reorganised Settings
4. **Library fixes:** launchers and apps labelled, playtime fallback, Valorant cover, app icon
5. **Online features:** Link Steam, owned games, IGDB recommendations
6. **Final check:** a fresh install should never show the words "API" or "key"
