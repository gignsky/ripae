# Ripae

A ledger for Star Citizen commodity runs, shared by an org. 0.2 "Stowage"
wears the CRT terminal look and tracks cargo down to the crate. The briefs:
[RIPAE-ALPHA.md](./RIPAE-ALPHA.md) (0.1, stack and working agreement),
[RIPAE-0.2.md](./RIPAE-0.2.md) (0.2 spec; §4 and §9 are law) and
[RIPAE-ROLL7.md](./RIPAE-ROLL7.md) (the roll 7 work order). The interactive
mock in `design/ripae-crt-mock.html` is the source of truth for how anything
looks or behaves.

## Local dev

```sh
export RIPAE_BIND=0.0.0.0:8080            # optional, this is the default
export RIPAE_DB_PATH=./ripae.db
export RIPAE_SESSION_SECRET=$(openssl rand -base64 64)
export RIPAE_SEED_USERS="lordgig:Lord Gig:changeme,firstofficer:First Officer:changeme"
export RIPAE_COOKIE_SECURE=0              # only when serving plain HTTP off localhost
cargo run                                 # from the repo root (see Decisions: static/ is cwd-relative)
```

Migrations run automatically on boot (`sqlx::migrate!`, embedded in the
binary at compile time). Seeding from `RIPAE_SEED_USERS` only happens while
`users` is empty -- safe to leave the var set across restarts, and safe to
drop it once the two accounts exist.

```sh
cargo test                 # unit tests: money maths, derived status, repo round-trips, auth
cargo build && node scripts/smoke.mjs     # headless-browser smoke test (Playwright, Chromium)
cargo build --release
```

`scripts/smoke.mjs` spawns `target/debug/ripae` on `127.0.0.1:18080` with a
throwaway database, walks the whole 0.2 story (gate → new run → crate
manifest → a crewmate on the voyage's crew on 25 %, forfeited back → partial
close split by the crew → SALE banner → leaderboard → a member's ledger,
totals equal to their board row → service record → delete through the
confirmation) and asserts there is no
horizontal scroll at 1280 / 760 / 400 px. Set `RIPAE_URL` to run it against
an already-running build instead. It needs `playwright` resolvable from
Node (`npm i -g playwright && npx playwright install chromium`, or a
preinstalled copy on `NODE_PATH`). The raster reveal is not something a
script can judge; eyeball it.

## Running a test copy locally

```sh
scripts/dev.sh                 # clean database, kept in .dev/clean.db (--reset empties it)
scripts/dev.sh live            # copy of the live database from spacedock (--refresh pulls anew)
```

Works from any host with `cargo` (`nix develop` / direnv) and, for `live`,
`ssh spacedock`. It serves on `127.0.0.1:8080` over plain HTTP (override with
`RIPAE_BIND`), keeps its databases under `.dev/` (gitignored), and strips any
Discord settings from the environment, so it can never sign in against, or
write to, the real site. Sign in with handle + password, `changeme` unless
`RIPAE_DEV_PASSWORD` says otherwise: `lordgig` / `firstofficer` on a clean
database, any live handle on a copy. A copy is pulled once and then reused;
local edits to it survive restarts until you `--refresh`.

On a copy, `RIPAE_DEV_RESET_LOGINS=<password>` (which `dev.sh live` sets)
gives every account that password, drops its Discord link and clears stored
sessions at boot, so you needn't set Discord up. The app refuses to start
with it set alongside Discord sign-in.

**One-time spacedock setup.** `/var/lib/ripae` is root-only, so the pull goes
through a single sudo command that takes a consistent `sqlite3 .backup` into
`~/.cache/ripae-snapshot.db` (`scripts/spacedock-snapshot.sh`). In the
dotfiles, on spacedock:

```nix
let
  ripae-snapshot = pkgs.writeShellApplication {
    name = "ripae-snapshot";
    runtimeInputs = [ pkgs.sqlite pkgs.coreutils pkgs.getent ];
    # keep a copy of scripts/spacedock-snapshot.sh next to the config
    text = builtins.readFile ./spacedock-snapshot.sh;
  };
in {
  environment.systemPackages = [ ripae-snapshot ];
  security.sudo.extraRules = [{
    users = [ "gig" ];
    commands = [{ command = "/run/current-system/sw/bin/ripae-snapshot"; options = [ "NOPASSWD" ]; }];
  }];
}
```

The snapshot is the live ledger, password hashes and Discord ids included;
it sits mode 600 in `~/.cache` on spacedock and in `.dev/` locally.

## Discord sign-in

Set all four of these and the gate becomes `[SIGN IN WITH DISCORD]`:
anyone in the NISC Discord server gets in, and their account is made on
first sign-in. Set none and the gate stays handle + password (what local
dev and `smoke.mjs` use). Setting only some of them is a boot error.

```sh
export RIPAE_DISCORD_CLIENT_ID=...        # Discord application → OAuth2 → Client ID
export RIPAE_DISCORD_CLIENT_SECRET=...    # same page → Client Secret (keep it in the sops secret)
export RIPAE_DISCORD_GUILD_ID=...         # the NISC server's id (Developer Mode → right-click the server → Copy Server ID)
export RIPAE_PUBLIC_URL=https://ripae.example   # what the browser sees; no trailing path
```

On the application (<https://discord.com/developers/applications>), under
**OAuth2 → Redirects**, add `$RIPAE_PUBLIC_URL/auth/discord/callback`
exactly. No bot is needed: the `identify` and `guilds.members.read` scopes
are enough to see who someone is and whether they're in the server.

Every sign-in refreshes the member's profile from Discord: their Discord
**username** becomes the handle, their **server nickname** (else their
Discord display name) the name on the ledger and leaderboard, and their
avatar goes on the service record. If the username is already some other
account's handle, the account keeps the handle it has.

**Claiming the old accounts.** Accounts from before Discord (the seeded
`lordgig` and `firstofficer`, with their voyages) are claimed once: on the
gate, open *Crew from before Discord? Claim your account*, enter the old
handle and password, and Discord links that account to you. From then on
only Discord signs into it; its password no longer works. An account that
is never claimed keeps its password but can only use it to claim.

## Building the image

Nix only (the Dockerfile fallback the brief allowed was never exercised and
was removed on 8 Nov 2026):

```sh
nix flake check          # builds the binary and runs the tests
nix build .#container    # dockerTools.buildLayeredImage → ./result (ripae:0.2.0)
```

The image carries `static/` (stylesheet, script, vendored fonts) next to
the binary with `WorkingDir = "/"`, so the cwd-relative `static/` lookup
finds it.

## Deploying to spacedock

Handled from the `.dotfiles` side: `containers/services/ripae.nix` declares
the `virtualisation.oci-containers.containers.ripae` unit (image from this
repo's `ripae` flake input, SQLite on a `/var/lib/ripae` bind-mount, bound
to `127.0.0.1:8080`, reverse-proxied by the host). See that file's header
comment for the one-time setup step (adding the `ripae-env` sops secret).
Bump the image tag there to `0.2.0`.

**The image is prebuilt on spacedock at promotion.** `rf promote` runs
`scripts/cache-on-spacedock.sh` as its last gate. It builds `.#container`
*on spacedock* against the nixpkgs its dotfiles have locked (they make
ripae follow their own nixpkgs, so the image from ripae's own lock would be
a different derivation), and it leaves a GC root at
`~/.cache/ripae-container`. The deploy's rebuild after
`nix flake update ripae` then finds the image already built. This works
from any machine that can `ssh spacedock` as a nix trusted user. If
spacedock can't be reached, the gate fails; promote with
`--force --reason ...` and the deploy builds the image itself.

### 0.1 → 0.2: wipe and reseed

0.2 starts the schema over (ruled 8 Nov 2026: no data migration). The
migrations are new files, so the 0.2 image **cannot** boot on a 0.1
database; the old file is moved aside and the first boot creates a fresh
one and reseeds the accounts from `RIPAE_SEED_USERS` (make sure the var is
still in the `ripae-env` secret).

```sh
# on spacedock, before switching to the 0.2 image:
systemctl stop podman-ripae   # or docker-ripae, per the host unit name
mv /var/lib/ripae/ripae.db /var/lib/ripae/ripae-0.1-final.db   # kept for a week, then deleted
# switch the image; first boot migrates 0001-0003 and reseeds users from RIPAE_SEED_USERS
```

**Keeping the accounts' "aboard since" dates.** The service record shows
`aboard since <users.created_at>`. A reseed stamps today's date on both
accounts; to keep the 0.1 dates (and the 0.1 password hashes, so nobody has
to learn a new password), copy the two `users` rows across *before* the
first 0.2 boot. The table is identical in both schemas, so a dump of just
that table replays cleanly, and the seed then sees a non-empty `users` and
does nothing:

```sh
# after the mv above, still stopped:
sqlite3 /var/lib/ripae/ripae-0.1-final.db ".dump users" > /var/lib/ripae/users-0.1.sql
# start the 0.2 container once so it creates the schema, then stop it again
systemctl start podman-ripae && sleep 5 && systemctl stop podman-ripae
sqlite3 /var/lib/ripae/ripae.db "DELETE FROM users;"          # drop the freshly seeded rows
sqlite3 /var/lib/ripae/ripae.db < /var/lib/ripae/users-0.1.sql # same ids, hashes and created_at as 0.1
systemctl start podman-ripae
```

Or, if the reseed has already happened, just carry the dates over by
handle:

```sh
sqlite3 /var/lib/ripae/ripae.db "ATTACH '/var/lib/ripae/ripae-0.1-final.db' AS old;
  UPDATE users SET created_at = (SELECT o.created_at FROM old.users o WHERE o.handle = users.handle)
  WHERE handle IN (SELECT handle FROM old.users);"
```

Promotion and the `v0.2.0` tag are `rf promote`'s job, not this branch's.

## Rotating the seed passwords

`RIPAE_SEED_USERS` only seeds accounts into an **empty** `users` table, so
it can't be used to reset an existing password. To rotate:

```sql
-- on the running container's /data/ripae.db, or via a sidecar with sqlx-cli
UPDATE users SET password_hash = '<new argon2 hash>' WHERE handle = 'lordgig';
```

Generate a new hash with `ripae`'s own hasher (there's no standalone CLI
yet -- the simplest route today is a one-off `cargo run` against a scratch
DB that calls `db::hash_password`, or `argon2` on the command line if
available on the host). A dedicated `ripae passwd <handle>` subcommand is
still a reasonable addition; not built here, kept to scope.

## Status (0.2 Definition of Done, RIPAE-0.2.md §1)

- [x] CRT terminal look on every screen: splash, ledger, leaderboard, overlays.
- [x] Voyage → loads → crates; merge into / detach.
- [x] Buy per SCU or per lot; both figures stored.
- [x] Whereabouts per crate; derived load status incl. `SPLIT`; New Run stamps one status on every crate.
- [x] Stop № and destination per load; Stowage Plan with the sc-cargo.space link.
- [x] Glitched and lost crates; lost SCU written off and shown.
- [x] Close per crate, price per SCU or per lot, shares by % or flat, forfeits, kept crates stay open.
- [x] SALE banner with blackout; DELETE confirmation; HOORAY / WOMP WOMP + service record.
- [x] Leaderboard sortable by profit / runs / SCU, org totals, recent shares.
- [x] Splash is the login; welcome once per session; `[CLOSE]` replays; `[LOG OUT]` on rail and splash.
- [x] No horizontal scrollbar at 1280 / 760 / 400 px; cards under ~980 px.
- [x] `cargo test` (65 tests incl. the proptest invariants of RIPAE-ROLL7.md §6); `scripts/smoke.mjs` passes.
- [x] `CHANGELOG.md` 0.2.0; `Cargo.toml` 0.2.0; badge reads `v0.2.0`.
- [ ] Deployed to spacedock -- operator action (`rf promote`, dotfiles deploy with the wipe + reseed above).

## Decisions

Where the briefs were silent, in order of appearance. 0.1 decisions that
still hold come first.

- **Trunk branch is `master`, not `main`.** Matches this author's other
  repos (gigpkgs' stable branch is already `master`).
- **roll-flow** (`.roll-flow.toml`: `stable_branch = "master"`,
  `rolling_branch = "rolling"`; gates `cargo test` + `cargo build --release`,
  plus `scripts/check-changelog.sh` on the way to `master`). `roll/` branches
  land on `rolling` → `master`; promotion tags.
- **The changelog is written on `rolling`, not in a roll.** Rolls leave
  `CHANGELOG.md` alone; the entry for a release is written on `rolling`
  just before promotion, covering every roll graduated since the last one.
  Only release versions must have an entry (`check-changelog.sh` on the
  way to `master`, and the changelog test for a build without a `-rollN`
  suffix).
- **sqlx: runtime queries, not compile-time `query!`/`query_as!` macros.**
  The compile-time macros need a live `DATABASE_URL` (or a committed
  `.sqlx` offline cache) at build time; plain `sqlx::query` with
  hand-written row mappers (`src/stowage/repo.rs`) keeps the build
  reproducible in any sandbox with no DB present, at the cost of losing
  compile-time SQL checking.
- **One SQLite connection (`max_connections(1)`).** SQLite only allows one
  writer at a time anyway, and a pool of `>1` connections to `:memory:`
  would each see a separate empty database in tests. Multi-row writes run
  in a transaction on that connection.
- **Session cookie signing key from `RIPAE_SESSION_SECRET`.** The var is
  base64, decoded to >=64 bytes and fed to `tower_sessions::cookie::Key`
  via `SessionManagerLayer::with_signed`, so the session id cookie is
  tamper-evident.
- **Validation errors return `422` with the form re-rendered**, carrying
  the *submitted* values and the message, never a bare framework error
  page. htmx 1.9 does not swap 4xx responses unless told to, so `base.html`
  opts 422 back in via `htmx:beforeSwap` (0.1.4); keep that listener.
- **No password-rotation CLI** -- see "Rotating the seed passwords" above.

0.2 "Stowage":

- **`static/` is served relative to the working directory**
  (`ServeDir::new("static")`). Start the binary from the repo root; the
  container sets `WorkingDir = "/"` and ships `static/` there. Chosen over
  resolving from the executable's path so the Nix image layout stays as it
  was.
- **The spec's `loads_status` index was dropped.** RIPAE-0.2.md §4 listed
  `CREATE INDEX loads_status ON loads(status)` but `loads` has no status
  column (it is derived, as the same section says). Per the brief the
  document is the error; the line is gone from the spec and noted in
  `migrations/0003_stowage.sql`.
- **`loads.submitted_by` on a sold row is the member who closed it** (the
  seller). The leaderboard's "where u sold" reads that column; whoever
  logged the purchase is still on the kept part of a partial close, and on
  the voyage's `logged_by`.
- **A partial close splits the load.** The sold crates move to a new load
  row on the same voyage with the sale leg filled in; the original keeps the
  rest, lost crates included, and its lot price is recomputed for what's
  left. Selling every sellable crate closes the row in place; lost crates
  ride along on it as the ledger's write-off record.
- **Crates define the SCU.** The New Run / Add Load forms derive SCU from
  the boxes and let you type over it, but a figure that disagrees with the
  boxes is refused (422) rather than silently kept -- there is nowhere to
  store an SCU that isn't crates.
- **Crate mixes are normalised**: `1×32, 2×32` is stored as three 32 SCU
  crates and reads back as `3×32`; sizes are listed largest first.
- **A ship name must be one of the 80 known hulls, or blank.** A hull needs
  an sc-cargo.space slug and the list is the site's; "no hull yet" is a
  first-class state (the Stowage Plan says so and offers no link).
  Commodity and terminal names, by contrast, are suggestions: accepted as
  typed, snapped to the list's spelling when they match.
- **`[ALL ABOARD]` / `[ALL STORED]` and the edit form's status move live
  crates only**; lost crates keep their last known whereabouts, sold ones
  are untouchable.
- **Detaching a load makes a new voyage with the same ship and date,
  logged by whoever detached it.** A voyage left without loads is deleted
  (on a move and on a load delete).
- **Voyage-level merge** (`POST /voyages/:id/move`) moves every open load;
  it backs the header's `[MERGE INTO…]` and the Suggested card's command.
  Routes beyond RIPAE-0.2.md §5 are all of this shape: `GET
  /voyages/:id/head`, `GET/PUT /voyages/:id/edit` (assign a hull), `GET
  /voyages/:id/merge`, `GET /voyages/:id/loads/new`, `GET /loads/:id` (plain
  row), `GET /loads/:id/delete` (same confirmation as a voyage).
- **Shares:** a member can't share with themself (the remainder is already
  theirs); percentage shares may not total more than 100; a flat fee may
  exceed the profit (the seller agreed to it). On a loss every share pays 0
  (ruled).
- **The voyage's crew is the one place a split is set** (0.3.1-roll7,
  migration 0012, replacing `voyage_shares` and the per-sale share rows).
  `voyage_crew` has one row per member: a cut of each sale's profit
  (`pct`), a flat fee per sale (`flat`), both, or neither (crew without a
  share), and an optional forfeit-to (`paid_to`: they get the aUEC, the
  earner keeps the credit). Every sale on the voyage settles the crew's
  terms for everyone **but its seller** -- their own cut doesn't apply to
  the sale they make, so the run's owner may hold a cut of a crewmate's
  sale. Cuts across the crew may not total more than 100 %. What a sale
  paid is still frozen in `shares`; editing the crew changes later sales
  only. The sale editor re-pays a sale's own shares on a corrected price,
  or re-splits it by the crew as it stands when asked (a seller who holds
  a share of that sale is refused, as before). No per-sale override: the
  close form shows the split read-only with `[EDIT CREW]`.
- **A member's ledger** (`GET /crew/:id/ledger`; the leaderboard's names,
  the service record's name and `[LEDGER]`, and the Ledger's *By* column
  link to it). Every voyage the member was on -- crew, owner, a load's
  buyer/holder/seller, or named on a share -- newest first (by bought-on
  date, then id), open ones included. Per voyage: role (logged / bought /
  sold / crew), their terms, loads they sold, *kept as seller* (their
  profit credit), *shares earned* (credit, forfeited or not), *collected*
  (aUEC that reached them: kept + shares paid to them), *losses* (lost
  cargo they held), and *credit* = `board_credit` over that voyage alone,
  with running credit and cash. The maths is `model::member_ledger` /
  `ledger_voyage`, which call `board_credit`, `cash` and `losses` on a
  one-voyage slice -- so the rows always sum to the leaderboard, whatever
  those functions come to count. `[LOADS n]` opens a voyage's loads.
- **Who's on the crew.** The run's owner, every load's buyer and holder (a
  sold row's seller), and anyone a share or a crew forfeit pays are always
  aboard (`repo::sync_crew` after every write; the voyage form shows them
  without `[REMOVE]`). Others are added on the voyage form. A detached
  load's new voyage copies the crew and terms; a load merged into another
  voyage brings its holder along, on no terms (the target's split applies,
  as before).
- **Migration 0012 mapping.** Voyage-level shares become the member's crew
  terms (rows for one member summed by kind -- 25 % and a 1,000 flat stay
  both; reasons joined with ` · `; the first forfeit kept). A member who only
  held shares on individual sales gets the terms of their **latest** sale
  on that voyage. Owners, buyers, sellers and forfeit recipients join
  without terms. `shares` is untouched, so every settled amount -- and every
  member's credit and cash -- is identical before and after (tested on a
  fixture DB). One corner can't be expressed in the new shape: a member
  whose % and flat parts were forfeited to different people keeps the
  first forfeit for both on future sales. "Runs closed" on the board counts voyages with every load sold in
  which the member sold at least one load.
- **Lost cargo is a loss.** What a lost or written-off crate cost
  (`written_off`) comes off realised (Open Cargo's totals and the board's
  *Org realised*) and off the lifetime profit and cash of whoever holds
  the load (`submitted_by`), whether the load is open, sold or written off
  whole. A sale's own profit, and the shares settled on it, still count
  only the crates that sold.
- **`users.created_at` is the service record's "aboard since."** Keep it
  across the 0.2 wipe with the steps under Deploying.
- **Bookkeeping for a crewmate.** Whose run it is (`voyages.logged_by`,
  `loads.submitted_by`) and who sold it (the sold row's `submitted_by`)
  are form fields -- *Crew member* on New Run, *Bought by* on Add Load,
  *Sold by* on the close form -- defaulting to the member logging it (and,
  for a sale, to the load's buyer). The member recording it is not stored;
  the SALE banner names them only when they differ from the seller.
- **Fragments mix `<tbody>` rows with out-of-band `<section>`s**, which the
  HTML parser only keeps under template parsing:
  `htmx.config.useTemplateFragments = true` (the `htmx-config` meta tag in
  `base.html`). The Open Cargo table has one `<tbody>` per voyage header
  and one per load, so a crate change re-renders its load, a header change
  its header, and anything that moves loads re-renders `#open-cargo`;
  totals and the Stowage Plan ride along out of band on every change.
- **Raster hooks on `htmx:load`, `htmx:afterSwap` and `htmx:oobAfterSwap`**
  with a queued marker so a line is painted once whichever fires first; the
  `.ras` / `.rs` hiding rules are scoped to `html.js` so a page without the
  script still shows everything.
- **Fonts are vendored** (`static/fonts/`, OFL 1.1) from Google's woff2
  builds; no Google Fonts request leaves the container.
- **`nix flake check` in a sandbox that blocks GitHub tarballs**: override
  the inputs to their `git+https://…?rev=…&shallow=1` forms; nothing in the
  flake itself changes.
