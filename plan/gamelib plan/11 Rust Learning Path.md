---
tags: [gamelib, learning]
---
# 11 Rust Learning Path

Back to [[00 Home]]

Learn just enough before each milestone, then learn the rest by building. The main resources are free:
- **The Rust Book**: doc.rust-lang.org/book
- **Rustlings**: small exercises that fix broken code (github.com/rust-lang/rustlings)
- **Rust by Example**: doc.rust-lang.org/rust-by-example
- **egui demo**: egui.rs (every widget, with source)

## Before M0/M1
| Topic | Book chapter | Where it shows up |
|---|---|---|
| Variables, functions, control flow | 1–3 | everywhere |
| **Ownership and borrowing** (the big one) | 4 | passing `&mut AppState` to UI functions |
| Structs | 5 | `Game`, `AppState` |
| **Enums and `match`** | 6 | `Source`, `LaunchTarget` |
| Modules | 7 | `db.rs`, `sources/`, `ui/` |
| `Vec`, `String`, `HashMap` | 8 | the game list, search |
| **Errors: `Result` and `?`** | 9 | every DB and file function |

Rustlings sections to do: `variables` → `functions` → `if` → `move_semantics` → `structs` → `enums` → `strings` → `modules` → `vecs` → `hashmaps` → `options` → `error_handling`.

## Before M2
| Topic | Book chapter | Where |
|---|---|---|
| **Traits** | 10 | `GameSource` |
| Closures and iterators | 13 | filtering and sorting the game list |
| **Threads and channels** | 16 | `worker.rs`, the most important one for this app |
| Testing | 11 | parser and DB tests |

Also: the `serde` "derive" basics, for reading Epic's JSON.

## Before M3/M4
- `Arc` and `Mutex` (ch. 16) only if needed. Channels should cover most cases.
- Reading the `sysinfo` and `reqwest` docs on **docs.rs**. Learning to read crate docs is a skill in its own right.

## When the compiler complains
- Read the **whole** error message. Rust's errors often tell you the fix.
- "borrowed value does not live long enough" / "cannot borrow as mutable" → reread Book ch. 4. Often the fix is to `.clone()` something small, and that's fine while learning.
- Ask Claude to **explain** the error rather than just fix it, so you learn it.
