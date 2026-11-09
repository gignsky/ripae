#!/bin/sh
# One-shot: create the release tags that roll-flow never cut for 0.1.x and push them.
# Why they were missing: .roll-flow.toml had tag_on_promote = false (promote = rolling -> master
# is the release step), tag_on_graduate = true (graduate = roll -> rolling, a dev version), and
# push_tag = false, so nothing was ever tagged on master and nothing would have been pushed anyway.
# Those three are flipped in this commit. Run this once from the repo root, then delete it.
set -eu
tag() { git tag -a "$1" "$2" -m "$3"; }
tag v0.1.0 aa53784 'Ripae 0.1.0 "Dockside" -- first alpha (retro-tagged 2026-11-08; master as last promoted before roll 1)'
tag v0.1.1 15e7665 'Ripae 0.1.1 -- shared crew ledger, pick-lists with fuzzy search, version badge + changelog (retro-tagged 2026-11-08)'
tag v0.1.2 bbb90b9 'Ripae 0.1.2 -- real-calendar date pickers (retro-tagged 2026-11-08)'
tag v0.1.3 6d08a6d 'Ripae 0.1.3 -- hotfix: PUT /runs/{id} route restored (retro-tagged 2026-11-08)'
git push origin v0.1.0 v0.1.1 v0.1.2 v0.1.3
echo "tags pushed; now: git rm scripts/retro-tags.sh"
