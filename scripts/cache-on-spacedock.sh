#!/bin/sh
# roll-flow gate (rolling -> master): build the container image on spacedock,
# exactly as its deploy will, so the rebuild after promotion finds it (and
# crane's dependency layer) already in spacedock's store instead of compiling.
#
# spacedock's dotfiles take ripae with `inputs.nixpkgs.follows = "nixpkgs"`,
# so the image it deploys is built against *its* nixpkgs (gigpkgs), not the
# one in ripae's flake.lock; a plain `nix build .#container` would never be
# reused. The build here overrides nixpkgs with whatever the dotfiles on
# spacedock have locked, which gives the same derivation (checked 9 Nov 2026:
# both bdca774n...-ripae.tar.gz.drv for 91d3c1d). If the dotfiles' nixpkgs
# is bumped between promoting and deploying, the deploy builds it afresh.
#
# The build runs on spacedock (`--store ssh-ng://`), whichever computer
# promotes; the ssh user must be in spacedock's nix trusted-users. The image
# is then held by a GC root at ~/.cache/ripae-container on spacedock (each
# promotion replaces it), so a garbage collection can't drop it before the
# deploy. RIPAE_CACHE_HOST names another ssh host to do this on.
set -eu

host=${RIPAE_CACHE_HOST:-spacedock}
lock=$(mktemp)
trap 'rm -f "$lock"' EXIT

ssh "$host" cat .dotfiles/flake.lock > "$lock"
nixpkgs=$(LOCK="$lock" nix eval --raw --impure --expr '
  let
    lock = builtins.fromJSON (builtins.readFile (/. + builtins.getEnv "LOCK"));
    n = lock.nodes.${lock.nodes.root.inputs.nixpkgs}.locked;
  in
  if n.type == "github" then "github:${n.owner}/${n.repo}/${n.rev}"
  else if n.type == "git" then "git+${n.url}?${if n ? ref then "ref=${n.ref}&" else ""}rev=${n.rev}"
  else throw "cache-on-spacedock: unhandled nixpkgs lock type ${n.type}"
')

echo "cache-on-spacedock: building .#container on $host against its nixpkgs ($nixpkgs)"
out=$(nix build ".#container" \
  --override-input nixpkgs "$nixpkgs" --no-write-lock-file \
  --store "ssh-ng://$host" --eval-store auto \
  --no-link --print-out-paths)

ssh "$host" nix-store --realise --add-root .cache/ripae-container "$out" > /dev/null
echo "cache-on-spacedock: $out is in $host's store"
