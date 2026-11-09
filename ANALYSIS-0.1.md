# Ripae -- Where the Alpha Stands (8 Nov 2026)

> Review of `RIPAE-ALPHA.md` (0.1 "Dockside") against the code on `roll/6-1108-design-work`,
> and of the original landmark plan against the 0.2 "Stowage" brief written the same evening.
> `cargo test`: 24 passed.

## 1. Alpha 0.1 "Dockside" -- scorecard

| §1 Definition of Done | State | Note |
|---|---|---|
| Reproducible container (`nix build .#container` / `docker build`) | partial | Nix image builds; Dockerfile has never been run |
| Runs on spacedock via `oci-containers`, SQLite bind-mount | done | `0.1.2` live at nisc.us |
| Two seeded accounts | done | |
| Login; log / edit / delete; shared table | done | Edit broke and was mended in the `0.1.3` hotfix |
| Open cargo with status / destination / stored-at | done | |
| Close a run → `sold` | done | |
| Per-run profit, running totals | done | |
| `/healthz` | done | Not yet on the Gatus status page |
| `cargo test` with profit maths | done | 24 tests |
| README: dev, image, deploy, password rotation | done | Rotation is a manual SQL `UPDATE` |

Beyond the brief, rolls 1–5 added shared crew edits, fixed pick-lists with fuzzy search, the version badge and changelog, a dev-shell fix, and real-calendar dates. **Dockside is done in substance.**

### Loose ends

| Item | Why it matters |
|---|---|
| No release tag has ever been cut (`v0.1.0`–`v0.1.3` absent from origin) | `tag_on_graduate = true` in `.roll-flow.toml`, so graduations are happening outside roll-flow or not at all. Badge says `0.1.2`, `Cargo.toml` says `0.1.3-roll6`. |
| `hotfix/1-1108-fix-unable-to-edit-job` still on origin | Merged (changelog has 0.1.3) but not pruned |
| Dockerfile unverified | Harmless whilst Nix is the path; test it or delete it |
| `/healthz` not wired into `status.gignsky.com` | The brief's stated reason for the route |
| No password-rotation CLI | Fine for two users; a chore at 0.3 |
| Classless Pico with `.grid` no-ops | Moot: 0.2 drops Pico |

## 2. The landmark plan, then and now

| Landmark | Original scope (`RIPAE-ALPHA.md` §2) | Now |
|---|---|---|
| 0.1 Dockside | runs, open cargo, two accounts | done |
| 0.2 | profit splits between crew | absorbed into 0.2 "Stowage", now ~5× the scope |
| 0.3 | Discord OAuth / org roster; sc-cargo.space links | sc-cargo link pulled forward into 0.2 (URL grammar decoded, ships data in hand); OAuth stays 0.3 |
| 0.4 | UEX import; own price feed | unchanged; `buy_price_scu` survives every schema change on purpose |

## 3. Where 0.2 collides with the existing code

1. **`ships` already exists** (`0002_lookups.sql`: `id, name, uex_id`). `design/0004_ships.sql` is therefore written as `ALTER TABLE … ADD COLUMN` plus `INSERT OR IGNORE` by name; `uex_id` must survive, and the 80 names need reconciling with whatever is already seeded.
2. **`runs` → `voyages` / `loads` / `crates` is a data migration**, not a schema tweak: live rows on spacedock, a `price_observations` intent to preserve, and crates that must be invented for historic runs (greedy 32 / 8 / 1, documented as a guess). Needs a tested migration and a backup step in the deploy notes.
3. **`hx-confirm` on delete** becomes the blackout confirmation; **every htmx swap** needs the raster reveal hooked on `htmx:afterSwap`, or the design falls apart on the first real fragment.
4. **Fonts**: the mock pulls `VT323` / `Share Tech Mono` from Google; the brief says vendor them into `static/` (both OFL; two `.woff2` files).
5. **Sessions**: splash-as-login means `/` must know "session valid" before rendering and the splash is served by both `/login` and `/`; small change in `auth.rs`, but it touches the cookie `Secure` path that bit before.
6. **Test surface**: 24 tests cover `runs`; per-lot pricing, partial close, shares, write-offs and derived status roughly triple the cases.

## 4. Recommended sequencing

0.2 as written is a big bite for a two-person org. Interface first still holds, and it splits cleanly:

| Roll | Contents | Why this order |
|---|---|---|
| 7 "Phosphor" | CRT stylesheet, splash-as-login, raster + blackout machinery, panel hide/show, responsive cards, delete confirmation -- **on the existing schema** | The look, with zero migration risk; proves htmx + raster together |
| 8 "Stowage" | migrations 0004 / 0005, voyages / loads / crates, crate manifest + whereabouts, merge / detach / add-load, stowage plan + sc-cargo link | The hard, data-bearing part, isolated |
| 9 "Settlement" | per-crate close, price modes, shares, SALE banner | Depends on crates existing |
| 10 "Standings" | leaderboard, service record, verdicts | Read-only; can slip without hurting anyone |

Then tag `v0.2.0` once, with the three questions in `RIPAE-0.2.md` §9 answered along the way. First, though: cut the missing `v0.1.3` tag so the badge, `Cargo.toml` and git agree on what is running.
