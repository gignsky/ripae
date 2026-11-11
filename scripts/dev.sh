#!/bin/sh
# Run a local test copy of Ripae, on any host with cargo (nix develop / direnv
# gives you one). Never touches the live site.
#
#   scripts/dev.sh                  clean database (kept in .dev/clean.db between runs)
#   scripts/dev.sh clean --reset    throw the clean database away and start empty
#   scripts/dev.sh live             copy of spacedock's live database (pulled once, then reused)
#   scripts/dev.sh live --refresh   pull a fresh copy first (discards local changes to it)
#
# Sign in with handle + password. clean: lordgig / firstofficer; live: any
# existing handle, every account's password is rewritten to the dev password
# (the copy's Discord links are dropped, so no Discord set-up is needed).
# Both are "changeme" unless RIPAE_DEV_PASSWORD says otherwise. Either way
# the gate also lists every account as a one-click sign-in
# (RIPAE_DEV_QUICK_LOGIN), so the password is rarely needed.
#
# Other knobs: RIPAE_BIND (default 127.0.0.1:8080), RIPAE_DEV_HOST (ssh host
# holding the live data, default spacedock).
set -eu

mode=clean
reset=0
while [ $# -gt 0 ]; do
  case $1 in
    clean|live) mode=$1 ;;
    --reset|--refresh) reset=1 ;;
    -h|--help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "dev.sh: unknown argument: $1 (try --help)" >&2; exit 2 ;;
  esac
  shift
done

cd "$(dirname "$0")/.."      # static/ is served relative to the cwd
host=${RIPAE_DEV_HOST:-spacedock}
password=${RIPAE_DEV_PASSWORD:-changeme}
dir=.dev
db=$dir/$mode.db
mkdir -p "$dir"

if [ "$reset" = 1 ] && [ "$mode" = clean ]; then
  rm -f "$db" "$db-shm" "$db-wal"
fi

if [ "$mode" = live ] && { [ "$reset" = 1 ] || [ ! -f "$db" ]; }; then
  echo "dev.sh: pulling the live database from $host (needs the ripae-snapshot sudo rule, see README)" >&2
  # The remote login shell may be nu, so the remote command is one plain
  # word list with no shell syntax; the snapshot is then fetched with scp.
  ssh "$host" sudo ripae-snapshot
  tmp=$db.part
  scp -q "$host:.cache/ripae-snapshot.db" "$tmp"
  if [ "$(head -c 15 "$tmp")" != "SQLite format 3" ]; then
    rm -f "$tmp"
    echo "dev.sh: what came back from $host is not a SQLite database" >&2
    exit 1
  fi
  rm -f "$db-shm" "$db-wal"
  mv "$tmp" "$db"
fi

# Nothing in the caller's environment (an .envrc with the real Discord
# secrets, say) may leak into a test copy.
unset RIPAE_DISCORD_CLIENT_ID RIPAE_DISCORD_CLIENT_SECRET RIPAE_DISCORD_GUILD_ID \
      RIPAE_PUBLIC_URL RIPAE_SEED_USERS RIPAE_DEV_RESET_LOGINS

export RIPAE_BIND=${RIPAE_BIND:-127.0.0.1:8080}
export RIPAE_DB_PATH=$db
export RIPAE_COOKIE_SECURE=0
export RIPAE_DEV_QUICK_LOGIN=1
export RIPAE_SESSION_SECRET=$(head -c 64 /dev/urandom | base64 | tr -d '\n')
if [ "$mode" = live ]; then
  export RIPAE_DEV_RESET_LOGINS=$password
else
  export RIPAE_SEED_USERS="lordgig:Lord Gig:$password,firstofficer:First Officer:$password"
fi

echo "dev.sh: $mode database $db, http://$RIPAE_BIND, password '$password'" >&2
exec cargo run
