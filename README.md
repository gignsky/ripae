# Ripae

A ledger for Star Citizen commodity runs, shared by an org. See
[RIPAE-ALPHA.md](./RIPAE-ALPHA.md) for the full Alpha 0.1 "Dockside" brief
this build follows.

## Local dev

```sh
export RIPAE_BIND=0.0.0.0:8080            # optional, this is the default
export RIPAE_DB_PATH=./ripae.db
export RIPAE_SESSION_SECRET=$(openssl rand -base64 64)
export RIPAE_SEED_USERS="lordgig:Lord Gig:changeme,firstofficer:First Officer:changeme"
export RIPAE_COOKIE_SECURE=0              # only when serving plain HTTP off localhost
cargo run
```

Migrations run automatically on boot (`sqlx::migrate!`, embedded in the
binary at compile time). Seeding from `RIPAE_SEED_USERS` only happens while
`users` is empty -- safe to leave the var set across restarts, and safe to
drop it once the two accounts exist.

```sh
cargo test     # unit tests -- profit math, seeding, auth, repo round-trips
cargo build --release
```

## Building the image

Nix only (the Dockerfile fallback the brief allowed was never exercised and was removed on 8 Nov 2026):

- **Nix:** `nix build .#container` via `flake.nix` (crane +
  `dockerTools.buildLayeredImage`). `nix flake check` (builds the binary and
  runs the 17 tests) and `nix build .#container` both pass, and the image
  contains `static/`. The image has not yet been loaded and run under
  docker/podman.

## Deploying to spacedock

Handled from the `.dotfiles` side: `containers/services/ripae.nix` declares
the `virtualisation.oci-containers.containers.ripae` unit (image from this
repo's `ripae` flake input, SQLite on a `/var/lib/ripae` bind-mount, bound
to `127.0.0.1:8080`, reverse-proxied by the host). See that file's header
comment for the one-time setup step (adding the `ripae-env` sops secret).

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
available on the host). A dedicated `ripae passwd <handle>` subcommand is a
reasonable 0.2 addition; not built here, kept to scope.

## Status (Definition of Done, §1)

Done:
- [x] Two accounts seeded from env vars at first boot, idempotently.
- [x] Login, log a run, edit/delete own run, see every run in a shared table.
- [x] A run can be logged before it's sold (status, destination, stored-at);
      Open Cargo panel lists every unsold run.
- [x] Closing a run records where/at what price/how much it actually sold,
      flips it to `sold`.
- [x] Per-run profit shown on the Ledger; running totals (`committed`,
      `realised`) shown on `/`.
- [x] `GET /healthz` returns `200 OK` / `ok`.
- [x] `cargo test` passes (17 tests); profit math has unit tests covering
      ordinary sale, partial sale, total loss, the zero-cost edge case, and
      that open runs land in `committed` not `realised`.
- [x] This README.

Open / unverified:
- [ ] Run the Nix-built image (`docker load < result`) -- it builds, but
      hasn't been started yet.
- [ ] Actual spacedock deployment -- the dotfiles-side module is written
      and pushed on its own roll branch, but nothing has been switched on
      the real host; that's an operator action, not bundled here.
- [ ] No password-rotation CLI (see above) -- a manual SQL UPDATE works for
      alpha.

Per §9's suggested order of work, steps 1-6 (scaffold through htmx
edit/close/delete) are done and tested; step 7 (container image) builds via
Nix but hasn't been run; step 8 (spacedock module + README deploy section +
`v0.1.0` tag) is split across repos -- the dotfiles half is done, the tag
is left for whoever graduates that roll.

## Decisions

Where the brief was silent, in order of appearance:

- **Trunk branch is `master`, not `main`.** Matches this author's other
  repos (gigpkgs' stable branch is already `master`). `master` is pushed
  alongside the branch this was actually developed on.
- **roll-flow installed, not yet used for this landmark.** `.roll-flow.toml`
  is in place (`stable_branch = "master"`, `rolling_branch = "rolling"`,
  gates = `cargo test` + `cargo build --release`), matching gigpkgs' shape.
  This initial alpha build happened directly on the working branch per the
  brief's own §8 working agreement (written before roll-flow was adopted
  here); `roll/` branches apply to feature work from here on, landing on
  `rolling` → `master`.
- **sqlx: runtime queries, not compile-time `query!`/`query_as!` macros.**
  The compile-time macros need a live `DATABASE_URL` (or a committed
  `.sqlx` offline cache) at build time; plain `sqlx::query`/`query_as` with
  a hand-written `Run` row mapper (`src/runs/repo.rs`) keeps the build
  reproducible in any sandbox with no DB present, at the cost of losing
  compile-time SQL checking. Worth revisiting once there's a stable CI
  environment to generate/commit the offline cache in.
- **One SQLite connection (`max_connections(1)`).** SQLite only allows one
  writer at a time anyway, and a pool of `>1` connections to `:memory:`
  would each see a separate empty database in tests.
- **Session cookie signing key from `RIPAE_SESSION_SECRET`.** The var is
  base64, decoded to >=64 bytes and fed to `tower_sessions::cookie::Key` via
  `SessionManagerLayer::with_signed`, so the session id cookie is tamper-evident.
- **`pico.css` vendored as the *classless* build** (`pico.classless.min.css`),
  per §3's "classless" wording. A few inert utility classes from the full
  build (`.grid`, `.secondary`, `.outline`) are used in the templates for
  layout hints; they're harmless no-ops under classless pico today. Worth
  switching to the full build (or dropping those classes) if the layout
  needs real polish before 0.2.
- **Edit form only offers `in_transit`/`stored` in its status `<select>`.**
  Flipping a run to `sold` is the dedicated `/runs/:id/close` flow, which
  is the only place that can populate the sale-leg columns the `sold` DB
  CHECK constraint requires.
- **Validation errors return `422` with the form/page re-rendered**, never
  a bare framework error page, per §5.
- **No password-rotation CLI** -- see "Rotating the seed passwords" above.
