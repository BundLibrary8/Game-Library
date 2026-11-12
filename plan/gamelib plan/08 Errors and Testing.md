---
tags: [gamelib, design]
status: draft-needs-review
section: 5
---
# 08 Errors and Testing (Section 5, draft)

Back to [[00 Home]] · Prev: [[07 UI Screens]] · Next: [[09 Milestones]]

> [!warning] Draft
> Not agreed yet.

## Error handling
- Core functions return `anyhow::Result<T>`, and you use `?` to pass errors up. **No `.unwrap()` outside tests** except where a failure is truly impossible.
- **Errors never crash the app.** They become a message in the status bar, or a small red notice for serious problems (e.g. the database can't be opened).
- **One failing thing doesn't stop others:** one source fails, the rest still scan. One cover fails, the rest still download. A game fails to launch: show "Couldn't start <game>: <reason>".
- **Common cases to handle:**
  - Launcher not installed: that source is skipped quietly (shown as "not found" in Settings)
  - Exe moved or deleted: game shown as "not installed", Play disabled, "Locate exe…" offered
  - No internet or no API key: covers are skipped, everything else works
  - Corrupt or odd manifest file: skip that one game and log it
- A **log file** at `%APPDATA%\gamelib\gamelib.log` (crates `log` + `simplelog`), useful for debugging and for coursework evidence.

## Testing
Run everything with `cargo test`.

| What | How |
|---|---|
| **Epic parser** | sample `.item` JSON files in `tests/fixtures/` → check title, launch URI |
| **Folder scan rules** | junk-exe filter and "pick the best exe" on lists of fake file names |
| **Database** | `Connection::open_in_memory()`: insert, rescan (no duplicates, playtime kept), tags, collections |
| **Playtime logic** | the pure state function from [[05 Launching and Playtime]], fed fake process lists and times |
| **Stats queries** | insert known sessions → check totals and top-10 order |
| **Steam / GOG / UI** | manual test checklist, run on my own PC before finishing each milestone |

- Write the test **before** the code for parsers and logic. This is test-driven development, and it works well when learning, because the test tells you when you're done.
- Run `cargo clippy` (Rust's linter) and `cargo fmt` before every commit.
- Optional extra for the portfolio: **GitHub Actions** running `cargo test` + `cargo clippy` on every push, which gives a green tick on the repo.
