#!/bin/sh
# Runs ON spacedock, as root (via sudo): writes a consistent snapshot of the
# live ripae database to ~$SUDO_USER/.cache/ripae-snapshot.db for
# `scripts/dev.sh live` to scp. Install it as the `ripae-snapshot` command
# with the sudoers rule in the README ("Running a test copy locally").
# sqlite's online backup is safe while the app is writing.
set -eu

src=${RIPAE_LIVE_DB:-/var/lib/ripae/ripae.db}
user=${SUDO_USER:?run this through sudo}
home=$(getent passwd "$user" | cut -d: -f6)
out=$home/.cache/ripae-snapshot.db

mkdir -p "$home/.cache"
rm -f "$out"
sqlite3 "$src" ".backup '$out'"
chown "$user" "$out"
chmod 600 "$out"
echo "ripae-snapshot: $src -> $out"
