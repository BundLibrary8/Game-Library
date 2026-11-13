---
title: GameLib Development Roadmap
aliases:
  - GameLib Plan
  - Game Library Roadmap
project: GameLib
status: active
created: 2026-11-13
tags:
  - gamedev
  - rust
  - roadmap
  - gamelib
---

# GameLib Development Roadmap

> [!abstract] Project goal
> Turn GameLib into a polished, reliable, all-in-one desktop game library: one place to organise, launch, track, understand, and enjoy a user's PC games.

## How to use this roadmap

- Mark work complete by changing `- [ ]` to `- [x]`.
- Keep implementation notes and decisions in the linked feature notes.
- Prioritise reliability and user experience before adding large new systems.
- Treat the priorities as a suggested order, not a requirement to build every feature.

## Current project snapshot

Based on the repository review:

- Rust desktop application using **egui** for its interface.
- SQLite-backed game and session data.
- Integrations / sources include Steam, Epic, GOG, Battle.net, Riot, EA, manual entries, and folder scanning.
- Existing capabilities include game launching, playtime tracking, statistics, profiles, friends, collections, franchises, settings, and the Stardust shop.
- Session checkpointing and recovery are already present.
- The playtime tracker detects processes associated with a game's install directory and periodically checkpoints sessions.

## Priority overview

| Priority | Area | Why it matters | Suggested effort |
|---|---|---|---|
| P0 | Reliability, data safety, and testing | Prevent lost playtime, corrupted data, and regressions | Medium |
| P1 | Library organisation and filtering | Makes a large library easy to manage | Small–medium |
| P1 | Playtime tracking improvements | Makes existing core functionality more accurate | Medium |
| P1 | Statistics dashboard | Makes tracked data useful and engaging | Medium |
| P2 | Completion status, notes, and ratings | Adds personal library management | Small–medium |
| P2 | Achievements and milestones | Builds on the existing reward system | Medium |
| P2 | Game recommendations and discovery | Helps users choose what to play | Medium–large |
| P3 | Save backup manager | Adds meaningful protection for game progress | Large |
| P3 | More launchers and import tools | Improves library coverage | Medium |
| P3 | Cloud sync and social features | Makes GameLib useful across devices and with friends | Large |
| P4 | Mod manager and in-game overlay | Ambitious differentiating features | Very large |

---

# Phase 1 — Reliability and foundations

**Goal:** Make the existing application dependable before expanding it.

## 1. Data safety and database reliability

- [ ] Add automatic SQLite database backups.
- [ ] Use SQLite's backup API or a safe checkpoint-and-copy procedure rather than copying an actively written database file.
- [ ] Add a configurable backup retention policy (for example, keep the latest 7 daily backups and 4 weekly backups).
- [ ] Add a visible **Restore backup** workflow with a confirmation step.
- [ ] Validate database migrations on both fresh installs and upgrades from older versions.
- [ ] Add clear error messages when the database cannot be opened, migrated, or written.
- [ ] Add a diagnostics/export option that collects logs and app version without exposing secrets.

## 2. Automated quality checks

- [ ] Run `cargo fmt --check` in CI.
- [ ] Run `cargo clippy --all-targets -- -D warnings` in CI.
- [ ] Run `cargo test` in CI on every pull request.
- [ ] Add tests for session recovery after an unexpected shutdown.
- [ ] Add tests proving session checkpoints are not double-counted.
- [ ] Add tests for duplicate game imports and game upserts.
- [ ] Add tests for launcher URI validation and launch failures.
- [ ] Add tests for statistics around midnight, week boundaries, and time zones.
- [ ] Add integration tests using temporary SQLite databases.

**Definition of done:** A clean checkout builds, formats, passes Clippy, and passes tests. A simulated crash does not lose already-checkpointed playtime or double-count a recovered session.

---

# Phase 2 — Better library organisation

**Goal:** Make it fast to find and manage games.

## 3. Advanced filters and sorting

- [ ] Add filters for genre, platform/source, installed state, favourite state, completion status, and last played.
- [ ] Allow multiple filters to be combined.
- [ ] Add sorting by recently added, last played, total playtime, title, rating, and completion progress.
- [ ] Add a clear-all-filters action.
- [ ] Persist the user's preferred view, sort order, and filters.

## 4. Smart collections

- [ ] Allow users to create rule-based collections.
- [ ] Example: `Installed = true AND Last played > 30 days ago`.
- [ ] Example: `Completion status = Not started AND Favourite = true`.
- [ ] Update smart collections automatically when game data changes.
- [ ] Keep ordinary manual collections working independently.

## 5. Game details and personal notes

- [ ] Add a completion status: **Not started**, **Playing**, **Completed**, **100%**, and **Abandoned**.
- [ ] Add a personal rating (for example, 1–10 or a star rating).
- [ ] Add notes for each game.
- [ ] Add optional fields for difficulty, estimated completion time, and replay intention.
- [ ] Add bulk actions for favourites, tags, collections, and completion status.
- [ ] Detect likely duplicate entries and offer a safe merge workflow.

**Definition of done:** Users can organise a large library without manually creating every collection or repeatedly opening each game.

---

# Phase 3 — Playtime tracking improvements

**Goal:** Make playtime measurement more accurate and transparent.

## 6. More flexible process detection

- [ ] Let users choose the executable used to detect each game.
- [ ] Support multiple process names for games with launchers or helper processes.
- [ ] Add an option to ignore known background/helper processes.
- [ ] Add a manual **Start tracking** / **Stop tracking** control.
- [ ] Show why GameLib believes a game is currently running.
- [ ] Allow users to correct a session's start time, end time, or duration.

## 7. Session edge cases

- [ ] Handle Windows sleep, hibernation, and lock/unlock transitions.
- [ ] Avoid counting time while the PC is asleep.
- [ ] Decide how multiple instances of the same game should be counted.
- [ ] Improve handling of games that launch through a separate executable.
- [ ] Make the start timeout and exit grace period configurable if appropriate.
- [ ] Add a recovery screen when an interrupted session is detected.

**Definition of done:** Users can understand and correct tracking decisions, and sleep or launcher behaviour does not silently inflate playtime.

---

# Phase 4 — Statistics and personal insights

**Goal:** Turn play history into useful, attractive information.

## 8. Statistics dashboard

- [ ] Add selectable date ranges: 7 days, 30 days, 90 days, this year, and all time.
- [ ] Add daily and weekly playtime charts.
- [ ] Add most-played games and recently played games.
- [ ] Add playtime by launcher/source.
- [ ] Add average session length and number of sessions.
- [ ] Add longest session and longest streak.
- [ ] Add comparisons with the previous period.
- [ ] Make chart labels and time-zone behaviour consistent.

## 9. GameLib Wrapped

- [ ] Create an annual recap of the user's gaming activity.
- [ ] Include total hours, top games, top genres (where available), longest session, and monthly activity.
- [ ] Add a shareable image/export.
- [ ] Let users choose whether game titles or personal information appear in exports.

**Definition of done:** Statistics use clearly defined date ranges and handle sessions crossing midnight and calendar boundaries correctly.

---

# Phase 5 — Progression and motivation

**Goal:** Expand the existing Stardust shop into a coherent reward system.

## 10. Achievements and milestones

- [ ] Add milestones for total playtime.
- [ ] Add milestones for number of games played and completed.
- [ ] Add streak and exploration achievements.
- [ ] Add per-game milestones, such as first session or 10 hours played.
- [ ] Add achievement descriptions, unlock dates, and progress indicators.
- [ ] Add an achievements page and achievement notifications.
- [ ] Keep rewards balanced so users cannot easily exploit repeated launches or artificial sessions.

## 11. Stardust shop improvements

- [ ] Add more cosmetic items, themes, profile frames, and badges.
- [ ] Add rotating or seasonal shop items.
- [ ] Add a transaction history view.
- [ ] Add clear explanations for how Stardust is earned and spent.
- [ ] Add tests for balances, rewards, and duplicate transaction prevention.

**Definition of done:** Rewards are transparent, consistent, and calculated from trustworthy session data.

---

# Phase 6 — Discovery and recommendations

**Goal:** Help users decide what to play next.

## 12. Backlog assistant and recommendations

- [ ] Add a **What should I play?** feature.
- [ ] Let users filter suggestions by available time, mood, genre, and installed status.
- [ ] Recommend games from the user's existing library first.
- [ ] Use ratings, tags, completion status, and play history where available.
- [ ] Explain why a game was recommended.
- [ ] Add a **Surprise me** option.
- [ ] Allow users to dismiss recommendations and give feedback.

## 13. Game discovery

- [ ] Add optional external game metadata lookup (cover, description, genres, release date).
- [ ] Investigate providers such as IGDB or RAWG and check their current API terms before implementation.
- [ ] Cache metadata and artwork locally.
- [ ] Make external lookup optional and resilient to network failure.
- [ ] Add a manual metadata edit option.

**Definition of done:** Discovery remains useful even without an internet connection or external API key.

---

# Phase 7 — Integrations and convenience

**Goal:** Reduce the amount of manual setup required.

## 14. Additional launchers and import

- [ ] Investigate Ubisoft Connect.
- [ ] Investigate Xbox / Microsoft Store games.
- [ ] Investigate Amazon Games.
- [ ] Investigate itch.io.
- [ ] Investigate Rockstar Games Launcher.
- [ ] Add Playnite library import if it can be done safely and reliably.
- [ ] Add duplicate detection across launchers.
- [ ] Clearly distinguish installed games from games that are only owned or available through a launcher.

## 15. Artwork and loading performance

- [ ] Load artwork asynchronously so the UI does not freeze.
- [ ] Cache resized thumbnails.
- [ ] Add placeholders and retry handling for missing artwork.
- [ ] Avoid repeatedly downloading the same images.
- [ ] Add a way to refresh artwork for one game or the whole library.

**Definition of done:** Large libraries remain responsive, and missing network resources do not break the interface.

---

# Phase 8 — Larger features

These are substantial projects. Start them only after the earlier phases are stable.

## 16. Save backup manager

- [ ] Research save locations for supported games.
- [ ] Allow users to add custom save paths.
- [ ] Create timestamped save backups.
- [ ] Support backup before and after a game session.
- [ ] Add restore and backup browsing.
- [ ] Warn users before overwriting existing saves.
- [ ] Consider cloud storage only after local backup and restore are reliable.

## 17. Cloud sync

- [ ] Design account and device identity.
- [ ] Decide which data syncs (library metadata, collections, settings, sessions, artwork).
- [ ] Design conflict resolution for edits made on multiple devices.
- [ ] Keep local-first behaviour so GameLib remains usable offline.
- [ ] Encrypt sensitive data in transit and at rest where appropriate.
- [ ] Add sync status, retry, and recovery controls.
- [ ] Never treat a simple last-write-wins database upload as a safe sync strategy.

## 18. Friends and social features

- [ ] Improve friend profiles and presence information.
- [ ] Add optional friend activity feeds.
- [ ] Add comparisons of playtime and favourite games.
- [ ] Add privacy controls for visibility of games and play sessions.
- [ ] Keep external platform API usage within provider terms and rate limits.

## 19. Mod manager

- [ ] Research mod sources and supported game formats.
- [ ] Start with manual mod folders and profiles for a small number of games.
- [ ] Support enabling/disabling mods.
- [ ] Back up original files before changes.
- [ ] Detect conflicts and show clear warnings.
- [ ] Do not download or install mods without explicit user action.

## 20. In-game overlay

- [ ] Research a lightweight Windows overlay approach.
- [ ] Prototype a minimal overlay showing session time and playtime.
- [ ] Add optional notes or quick controls.
- [ ] Test compatibility with fullscreen and anti-cheat protected games.
- [ ] Make the feature optional and easy to disable.

---

# Technical implementation notes

## Database changes

For every new persistent feature:

1. Add a versioned database migration.
2. Keep migrations safe for existing users.
3. Add indexes for frequently filtered or sorted columns.
4. Add tests for fresh databases and upgrades.
5. Avoid destructive migrations unless there is a verified backup and migration path.

Potential future data concepts:

- `game_progress` — completion status, rating, and personal notes.
- `smart_collections` — saved filter rules.
- `achievements` and `user_achievements` — achievement definitions and unlock state.
- `game_process_rules` — executable/process matching rules per game.
- `save_backup_profiles` and `save_backups` — save paths and backup history.
- `recommendation_feedback` — dismissed or liked recommendations.

These are design suggestions, not assumptions about the current schema. Inspect existing tables and naming conventions before adding anything.

## Rust implementation practices

- Keep database operations out of egui rendering code.
- Keep process detection and network calls away from the UI thread.
- Use explicit error types and surface actionable errors to the user.
- Keep business logic testable independently from the UI.
- Prefer small, reversible changes over large rewrites.
- Add dependencies only when they provide a clear benefit.

## Networking

- Set request timeouts.
- Handle rate limits and transient failures with bounded retries/backoff.
- Cache API responses where appropriate.
- Do not block the UI while waiting for external services.
- Store API keys securely and never commit secrets.

## Suggested pull-request checklist

- [ ] The change has a clear user benefit.
- [ ] Existing data is preserved.
- [ ] Database migrations are tested, if applicable.
- [ ] Error and offline behaviour are handled.
- [ ] UI remains responsive.
- [ ] Tests have been added or updated.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --all-targets -- -D warnings` passes.
- [ ] `cargo test` passes.
- [ ] README or user documentation is updated if needed.

---

# Suggested first 10 tasks

Do these in order for a practical first development cycle:

1. [ ] Establish a clean baseline: run formatting, Clippy, and all tests.
2. [ ] Review existing database migrations and add migration tests.
3. [ ] Implement safe automatic database backups.
4. [ ] Add advanced library filters and sorting.
5. [ ] Add completion status and personal ratings.
6. [ ] Improve process detection with per-game executable rules.
7. [ ] Fix and test statistics around midnight and calendar-week boundaries.
8. [ ] Build a refreshed statistics dashboard.
9. [ ] Add achievements and milestones on top of the existing Stardust ledger.
10. [ ] Add smart collections.

# Development log

Use this section to record progress, decisions, and blockers.

## 2026-11-13

- Created the initial roadmap from the repository review.
- Initial recommendation: prioritise reliability, data safety, testing, library organisation, and playtime accuracy before starting cloud sync, mod management, or overlays.

## Future entries

- 

---

# Related notes

Create these notes as the project grows:

- [[GameLib Architecture]]
- [[GameLib Database Schema]]
- [[GameLib Playtime Tracking]]
- [[GameLib UI Ideas]]
- [[GameLib Technical Debt]]
- [[GameLib Release Checklist]]
