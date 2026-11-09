# Ripae -- Roll 7 "Stowage": agent brief

> Hand this file to Claude Code at the repo root. It is the *work order*; the *specification* is
> `RIPAE-0.2.md` and the *source of truth for how anything looks or behaves* is
> `design/ripae-crt-mock.html` (open it in a browser and click everything before writing a line).
> Where this file, the brief and the mock disagree: mock > brief > this file. Say so in the commit when you hit one.

## 0. Read first, in this order

1. `RIPAE-ALPHA.md` §3 (stack) and §8 (working agreement) -- still binding.
2. `RIPAE-0.2.md` -- the whole thing; §4 (data model + derived figures) and §9 (rulings) are law.
3. `design/ripae-crt-mock.html` -- run it, log in with any handle/password, close a load, merge voyages, open a crate manifest, visit the leaderboard, click a crew name, delete a voyage.
4. `ANALYSIS-0.1.md` §4 -- the step order below is the same, with more detail.
5. `src/` as it stands -- you are replacing the `runs` module, not the app around it.

## 1. Ground rules

- Branch: `roll/7-<MMDD>-stowage` from `master`, via roll-flow (`rf`). Small commits, each buildable, each with `cargo test` green.
- **Commit message format (mandatory):** descriptive title; a line `[ ] Reviewed by Lord G.`; a file-tree diff of what changed; host status (`spacedock: untouched` until deploy); agent signature; `---`; then context. Inspect `git status`, `git diff`, `git log` before staging. Never commit secrets (`.env*`, `*.db` are ignored).
- Watch files for `#FERREN` / `#CLAUDE` / `#AGENT_NAME` tags -- notes to you.
- **Tests before code.** Every step below names the tests that must exist and fail before the code that makes them pass. No handler is written before its model test exists.
- The 0.1 database is wiped (ruled). Do not write data migrations. Do not keep a `runs` table.
- Where the brief is silent, choose the simplest thing and write it under `README.md` → Decisions.
- Nothing from the Deferred column of `RIPAE-0.2.md` §2. Leave `// TODO(0.3)` notes instead.

## 2. Definition of Done (roll 7 = all of 0.2)

Everything in `RIPAE-0.2.md` §1, plus:

- [ ] `cargo test` green with the unit tests in §5 below and the `proptest` properties in §6.
- [ ] `nix flake check` and `nix build .#container` pass.
- [ ] A headless-browser smoke script (`scripts/smoke.mjs`, Playwright, pattern below) passes against a locally running build.
- [ ] `CHANGELOG.md` has a `0.2.0` entry; `Cargo.toml` is `0.2.0`; `scripts/check-changelog.sh` passes.
- [ ] README: local dev, image, deploy *with the wipe + reseed step*, Decisions updated.
- [ ] `spacedock` is **not** deployed by you. Stop at "ready to promote"; Lord G. runs `rf` promote (which now tags `v0.2.0`) and the dotfiles deploy.

## 3. Step order

Each step ends with a commit. Do not start the skin (7.5) before 7.2–7.4 are green.

### 7.1 -- Hotfix 2 awareness
`hotfix/2-1108-show-validation-errors` (0.1.4) may or may not be promoted when you start. Either way, the `htmx:beforeSwap` 422 opt-in from that branch's `templates/base.html` **must** be present in your base template. Check `git log origin/master --oneline | head` and merge master if it moved.

### 7.2 -- Tests first (`src/stowage/model.rs`, new)
Write the types and the tests in §5–§6 with `todo!()` bodies where needed so the suite compiles and **fails**. Commit: `test(stowage): failing specification for 0.2 money maths and derived status`.

### 7.3 -- Schema, models, repo
- Delete `migrations/000{1,2,3}_*.sql`. Write:
  - `0001_users.sql` -- `users` exactly as 0.1 (seeding code unchanged).
  - `0002_lookups.sql` -- `commodities`, `terminals`, `ships`. `ships` has the 0.2 columns from the start (`manufacturer, name, short, sc_cargo_slug, capacity_scu, concept, uex_id`). Seed ships from `design/ships-sc-cargo.csv` (all 80). Seed commodities and terminals from the old 0002 lists.
  - `0003_stowage.sql` -- `voyages`, `loads`, `crates`, `shares` **verbatim from `RIPAE-0.2.md` §4**, including the CHECKs.
- `src/stowage/{model,repo,mod}.rs` replaces `src/runs/`. Keep `lookups.rs` but change `canonical()` semantics: returns the list spelling if matched, else the trimmed input (names are suggestions, not gates -- ruled).
- Crate-mix parser `parse_boxes("2×32, 4x8") -> Vec<(count, size)>`: accepts `,` `;` whitespace as separators, `x` `X` `×` `*` as the times sign, sizes restricted to `{1,2,4,8,16,24,32}`, rejects zero counts; property test in §6.
- Make 7.2 pass. Commit per sub-step if large.

### 7.4 -- Handlers and plain fragments (old look)
Implement every route in `RIPAE-0.2.md` §5 on the existing Pico base so behaviour can be tested without the skin. Fragments are Askama partials, swapped by htmx exactly as 0.1 did. Specifics the mock settles:

| Behaviour | Rule |
|---|---|
| New run | one voyage (date, ship, status-for-all-crates, stored-at) + 1..n loads; each load: commodity, boxes, SCU (derived from boxes, overridable with a warning), price mode + price, terminal, stop №, destination |
| Add load to voyage | same load fields; inherits ship/date |
| Merge into / detach | `POST /loads/:id/move` with `voyage_id` or `new`; an emptied voyage is deleted; header counts recomputed |
| Crate manifest | per-crate condition cycle `sound → glitched → lost`; whereabouts toggle `aboard ↔ stored` (with `stored_at`); `[ALL ABOARD]` / `[ALL STORED]`; load status and where-now derived per `RIPAE-0.2.md` §4 |
| Close | crate picker (default: sound crates selected; glitched unselected but selectable; lost disabled); `[ALL SOUND CRATES]` / `[NONE]`; sell price per SCU or per lot; shares rows (crew, `% of profit` / `flat`, amount, reason, **forfeit to** optional); kept crates stay open -- implement as the split described in §4 "Partial close" |
| Delete voyage | `GET /voyages/:id/delete` returns the confirmation fragment (loads, SCU, committed aUEC); `DELETE` only from that fragment's button; **no `hx-confirm`** |
| Stowage plan | derived on every render of `/`: one card per hulled voyage; unassigned voyages clustered by shared destination (transitive); clusters ≥2 render a *Suggested* card with a merge command; link per `RIPAE-0.2.md` §6 |
| Leaderboard | `/board` with sort param; `/crew/:id` service-record fragment |
| Sale | `POST /loads/:id/close` responds with the SALE fragment (profit, route, shares, kept) |

Validation errors re-render the fragment with the message and the **submitted** values (not the stored ones -- the 0.1.4 follow-up), status 422.

### 7.5 -- The skin
- `static/ripae.css` replaces `pico.min.css` (delete it). Tokens, type, components **from `design/ripae-crt-mock.html`'s `<style>`** -- lift it, don't reinterpret it.
- Vendor `VT323` and `Share Tech Mono` as `.woff2` into `static/fonts/` with `@font-face`; no Google Fonts call.
- `static/ripae.js`: lift `raster()`, the blackout flicker, panel `[HIDE]/[SHOW]`, the splash sequence, the crate-mix live parser, price-mode derivation, and the `data-l` card labelling from the mock. Wire the raster to **`htmx:afterSwap`** (and `htmx:load` for the first paint) so every fragment paints line by line; each fragment's top-level children carry `class="ras"`.
- Splash = login: `/login` renders the splash with the gate; `/` with a valid session renders the splash with *Welcome back · press any key · [LOG OUT]* once per session (a session flag, so it doesn't replay on every page load), then the ledger. Footer `[CLOSE]` replays the splash in the current mode. `[LOG OUT]` posts `/logout` then shows the gate.
- Rail: `CREW MEMBER: <DISPLAY NAME>`, `[LEDGER] [LEADERBOARD] [LOG OUT]`; subtitle `LEDGER TERMINAL · NISC CARGO`.
- Screen switch ledger ↔ leaderboard: raster out, navigate (full page is fine), raster in.
- `prefers-reduced-motion`: everything instant.
- No horizontal scroll at 1280 / 760 / 400 px; table rows become cards under 980 px.

### 7.6 -- Overlays
SALE banner, DELETE confirmation, HOORAY / WOMP WOMP + service record (one raster pass, together). ASCII art is in the mock; copy it byte for byte.

### 7.7 -- Release prep
`CHANGELOG.md` 0.2.0 (user-facing, in the voice of the existing entries); `Cargo.toml` 0.2.0; README; `scripts/smoke.mjs`; final `nix flake check`. Stop and report.

## 4. Deploy note for the README (you write it, Lord G. runs it)

```
# on spacedock, before switching to the 0.2 image:
systemctl stop podman-ripae   # or docker-ripae, per the host unit name
mv /var/lib/ripae/ripae.db /var/lib/ripae/ripae-0.1-final.db   # kept for a week, then deleted
# switch the image; first boot migrates 0001-0003 and reseeds users from RIPAE_SEED_USERS
```

## 5. Unit tests that must exist (names are suggestions; coverage is not)

`model.rs`
- `load_status_all_aboard_is_in_transit`, `…_all_stored_is_stored`, `…_all_pickup_is_awaiting`, `…_mixed_is_split`, `…_all_sold_or_lost_is_sold`
- `where_now_lists_only_nonzero_groups` (e.g. `"1 aboard JT's Hull C · 2 stored at Everus Harbor"`)
- `scu_excludes_lost_crates`; `cost_includes_lost_crates`; `written_off_is_lost_scu_times_buy_price`
- `per_lot_buy_derives_per_scu` and the reverse; `per_lot_sell_derives_per_scu`
- `partial_close_splits_load_and_keeps_rest_open`
- `profit_uses_cost_of_crates_sold_only`
- `pct_share_on_loss_is_zero`; `flat_share_on_loss_is_zero` (ruled); `flat_share_on_profit_is_full_amount`; `shares_sum_plus_kept_equals_profit`
- `forfeit_moves_cash_not_credit` (`to_user` gets `board_credit`, `paid_to` gets `cash`)
- `board_credit_and_cash_sum_to_same_org_total`
- `committed_subtracts_written_off`; `realised_counts_sold_crates_only`
- `parse_boxes_accepts_commas_spaces_and_three_times_signs`; `parse_boxes_rejects_bad_sizes_and_zero_counts`
- `stowage_order_last_stop_first_then_larger_crates`; `stowage_labels_deepest_doorside_midhold`
- `unassigned_voyages_cluster_by_shared_destination_transitively`; `undecided_destination_never_pairs`
- `sc_cargo_link_grammar` (`Hull C`, 2×8 + 4×32 → `…/hull-c-official-r2-u4`; no crates → no suffix; spaces URL-encoded)
- `glitched_crates_excluded_from_select_all_but_selectable`; `lost_crates_never_sellable`

`repo.rs` (sqlite in-memory, as 0.1 did)
- round-trips for voyage / load / crate / share; move load between voyages; emptied voyage removed; cascade on voyage delete; derived status survives a reload

`auth.rs`
- unchanged tests + `splash_mode_is_gate_without_session_and_welcome_with_one`

## 6. Property tests (`proptest`)

Generators: crate mixes (1–40 crates over the seven sizes, each with random condition and whereabouts), buy and sell prices (0–50 000 per SCU, or lot totals), 0–4 shares with random kind/amount/forfeit, random sold-subsets of sellable crates.

Invariants:
1. `Σ share.paid + seller_keeps == profit` (within 1e-6).
2. `profit <= 0 ⇒ every share.paid == 0`.
3. `Σ_u board_credit(u) == Σ_u cash(u)` over the org.
4. `committed − written_off >= 0`.
5. `parse_boxes(format_boxes(mix)) == mix` for every mix.
6. `stowage_order` is a permutation of the loads and non-increasing in `stop_no`.
7. `scu_sold <= scu` and `scu_sold == Σ sizes of selected crates`.
8. Clustering is a partition of unassigned voyages; two voyages sharing a destination are always in the same cluster.

Fuzz-adjacent: run the `parse_boxes` property with `cases = 10_000` once locally; keep the default in CI.

## 7. Smoke script shape (`scripts/smoke.mjs`)

Playwright, headless, against `RIPAE_BIND=127.0.0.1:18080` started **from the repo root** (static files are served relative to cwd). Steps: gate login → ledger visible → new run with two loads (one per-lot) → crate manifest: mark one lost, one stored → status reads SPLIT → close the first load selecting two crates with one 25 % share forfeited to the other crew member → SALE banner text contains the profit → leaderboard → click a crew name → service record visible → delete the second voyage via confirmation → gone. Assert `document.documentElement.scrollWidth == clientWidth` at 1280, 760, 400.

## 8. Things that will bite

- `ServeDir::new("static")` is cwd-relative; the smoke script and README must start the binary from the repo root (or make it absolute from the executable's directory -- a fine Decision to record).
- htmx 1.9 discards 4xx unless `htmx:beforeSwap` says otherwise (see hotfix 2). Keep that listener.
- `tower-sessions` cookie `Secure` flag: `RIPAE_COOKIE_SECURE=0` for local HTTP; the splash "welcome back" check must not loop if the cookie is rejected.
- Askama: overlays and fragments share partials; avoid duplicating the ASCII banners -- one partial each.
- The raster needs `.ras` on fragment children; forgetting it on a new partial makes that fragment pop in instantly. The smoke script can't see this; eyeball it.
- `INSERT OR IGNORE` on `ships.name` is `COLLATE NOCASE`; the CSV has `Hull-C` (hyphen). Use the CSV spelling everywhere so the slug and the display agree.
