# Ripae -- Landmark `0.2 "Stowage"`

> Second landmark. Builds on `0.1 "Dockside"` (see `RIPAE-ALPHA.md`); the stack, working agreement and commit format there still govern.
> Decided in the design session of 8 Nov 2026 against the interactive mock `design/ripae-crt-mock.html`. Where this document and the mock disagree, the mock is the intent and this document is the error -- fix the document.

## 1. Definition of Done

- [ ] The site wears the **CRT terminal** look (§3) on every screen: splash, ledger, leaderboard, overlays.
- [ ] A run is a **voyage** carrying one or more **cargo loads**; each load is made of **crates** (§4). Loads can be **merged into** another voyage or **detached** into a voyage of their own.
- [ ] A load is **bought** per SCU *or* per lot; the other figure is derived and both are stored.
- [ ] **Where a crate is lives on the crate** (`awaiting_pickup` / `aboard` / `stored at …`), set from the crate manifest one at a time or with `[ALL ABOARD]` / `[ALL STORED]`. A load's status is *derived*: all crates agree → that status; they differ → `SPLIT`; all sold → `sold`. The New Run form still takes one status for the whole voyage and stamps it onto every crate.
- [ ] Each load carries a **stop №** and a **destination**; the **Stowage Plan** panel derives a loading order per voyage (last stop deepest) and a working **sc-cargo.space** link from the voyage's ship and crate mix (§6).
- [ ] Individual crates can be marked **glitched** or **lost**; lost SCU is written off and shown.
- [ ] **Closing** a load is done per crate: pick which crates sold (`[ALL SOUND CRATES]` / `[NONE]`; glitched crates must be ticked by hand; lost ones can't be), sell price per SCU or per lot, and optional **profit shares** to other crew by percentage or flat fee. On a losing sale no share of either kind is paid. A share may be **forfeited**: the earner keeps the leaderboard credit, the aUEC goes to the member named (so a captain can bankroll a first officer without losing his place on the board). Kept crates stay open on the voyage.
- [ ] Recording a sale flickers the screen to black and shows the **SALE** banner (profit, route, shares, what the seller keeps).
- [ ] A **Leaderboard** screen: crew standings sortable by profit / runs / SCU, org totals, recent shares. Clicking a crew name flickers to black, then **HOORAY** (rank 1) or **WOMP WOMP** (everyone else) and that member's **service record** raster in together, top to bottom, as one screen.
- [ ] The **splash screen is the login**: ASCII `RIPAE`, boot log ending `IDENTIFY TO PROCEED`, then *Speak, friend, and enter* with handle and password; a wrong pair shows *The door does not open* in red on the same screen. With a valid session cookie the same splash runs without the gate: boot log ends `SESSION VALID · <name>`, then *Welcome back, <name> · Press any key · [LOG OUT]*. `[LOG OUT]` (rail or splash) clears the session and shows the gate; `[CLOSE]` in the footer replays the splash in whichever mode applies. `/login` renders the gate; `/` redirects there when unauthenticated.
- [ ] **Deleting a voyage** flickers to black and rasters in a `DELETE` confirmation (amber banner, the voyage's loads and committed aUEC, red warning); only `[DELETE VOYAGE]` deletes, `[KEEP IT]` returns. No `hx-confirm` dialogs anywhere.
- [ ] No horizontal scrollbar at any width; under ~980 px table rows become labelled cards.
- [ ] `cargo test` passes; the new money maths (per-lot pricing, partial sales, shares, write-offs) has unit tests.
- [ ] `CHANGELOG.md` gains a `0.2.0` entry; version badge reads `v0.2.0`.

## 2. Scope Fence

| In (0.2) | Deferred |
|---|---|
| CRT design system, splash, screen flicker | Discord OAuth / org roster beyond the seeded two (0.3) |
| Voyages → loads → crates; merge / detach | UEX import; Ripae's own price feed (0.4) |
| Four load statuses; per-lot buying | Reminders / Discord pings for stale cargo |
| Stowage plan + sc-cargo.space deep link | Reading sc-cargo.space grids back into Ripae |
| Crate states: aboard / glitched / lost | Insurance claims, refunds |
| Per-crate close; profit shares (% or flat) | Settling shares (who has actually paid whom) -- record only |
| Leaderboard, service record, verdict banners | Seasons / resets of the leaderboard |
| Ships lookup seeded from sc-cargo.space (80 ships) | Terminals lookup from UEX (0.4) |

**Design constraints carried forward from 0.1:** every load is still a price observation (commodity, terminal, price-per-SCU, timestamp, submitter) -- the per-lot price is stored *alongside* the derived per-SCU price, never instead of it.

**Names (changed from 0.1.1):** the pick-lists are *suggestions, not gates*. Any terminal or commodity name is accepted as typed, trimmed; a name on the list is stored with the list's spelling, a name off the list is stored verbatim. The 0.1 starter lists miss real locations and the UEX import in 0.4 is where names get reconciled; until then nobody is blocked from logging a run.

## 3. Design System -- "MU-TH-UR"

Inspired by the ship's-computer screens of *Alien* (1979): green phosphor on black glass. Single committed dark theme; there is no light mode.

| Token | Value | Use |
|---|---|---|
| `--bg` | `#050806` | glass |
| `--bg-2` | `#0a120c` | panel field |
| `--fg` | `#7dff8a` | phosphor text |
| `--fg-dim` | `#3f9a4c` | labels, hints, low beam |
| `--fg-faint` | `#1d4a26` | rules |
| `--amber` | `#f2b23a` | `STORED`, glitched crates, womp-womp |
| `--cyan` | `#5fd8e6` | `AWAITING PICKUP` |
| `--red` | `#ff5a4e` | losses, lost crates, destructive commands |
| `--glow` | `0 0 2px rgba(125,255,138,.75), 0 0 10px rgba(125,255,138,.28)` | text-shadow everywhere |

- **Type.** `Share Tech Mono` for everything; `VT323` for big figures and the `RIPAE` wordmark. Both vendored into `static/` (no Google Fonts call from the container). Tabular numerals.
- **Scanlines + vignette** as a fixed `body::after` overlay; never on top of inputs' hit areas (pointer-events none).
- **Commands, not buttons.** Every action is bracketed text: `[EDIT]`, `[CLOSE]`, `[DEL]`; inverse video on hover/focus. Destructive ones in red. Primary command gets a 1px box.
- **Panels** are 1px-bordered boxes with a `// TITLE` header and a dim hint on the right.
- **Status tags** are bordered uppercase chips: green solid `IN TRANSIT`, amber `STORED`, cyan dashed `AWAITING PICKUP`, amber double-rule `SPLIT` when a load's crates are in more than one place. Stored and awaiting rows also get a 3px left stripe in their colour.
- **Crate tiles** show size large, `#n · condition` small; green sound, amber dashed glitched, red struck-through lost. Beneath each tile a dotted toggle names its whereabouts (`ABOARD` / `STORED` / `AWAITING PICKUP`).
- **The raster reveal.** Anything that appears or disappears paints *line by line*, each line as a whole, on an uneven tick (120 ms, every third line 210 ms), with a 90 ms flash as it lands. No easing, ever. Disappearing runs the same sequence bottom-up. Honour `prefers-reduced-motion` (instant). Reference implementation: `raster(lines, on, done)` in the mock.
- **Blackout.** Sale banner and verdicts begin with the whole screen flickering to black (`steps()` keyframes, ~0.9 s), then the overlay rasters in; `[CONTINUE]` flickers back out.
- **ASCII banners** use the `ansi_shadow` figlet face: `RIPAE`, `SALE`, `HOORAY`, `WOMP WOMP` (two-line, amber).
- **Copy.** Uppercase for labels and commodities; `CREW MEMBER: LORD GIG` in the rail (no handle there; the handle lives on the leaderboard). Rail subtitle: `LEDGER TERMINAL · NISC CARGO`.
- **Responsive.** Tables never scroll sideways: cells wrap, short names from the lookups, and below 980 px each row becomes a card with `data-l` column labels (set once from `thead` by a small script).

Implementation note: keep `pico.min.css` out of the new pages entirely; the mock's stylesheet is ~300 lines and replaces it. `htmx` stays for fragments.

## 4. Data Model

```sql
-- migrations/0002_lookups.sql  (ships: 80 hulls scraped from sc-cargo.space, 8 Nov 2026 -- see design/ships-sc-cargo.csv)
CREATE TABLE ships (
  id INTEGER PRIMARY KEY, manufacturer TEXT NOT NULL, name TEXT NOT NULL UNIQUE,
  short TEXT NOT NULL, sc_cargo_slug TEXT NOT NULL, capacity_scu INTEGER NOT NULL, concept INTEGER NOT NULL DEFAULT 0
);

-- migrations/0003_stowage.sql
CREATE TABLE voyages (
  id          INTEGER PRIMARY KEY,
  logged_by   INTEGER NOT NULL REFERENCES users(id),
  ship_id     INTEGER REFERENCES ships(id),        -- NULL = no hull yet
  bought_at   TEXT NOT NULL,                       -- real calendar date, ISO-8601
  notes       TEXT,
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE loads (
  id             INTEGER PRIMARY KEY,
  voyage_id      INTEGER NOT NULL REFERENCES voyages(id),   -- merge/detach = UPDATE this
  submitted_by   INTEGER NOT NULL REFERENCES users(id),
  -- no status column: derived from crates (see §4 derived figures)
  commodity      TEXT NOT NULL,
  buy_terminal   TEXT NOT NULL,
  price_mode     TEXT NOT NULL CHECK (price_mode IN ('scu','lot')),
  buy_price_scu  REAL NOT NULL CHECK (buy_price_scu >= 0),   -- always stored (derived if mode = lot)
  buy_price_lot  REAL CHECK (buy_price_lot IS NULL OR buy_price_lot >= 0),
  stop_no        INTEGER NOT NULL DEFAULT 1 CHECK (stop_no >= 1),
  destination    TEXT,
  sold_at        TEXT, sell_terminal TEXT,
  sell_price_scu REAL CHECK (sell_price_scu IS NULL OR sell_price_scu >= 0),
  sell_price_lot REAL,
  notes          TEXT,
  created_at     TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
  CHECK (sold_at IS NULL OR (sell_terminal IS NOT NULL AND sell_price_scu IS NOT NULL))
);

CREATE TABLE crates (
  id         INTEGER PRIMARY KEY,
  load_id    INTEGER NOT NULL REFERENCES loads(id) ON DELETE CASCADE,
  size_scu   INTEGER NOT NULL CHECK (size_scu IN (1,2,4,8,16,24,32)),
  condition  TEXT NOT NULL DEFAULT 'sound' CHECK (condition IN ('sound','glitched','lost','sold')),
  whereabouts TEXT NOT NULL DEFAULT 'aboard' CHECK (whereabouts IN ('awaiting_pickup','aboard','stored')),
  stored_at  TEXT,                                 -- station, when whereabouts = 'stored'
  sold_at    TEXT,                                 -- set when condition = 'sold'
  CHECK (whereabouts <> 'stored' OR stored_at IS NOT NULL)
);

CREATE TABLE shares (
  id         INTEGER PRIMARY KEY,
  load_id    INTEGER NOT NULL REFERENCES loads(id) ON DELETE CASCADE,
  to_user    INTEGER NOT NULL REFERENCES users(id),   -- who EARNED it (leaderboard credit)
  paid_to    INTEGER NOT NULL REFERENCES users(id),   -- who RECEIVES the aUEC; = to_user unless forfeited
  kind       TEXT NOT NULL CHECK (kind IN ('pct','flat')),
  amount     REAL NOT NULL CHECK (amount >= 0),       -- percent (0-100) or aUEC
  reason     TEXT,
  paid_auec  REAL NOT NULL                            -- frozen at close; 0 on a losing sale (both kinds)
);

CREATE INDEX loads_voyage ON loads(voyage_id);
CREATE INDEX loads_status ON loads(status);
CREATE INDEX crates_load  ON crates(load_id);
```

**No migration from 0.1.** The 0.1 database holds nothing worth keeping (ruled 8 Nov). `migrations/` is rewritten from scratch for 0.2: `0001_users.sql`, `0002_lookups.sql` (commodities, terminals, ships with the sc-cargo columns from the start), `0003_stowage.sql` (voyages, loads, crates, shares). The `runs` table never existed as far as 0.2 is concerned; spacedock's `/var/lib/ripae/ripae.db` is deleted on deploy and reseeded.

**Derived figures (Rust, tested, never stored):**

```
load.status       = 'sold' if every crate sold/lost; else the one whereabouts all live crates share; else 'split'
load.where_now    = "n aboard <owner>'s <ship> · m stored at <station> · k awaiting pickup"  (only the non-zero parts)
load.scu          = Σ size_scu over crates where condition ≠ 'lost'
load.cost         = scu(all crates incl. lost) × buy_price_scu
load.written_off  = Σ size_scu over lost crates × buy_price_scu
sale.scu_sold     = Σ size_scu over crates sold in this close
sale.revenue      = scu_sold × sell_price_scu          (or sell_price_lot when mode = lot)
sale.cost_sold    = scu_sold × buy_price_scu
sale.profit       = revenue − cost_sold
share.paid        = 0 when profit ≤ 0 (ruled: nothing is owed on a loss, flat fees included)
                    else profit × amount/100 (pct) | amount (flat)
seller_keeps      = profit − Σ share.paid
board_credit(u)   = Σ seller_keeps where u sold  +  Σ share.paid where share.to_user = u      -- net of shares
cash(u)           = Σ seller_keeps where u sold  +  Σ share.paid where share.paid_to = u      -- who actually holds the aUEC
committed         = Σ cost over open loads − written_off
realised          = Σ profit over sold crates
```

**Partial close.** Selling some crates of a load keeps the load open with the remaining crates; the sold crates are recorded with the sale's terminal/price/date. Simplest implementation: split the load -- the sold crates move to a new `loads` row (same voyage, status `sold`); the original keeps the rest. This keeps "one load = one sale" true and the ledger flat.

## 5. Routes

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/` | Ledger terminal: totals, Open Cargo (grouped by voyage), Stowage Plan, New Run (collapsed), Ledger |
| `GET` | `/board` | Leaderboard: standings, org totals, recent shares |
| `GET` | `/crew/:id` | Service-record fragment (htmx) shown under the verdict banner |
| `POST` | `/voyages` | New run: voyage fields + 1..n loads, each with crate mix `2×32, 4×8` (parser accepts `,` ` ` `x` `×`) |
| `POST` | `/voyages/:id/loads` | Add a load to an existing voyage |
| `POST` | `/loads/:id/move` | Merge into another voyage (`voyage_id`) or detach (`voyage_id = new`) |
| `GET/PUT` | `/loads/:id/edit` | Inline edit incl. status, stop №, stored-at |
| `GET` | `/loads/:id/crates` | Crate manifest fragment |
| `POST` | `/crates/:id/condition` | Cycle sound → glitched → lost |
| `POST` | `/crates/:id/where` | Toggle aboard ↔ stored (with `stored_at`) |
| `POST` | `/loads/:id/where` | All crates aboard / all stored |
| `GET` | `/loads/:id/close` | Close fragment: crate picker, sale price, shares |
| `POST` | `/loads/:id/close` | Record the sale; respond with the SALE banner fragment |
| `GET` | `/voyages/:id/delete` | Confirmation overlay fragment (loads, SCU, committed aUEC) |
| `DELETE` | `/loads/:id`, `/voyages/:id` | Delete; a voyage delete cascades to its loads and crates after confirmation |

Ownership rule from 0.1.1 stands: any crew member may edit, close or delete any load.

## 6. sc-cargo.space Link

Grammar, read from the site's bundle (`index-Cy0P0zyR.js`, 8 Nov 2026):

```
https://sc-cargo.space/#/viewer/<ship>-<official|unofficial>[-q<n>][-w<n>][-e<n>][-r<n>][-t<n>][-y<n>][-u<n>][,<ship2>…]
<ship>  = ships.sc_cargo_slug  (= name lower-cased; spaces URL-encoded)
q w e r t y u = crate sizes 1 2 4 8 16 24 32, in that order; omit sizes with count 0
```

Example: Hull C with 2×8 and 4×32 → `hull-c-official-r2-u4`. Build it from the voyage's ship and every non-lost crate across its loads. If the voyage has no ship, the stowage plan says so and offers no link.

Stowage order: loads sorted by `stop_no` descending (last stop loads first, "deepest"); within a stop, larger crates first. Label the first entry `DEEPEST`, the lowest-stop entries `DOOR SIDE`, anything between `MID HOLD`.

**Unassigned voyages.** Voyages without a hull are clustered: two voyages join a cluster when any of their loads share a destination (transitively). A cluster of one gets its own card ("no hull yet"). A cluster of several gets a *Suggested* card (amber stripe) listing the combined load order and a `[MERGE a + b INTO VOYAGE a]` command (lowest number wins). Loads whose destination is undecided never cause a pairing. The panel re-derives itself after any change to Open Cargo.

Every panel header carries `[HIDE]`/`[SHOW]`; the body rasters out and in. New Run is hidden by default; the others shown.

## 7. Leaderboard & Service Record

Standings per user, closed sales only: profit **net of shares** (`board_credit`: what you kept as seller plus shares earned, forfeited or not), runs closed (voyages with all loads sold), loads sold, SCU hauled, best route, share of org profit (meter). Sort by profit (default), runs, SCU.

Service record fields: lifetime profit (board credit), cash position (`cash(u)`, shown beside it when it differs because of forfeits), runs closed, loads sold, SCU hauled, best trade, worst trade, favourite commodity, favourite route, crates lost, shares paid out, shares received, ships flown, first sale, latest sale. Verdict: rank 1 by *profit* gets `HOORAY`; all others `WOMP WOMP`. Commentary lines are a small random pool; keep them short and unkind only to the ledger.

## 8. Order of Work

1. Stylesheet + base template in the CRT system; splash; footer `[CLOSE]`. *(ledger still on old schema)*
2. Migrations 0004 (ships) and 0005; data migration from `runs`; tests for derived figures.
3. Voyage / load / crate models and repo; New Run with multiple loads and the crate-mix parser.
4. Open Cargo grouped by voyage; merge / detach; crate manifest.
5. Close per crate, price modes, shares; SALE banner.
6. Stowage plan + sc-cargo.space link.
7. Leaderboard, service record, verdict banners.
8. Responsive card layout; reduced-motion pass; `CHANGELOG.md` 0.2.0; tag.

## 9. Rulings (8 Nov 2026, Lord G.)

- A `pct` share on a loss pays 0. A `flat` share on a loss **also pays 0** -- nothing is owed on a losing run.
- `glitched` crates are **sellable by hand-pick only**; excluded from `[ALL SOUND CRATES]`.
- Leaderboard credit is **net of shares**; a share may be **forfeited** to another member (`shares.paid_to`), keeping the credit with the earner.
- Pick-lists are suggestions; **any location name is accepted** until the 0.4 UEX import.
- The 0.1 database is **wiped**; migrations are rewritten, no data migration.
- **Tests first**: the derived-figure table above becomes the unit-test list before any handler is written; `proptest` covers the money maths with random crate mixes, prices and share sets (invariants: `Σ share.paid + seller_keeps = profit`; `committed − written_off ≥ 0`; `cash` sums to `board_credit` sums across the org).
- **One roll** (roll 7 "Stowage") carries all of 0.2; see `ANALYSIS-0.1.md` §4 for its internal milestones.
