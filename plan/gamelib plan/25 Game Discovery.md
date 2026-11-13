---
tags: [gamelib, design, discovery]
status: proposed
created: 2026-11-13
phase: 3
---
# 25 Game Discovery

Back to [[00 Home]] · Vision: [[19 Product Vision#E. Discovery: from your library outwards]] · Roadmap: [[28 Expanded Development Roadmap#Phase 3 – Personal Discovery & Progression (local-first)]]

> [!abstract] Status: 🔭 future. Most of it is **local** and comes *before* accounts
> Moved earlier than the original suggestion (it was last): wishlists, release tracking, price comparison and "what to play next" need **no backend**, only metadata from Phase 1. Friends- and community-based discovery waits for Phases 4–5.

## Rule: discovery, not a storefront
GameLib never sells anything. Every "buy" action opens the store's own page (Steam, GOG, Epic…) in the browser. Price data comes from aggregator APIs, shown with attribution.

## Features by dependency
| Feature | Needs | Phase |
|---|---|---|
| **What should I play next?** Backlog picker from *your* library: unplayed / short / matching genres you play most / not touched in N months | Local library + genres (IGDB, Phase 1) | 3 |
| **Wishlist** (games you don't own) | Canonical game records without installations/ownerships ([[20 Game Identity and Installation Model]]) | 3 |
| **Release tracking** (upcoming games, release date changes, "released today" notice) | IGDB release dates, refreshed weekly | 3 |
| **Price comparison & price alerts** for wishlisted games | IsThereAnyDeal API (free key) or CheapShark | 3 |
| **"Because you play X"** content-based recommendations | IGDB genres/themes/similar games for your top-played games | 3 |
| **Genre & tag browsing** of IGDB (incl. upcoming indie games) | IGDB queries, cached | 3 |
| **Played by friends** | Steam friends' games (Steam Web API, if their profiles are public) ✅ partially (friends list exists); GameLib friends (Phase 4) | 3 (Steam) / 4 |
| **Community-curated lists** | Backend + lists ([[23 Social Platform Architecture#Communities]]) | 5 |
| **Collaborative recommendations** ("people with libraries like yours…") | Many users' data on a server, privacy review | 6 / maybe never |

## Recommendation approach (simple first)
1. **Content-based, local:** score candidate games by overlap of genres/themes/keywords with your most-played games, weighted by your playtime; remove owned games; boost IGDB "similar games". Explainable ("Because you played Hades: roguelike, action"). Testable with fixtures.
2. **Backlog:** rules, not ML. E.g. "owned, < 1 h played, genre you like, IGDB time-to-beat < 15 h" 🔬 (IGDB has a time-to-beat resource; verify).
3. Only consider collaborative filtering if there are thousands of opted-in users, and only with aggregated data.

## Price data sources (researched 13 Nov 2026)
| Source | Access | Notes |
|---|---|---|
| **IsThereAnyDeal API v2** | Free API key by registering an app; key sent as query param or `ITAD-API-Key` header | Lookup by **Steam app ID** or exact title → ITAD game ID → prices/history. Some endpoints need explicit approval. Asks not to misuse the API. 🔬 check price endpoint terms & limits |
| **CheapShark API** | Free, no key | Deals across many PC stores; affiliate-funded. 🔬 check rate limits and attribution |
| Store APIs directly | Steam Store `appdetails` already used ✅ for info (includes price) | Fine for Steam only |

Show: current best price per store, historical low, "on sale until". Link to the store page (affiliate links are **out of scope**; they'd turn the project commercial).

## UI sketch (new screens)
- **Discover** tab in the top bar: "Play next" shelf, "Because you play…" shelves, "Upcoming from your wishlist", "On sale from your wishlist".
- **Game page (not owned):** IGDB info, screenshots, price table, ♥ Wishlist, release date, "Open in Steam/GOG/Epic".
- Re-use existing card/shelf drawing from `ui/home.rs` ✅.

## Out of scope
- Selling games or keys, affiliate links, ads.
- News feeds/journalism.
- Any recommendation that needs uploading the user's library without explicit opt-in.
