---
tags: [gamelib, design, feature]
status: draft-needs-review
created: 2026-11-12
---
# 17 Shop

Back to [[00 Home]] · Related: [[16 Profile and Friends]] · [[15 Design Variants]]

A shop for **cosmetics**: library backgrounds, profile banners, badges and name changes.

> [!important] Proposal: pay with Stardust earned by playing, not real money
> The shop uses **Stardust ✦**, a points currency you **earn by playing your games** (tracked by [[05 Launching and Playtime]]). This makes it a reward system. **Real-money purchases are out of scope.** The reasons are in the table below.

> [!info] Plan review, 13 Nov 2026
> Built ✅ as described. **Stardust stays**, still cosmetic-only with no real money. Proposed expansion (XP and levels, idle filter and daily cap on playtime rewards, themes, profile layouts, server-side ledger once accounts exist) is in [[26 Progression and Stardust Economy]]. The review found two small exploits in today's rules (idle playtime pays without limit; deleting and re-adding a game lets "finished" pay twice), both listed there.

## Mockup
![[gamelib-08-shop.png]]

- **Shop** tab in the top bar, and a **✦ balance** next to the avatar on every screen.
- Category chips: All · Library backgrounds · Profile banners · Badges · Name change · Owned.
- A **Featured** item at the top. Here it's the **Constellation** library background from [[B - Constellation]], so that design becomes something you unlock.
- A panel showing **how to earn Stardust**.
- Item cards show a preview, name, type and price, or **✓ Owned**.
- **Preview** before buying (backgrounds and banners show on your library or profile for 10 seconds).

## What's for sale (starter catalogue)
| Item | Type | Price | Notes |
|---|---|---|---|
| Constellation | Library background | ✦ 800 | Star map + optional twinkle ([[B - Constellation]]) |
| Aurora | Library background | ✦ 600 | Green and violet bands |
| Synthwave Grid | Library background | ✦ 600 | Retro horizon |
| Nebula, Pixel Forest, … | Profile banner | ✦ 300 | |
| Night Owl, Speedrunner, Co-op Captain… | Badge | ✦ 150 | Shown next to your name (max 3) |
| Name change | Service | ✦ 500 | Your first name is free |
| Founder | Badge | Free | Given to everyone who sets up GameLib in its first version |

The catalogue is a **list built into the app** (a Rust `const` array of `ShopItem`s), not downloaded. Adding items means releasing a new version, which is fine for a local app.

## Earning Stardust
| Action | Reward |
|---|---|
| Every hour of tracked playtime | ✦ 10 (counted per session, rounded down to whole minutes) |
| Finish a game (move it to **Finished**, needs ≥ 1 h played) | ✦ 50, **once per game** |
| First game launched each day | ✦ 5 |
| Playtime milestones (100 h, 250 h, 500 h…) | A free **earned banner** (see [[16 Profile and Friends]]) |

- Every change goes in a **ledger** table: every reward and every purchase is a row, and the balance is the sum. This makes it easy to test and to show **why** you have the balance you have. It's also good coursework evidence of careful design.
- Because it's all local, someone *could* edit the database to give themselves Stardust. That's fine: nothing has real value, and only they would see the result. This is a good reason to keep real money out.

## Why no real money (for now)
| Real-money shop would need | Why it's a problem here |
|---|---|
| A **payment provider** (e.g. Stripe, PayPal) | The account holder must be an adult or a registered business, with identity checks |
| An **online server + accounts** | Something has to remember what each person bought. A local database can be edited, so purchases couldn't be protected |
| **Legal and tax** | UK consumer law (refunds, clear pricing), VAT, privacy rules for storing payment and customer data |
| **Security** | Handling money makes you a target. Mistakes cost real people real money |

If you ever want real purchases, it would be a **separate, later project** built on the server from [[16 Profile and Friends#Friends list: the options]] Option 2, with an adult responsible for the payment account.

## Data (adds to [[03 Data Model]])
```sql
stardust_ledger(id INTEGER PRIMARY KEY, amount INTEGER NOT NULL,   -- + earned, - spent
                reason TEXT NOT NULL,      -- 'playtime' | 'finished' | 'daily' | 'purchase'
                game_id INTEGER, item_id TEXT, created_at INTEGER NOT NULL)
owned_items(item_id TEXT PRIMARY KEY, acquired_at INTEGER NOT NULL, how TEXT NOT NULL)  -- 'bought' | 'earned' | 'gift'
-- equipped choices live in settings: 'equipped.library_bg', and in profile.banner / profile_badges
```

```rust
pub enum ItemKind { LibraryBackground, ProfileBanner, Badge, NameChange }
pub struct ShopItem { pub id: &'static str, pub name: &'static str, pub kind: ItemKind, pub price: u32 }
```

### Buying something (in one database transaction)
1. Check you don't already own it (except Name change, which is used up when spent).
2. Check balance ≥ price.
3. Insert a **negative** ledger row and an `owned_items` row **together**. If either fails, neither is saved.

## Tests
- Balance = sum of ledger rows; buying reduces it by exactly the price.
- **Can't buy with too little Stardust**; can't buy the same item twice.
- Reward rules: 90 minutes → ✦ 15; "finish" twice on one game → only one ✦ 50; finishing a game with 20 minutes played → nothing.
- A failed purchase leaves **both** tables unchanged (transaction test).
