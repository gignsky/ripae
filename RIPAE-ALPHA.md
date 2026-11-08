# Ripae -- Alpha Landmark `0.1 "Dockside"`

> *Ripae* (Lat. *ripa*, bank/shore) -- a ledger for Star Citizen commodity runs, shared by an org.
> This document is the complete brief for the first landmark. Build exactly this; nothing more.

## 1. Definition of Done

The landmark is reached when **all** of the following are true:

- [ ] A single OCI container image builds reproducibly from the repo (`nix build .#container` **or** `docker build .` -- see §6).
- [ ] That container runs on host `spacedock` via a NixOS `virtualisation.oci-containers` declaration, with SQLite data on a persistent bind-mount.
- [ ] Two accounts exist (Captain + First Officer), seeded from environment variables at first boot.
- [ ] Either user can log in, **log a commodity run**, **edit or delete their own run**, and **see every run** in a shared table.
- [ ] A run may be logged **before it is sold**: it carries a status (`in_transit` / `stored` / `sold`), an intended **destination**, and -- when parked -- the **station where the cargo currently sits**. An "Open Cargo" panel lists every unsold run so nobody forgets a hold full of Laranite at Everus Harbor.
- [ ] A user can **close** an open run by entering where it actually sold, at what price, and how much; this flips it to `sold`.
- [ ] Each sold run shows its computed profit; the table shows a running total of realised profit and the aUEC currently tied up in open cargo.
- [ ] `GET /healthz` returns `200 OK` (for the fleet's Gatus status page later).
- [ ] `cargo test` passes; the profit maths has unit tests.
- [ ] A `README.md` explains: local dev, building the image, deploying to spacedock, rotating the seed passwords.

## 2. Scope Fence

| In (Alpha 0.1) | Deferred (named so nobody wanders) |
|---|---|
| Log / list / edit / delete runs | Profit **splits** between crew (0.2) |
| Open-cargo tracking: status, destination, stored-at | Reminders / Discord pings for stale cargo |
| Two seeded local accounts | Discord OAuth / org roster (0.3) |
| Manual entry of buy & sell prices | UEX API import; Ripae's own price feed (0.4) |
| Per-run profit, running total | Dashboards, charts, routes |
| SQLite, single container | Postgres, multi-host |
| Server-rendered HTML | SPA / mobile |
| -- | Deep links into **sc-cargo.space** (3D cargo-grid viewer) for a run's ship/load (0.3) |

**Design constraints carried forward:**

*Ship as data, not prose.* The `ship` column stays free text in alpha, but 0.3 wants to build an outbound link to sc-cargo.space for the run's ship and container mix. Keep `ship` normalisable (trim, title-case on save) so a later `ships` lookup table (name → sc-cargo.space slug) can join cleanly. Before 0.3, check whether sc-cargo.space exposes a URL scheme or query parameters for pre-selecting a vehicle and containers; if it does not, the link is simply to the site with the ship named in the UI.

*Every run is a price observation.* Every run already records *commodity, terminal, price, timestamp, submitter*. That tuple is exactly a **price observation**. Do not collapse or discard those columns -- they become the seed of Ripae's own fresher-than-UEX price data in 0.4. Build the schema so a future `price_observations` view can be derived from `runs` without migration pain.

## 3. Stack (decided -- do not relitigate)

| Layer | Choice | Reason |
|---|---|---|
| Language | Rust (stable, 2021 edition) | Owner's primary language |
| Web | `axum` + `tokio` | Minimal, well-trodden |
| Templates | `askama` (server-rendered) + `htmx` for inline edit/delete | No JS build step |
| DB | SQLite via `sqlx` (compile-time checked queries, `sqlx migrate`) | One file, trivially backed up |
| Sessions | `tower-sessions` with SQLite store; passwords hashed with `argon2` | Simple, safe |
| Config | Env vars via `figment` or plain `std::env` | 12-factor; container-friendly |
| Styling | `pico.css` (classless, vendored) | Looks decent with zero effort |
| Build | `flake.nix` with `crane` or `naersk`; `pkgs.dockerTools.buildLayeredImage` | Matches the `.dotfiles` / `gigpkgs` world |
| Logging | `tracing` + `tracing-subscriber` (JSON when `RIPAE_LOG_JSON=1`) | Container logs |

## 4. Data Model

```sql
-- migrations/0001_init.sql
CREATE TABLE users (
  id            INTEGER PRIMARY KEY,
  handle        TEXT NOT NULL UNIQUE,     -- e.g. 'lordgig'
  display_name  TEXT NOT NULL,            -- e.g. 'Lord Gig'
  password_hash TEXT NOT NULL,
  created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE runs (
  id             INTEGER PRIMARY KEY,
  submitted_by   INTEGER NOT NULL REFERENCES users(id),

  -- lifecycle
  status         TEXT NOT NULL DEFAULT 'in_transit'
                 CHECK (status IN ('in_transit','stored','sold')),

  -- purchase leg (always known)
  bought_at      TEXT NOT NULL,           -- ISO-8601, user-entered
  commodity      TEXT NOT NULL,           -- free text in alpha; e.g. 'Laranite'
  scu            REAL NOT NULL CHECK (scu > 0),
  buy_terminal   TEXT NOT NULL,           -- e.g. 'ArcCorp Mining Area 141'
  buy_price_scu  REAL NOT NULL CHECK (buy_price_scu >= 0),   -- aUEC per SCU

  -- intent & whereabouts (open cargo)
  destination    TEXT,                    -- where it is MEANT to be sold; may be NULL ("undecided")
  stored_at      TEXT,                    -- required when status = 'stored'; the station/hangar holding it
  ship           TEXT,                    -- optional; e.g. 'Hull A' -- the hull carrying it if in_transit

  -- sale leg (NULL until status = 'sold')
  sold_at        TEXT,
  sell_terminal  TEXT,                    -- where it ACTUALLY sold (may differ from destination)
  sell_price_scu REAL CHECK (sell_price_scu IS NULL OR sell_price_scu >= 0),
  scu_sold       REAL CHECK (scu_sold IS NULL OR scu_sold >= 0),  -- may be < scu (loss, seizure, partial)

  notes          TEXT,
  created_at     TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at     TEXT NOT NULL DEFAULT (datetime('now')),

  CHECK (status <> 'stored' OR stored_at IS NOT NULL),
  CHECK (status <> 'sold'   OR (sold_at IS NOT NULL AND sell_terminal IS NOT NULL
                                AND sell_price_scu IS NOT NULL AND scu_sold IS NOT NULL))
);

CREATE INDEX runs_status    ON runs(status);
CREATE INDEX runs_bought_at ON runs(bought_at DESC);
CREATE INDEX runs_commodity ON runs(commodity);
```

Status semantics: `in_transit` -- aboard a ship, en route or sitting in a hold; `stored` -- offloaded into a station/hangar inventory, `stored_at` names it; `sold` -- done. Open cargo = `status <> 'sold'`. Status changes are plain edits (no state machine beyond the CHECKs), so a stored load can go back to `in_transit` when picked up.

**Money** (computed in Rust, not stored):

```
cost      = scu      * buy_price_scu                 -- always
revenue   = scu_sold * sell_price_scu                -- sold only
profit    = revenue - cost                           -- sold only
margin    = profit / cost  (0 when cost == 0)        -- sold only
committed = Σ cost over open runs                    -- the "aUEC afloat" figure
realised  = Σ profit over sold runs
```

aUEC stored as `REAL`, displayed with thousands separators, no decimals. Unit-test: ordinary sale, partial sale, total loss (`scu_sold = 0`), zero-cost edge, and that an open run contributes to `committed` but never to `realised`.

## 5. Routes

| Method | Path | Auth | Purpose |
|---|---|---|---|
| `GET` | `/healthz` | none | Liveness; returns `ok` |
| `GET` | `/login` | none | Login form |
| `POST` | `/login` | none | Authenticate; set session |
| `POST` | `/logout` | user | Clear session |
| `GET` | `/` | user | **Open Cargo** panel (unsold runs, oldest first, with destination / stored-at / ship) above the **Ledger** (sold runs, newest first) + totals strip (`committed`, `realised`) + "new run" form |
| `POST` | `/runs` | user | Create run -- purchase leg + optional destination; status defaults `in_transit` |
| `GET` | `/runs/:id/edit` | owner | Inline edit row (htmx fragment) -- all fields incl. status / stored_at |
| `PUT` | `/runs/:id` | owner | Update |
| `GET` | `/runs/:id/close` | owner | Inline "close run" fragment: sold_at, sell_terminal (pre-filled from `destination`), sell_price_scu, scu_sold (pre-filled from `scu`) |
| `POST` | `/runs/:id/close` | owner | Record the sale; sets `status = 'sold'` |
| `DELETE` | `/runs/:id` | owner | Delete |

"Owner" = `runs.submitted_by == session.user_id`. Non-owners see rows read-only. Both users see all rows -- the Open Cargo panel is the shared memory of *where everything is and where it is bound*.

Validation errors render back into the form with a message; never a bare 4xx page.

## 6. Container & Deployment

### Environment

| Var | Required | Example |
|---|---|---|
| `RIPAE_BIND` | no (default `0.0.0.0:8080`) | |
| `RIPAE_DB_PATH` | yes | `/data/ripae.db` |
| `RIPAE_SESSION_SECRET` | yes | 64 random bytes, base64 |
| `RIPAE_SEED_USERS` | first boot only | `lordgig:Lord Gig:<pw>,firstofficer:First Officer:<pw>` |
| `RIPAE_LOG_JSON` | no | `1` |

Seeding runs only when the `users` table is empty; afterwards the var is ignored and may be removed.

### Image

Preferred: `flake.nix` output `packages.x86_64-linux.container` via `dockerTools.buildLayeredImage` (static musl build, `scratch`-like base, `/data` as volume). Provide a plain multi-stage `Dockerfile` as fallback for anyone without Nix.

### spacedock module (target shape -- adjust to the host's existing conventions)

```nix
virtualisation.oci-containers.containers.ripae = {
  image = "ripae:0.1.0";           # loaded from the nix-built tarball, or a registry if one exists
  ports = [ "127.0.0.1:8080:8080" ];
  volumes = [ "/var/lib/ripae:/data" ];
  environmentFiles = [ config.age.secrets.ripae-env.path ];  # or sops -- whatever spacedock already uses
  extraOptions = [ "--health-cmd" "wget -qO- http://127.0.0.1:8080/healthz || exit 1" ];
};
```

Expose via the host's existing reverse proxy; TLS and hostname are the operator's choice, not this landmark's.

## 7. Repository Layout

```
ripae/
├── flake.nix · flake.lock
├── Cargo.toml · Cargo.lock
├── Dockerfile
├── README.md
├── RIPAE-ALPHA.md          ← this file
├── migrations/
│   └── 0001_init.sql
├── src/
│   ├── main.rs             # bootstrap, router, tracing
│   ├── config.rs
│   ├── db.rs               # pool, migrate, seed
│   ├── auth.rs             # login/logout, session extractor
│   ├── runs/
│   │   ├── mod.rs          # handlers
│   │   ├── model.rs        # Run, NewRun, profit()
│   │   └── repo.rs         # sqlx queries
│   └── templates.rs        # askama structs
├── templates/
│   ├── base.html · login.html · index.html
│   └── partials/run_row.html · run_form.html · open_cargo_row.html · close_form.html
└── static/
    ├── pico.min.css · htmx.min.js
```

## 8. Working Agreement for the Agent

- Work on a branch `alpha/0.1-dockside`; small commits, each buildable.
- **Commit message format (mandatory):** descriptive title; a line `[ ] Reviewed by Lord G.`; a file-tree diff of what changed; host status (`spacedock: untouched` until deploy); agent signature; `---`; then any additional context. Inspect `git status`, `git diff`, and `git log` before staging. **Never commit secrets** -- `.env*` and `*.db` are gitignored from the first commit.
- Watch files for `#FERREN` / `#CLAUDE` / `#AGENT_NAME` tags -- they are direct notes to you.
- Where this brief is silent, choose the simplest thing and write the choice down in `README.md` under "Decisions".
- Do not add features from the Deferred column, however tempting; leave a `// TODO(0.2)`-style note instead.

## 9. Suggested Order of Work

1. Scaffold: Cargo workspace, flake, `/healthz`, tracing. *(buildable)*
2. DB: migration, pool, seed-on-empty. *(tests: seed idempotent)*
3. Auth: login/logout, session extractor, argon2. *(tests: wrong password rejected)*
4. Runs: model with `Status` enum, `cost()` / `profit()` / totals with tests; repo; create/list.
5. Templates: base, login, index with Open Cargo panel + Ledger table + form; pico.css.
6. Edit / close / delete with htmx fragments; ownership guard.
7. Container image; run locally with `/data` volume.
8. spacedock module + README deploy section; tag `v0.1.0`.
