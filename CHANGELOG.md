# Changelog

Every notable change to Ripae. Newest first. This file is also what the
version badge in the corner of the site shows.

## 0.2.3

### Added
- **Live ledger.** When a crewmate logs a run, edits a load, moves crates
  or records a sale, everyone else's ledger screen catches up on its own:
  totals, Open Cargo, the stowage plan and the ledger refresh in place and
  only the lines that changed paint in. A row you have open (an edit,
  close or add-load form, or a crate manifest) is left alone until you're
  done with it.
- **Price from your balance.** The trade terminal doesn't always show what
  you actually paid or got. *Price as → From balance* takes your aUEC
  balance before and after the trade and books the difference as the lot
  price, on both the buy and the sale. A purchase that raises the balance
  (or a sale that lowers it) is refused as probably swapped. The balances
  aren't kept; the load stores the lot price as usual.
- **Write off cargo.** *Write off* on an open load marks every crate
  still in play lost, so glitched or vanished boxes you'll never sell stop
  sitting in Open Cargo. The load leaves Open Cargo without a sale and
  without a blank line in the ledger.
- **Glitched crates are written off at close.** When you close a load
  with glitched crates you aren't selling, they're marked lost by default
  instead of staying open on the voyage, so the load closes whole. Untick
  the box in the close form to keep them open.

### Fixed
- **Profit shares count on the leaderboard.** A crew member on a share of
  a sale's profit now gets the run, the load, its SCU and the route, not
  just the aUEC. Before, a 50 % partner who never closed a sale showed
  no runs and no loads. *Shares paid out* still counts only what you paid.
- **"Logged by" names whoever typed the run in.** Keeping the books for a
  crewmate used to mark the whole voyage as logged by them. Now the
  voyage header shows the bookkeeper, and the loads' *By* column still
  shows whose cargo it is. Voyages logged before this show their owner;
  pick the right name under *Logged by* in the voyage's Edit form.

## 0.2.2

### Fixed
- **Editing a load saves again.** On a load bought per lot whose price
  doesn't divide evenly into cents per SCU (1,000 aUEC for 3 SCU), *Save*
  did nothing: the hidden per-SCU field held 333.333… and the browser
  quietly refused to submit it. Edit now shows prices to the cent and the
  price fields take any figure.

## 0.2.1

### Added
- **Ironclad, Ironclad Assault and Centaurus** in the ships list. They
  were missing from the sc-cargo.space scrape, so the new-run Ship picker
  didn't offer them.
- **Every Stanton trade terminal** in the locations list: all the L-point
  stations, gateways (Nyx, Pyro, Terra), mining facilities, research
  outposts, salvage yards and distribution centres UEX lists as live.

### Changed
- **Free cargo needs no buy terminal.** When the buy price is 0, the buy
  terminal can be left blank; it shows as `—`, and such loads don't count
  towards best or favourite routes. Any price above 0 still needs one.

### Fixed
- **Custom locations stick.** Typing a station that isn't on the list and
  pressing Enter or Tab no longer swaps it for the nearest fuzzy match.
  The list only pre-highlights a name that actually contains what you
  typed; arrow down to pick a fuzzy match on purpose.

## 0.2.0 "Stowage"

### Added
- **The CRT terminal.** Every screen now wears the ship's-computer look:
  green phosphor on black glass, scanlines, bracketed `[COMMANDS]`, and
  everything that appears or disappears paints in line by line. The splash
  screen is the login: boot log, then *Speak, friend, and enter*. With a
  valid session it welcomes you back instead. `[CLOSE]` in the footer
  replays it; `[LOG OUT]` lives on the rail and on the splash.
- **Voyages, loads and crates.** A run is now a *voyage* (ship, date)
  carrying one or more *cargo loads*, and every load is made of *crates*
  (`2×32, 4×8`). Log several commodities in one run; add a load to a voyage
  later; merge a load into another voyage or detach it into one of its own.
- **Buy per SCU or per lot.** Enter whichever figure you have; the other is
  worked out and both are kept.
- **Where a crate is lives on the crate.** Open the crate manifest to mark
  crates aboard or stored (one at a time, or `[ALL ABOARD]` /
  `[ALL STORED]`), glitched, or lost. A load's status follows its crates:
  `IN TRANSIT`, `STORED`, `AWAITING PICKUP`, or `SPLIT` when they're in
  more than one place. Lost SCU is written off and shown.
- **Close per crate.** Pick which crates sold (`[ALL SOUND CRATES]` /
  `[NONE]`; glitched crates by hand, lost ones never), price per SCU or
  per lot, and split the profit with crew by percentage or flat fee. Kept
  crates stay open on the voyage. Nothing is owed on a losing sale. A share
  can be *forfeited* to another member: the earner keeps the leaderboard
  credit, the aUEC goes to whoever was named.
- **SALE banner.** Recording a sale flickers the screen to black and shows
  the profit, route, shares, and what the seller keeps.
- **Stowage plan** with a working **sc-cargo.space** link for each hulled
  voyage: last stop loads first (deepest), larger crates first within a
  stop. Hull-less voyages bound the same way are offered as one with a
  `[MERGE]` command.
- **Leaderboard** (`/board`): crew standings by profit, runs or SCU, org
  totals, recent shares. Click a crew name for `HOORAY` (first by profit)
  or `WOMP WOMP` and that member's service record.
- **Deleting a voyage** asks first, on its own screen, with the loads and
  committed aUEC it would take with it. No browser confirm dialogs anywhere.
- **Ships list** of 80 hulls from sc-cargo.space, with cargo capacity.
- **Keep the books for a crewmate.** New Run has a *Crew member* picker,
  Add Load a *Bought by* one, and the close form a *Sold by* one (defaulting
  to whoever bought the load). Log a run as JT's and JT is the buyer, the
  seller and the one credited on the board; the SALE banner notes who did
  the booking.

### Changed
- **Pick-lists are suggestions, not gates.** Any commodity or terminal name
  is accepted as typed; a name on the list is stored with the list's
  spelling. A ship, though, must be one of the 80 known hulls (or blank).
- Tables never scroll sideways; below ~980 px each row becomes a labelled
  card.

### Removed
- **The 0.1 database.** Nothing in it was worth keeping, so the schema
  starts over: the deploy wipes `ripae.db` and reseeds the two accounts
  from `RIPAE_SEED_USERS`. See the README for the steps (and how to keep
  the accounts' "aboard since" dates).

## 0.1.4

### Fixed
- **Inline edit and close forms show their errors.** When a save was
  refused (a terminal not on the pick-list, a stored run with no station,
  a date in the future) the message was sent back but the page swallowed
  it, so the form looked frozen. The message now appears in the form.
  This is what 0.1.3 set out to fix and only half did.

## 0.1.3

### Fixed
- **Editing a run works again.** Saving the edit form used to fail, so
  changes to a run were never stored. Edits now save and the row updates
  in place.

## 0.1.2

### Changed
- **Real-world dates.** Date fields are now date pickers using the real
  calendar instead of in-game SC dates. "Bought on" and "Sold on" default to
  today's date.
- Dates already logged in SC years (e.g. 2956) were converted to the real
  year (SC year minus 930), so old and new runs sort together.
- A date far in the future is refused with a reminder to use the real
  date, which catches an SC date typed by habit.

## 0.1.1

### Added
- **Shared crew ledger.** Either of you can now edit, close, or delete any
  run, not just your own. Cargo JT bought, Gig can sell.
- **"By" column** in Open Cargo and the Ledger, showing who logged each run.
- **Fixed pick-lists** for commodities, terminals, and ships, with a
  fuzzy-search dropdown. Type a few letters (e.g. `am141`) to find
  *ArcCorp Mining Area 141*. Names must come from the list, so the ledger
  stays consistent and is ready to match against UEX data in 0.4.
- **Smart defaults for the next run.** The buy terminal starts as the crew's
  most recent delivery point, and the ship starts as the crew's most
  recently used ship.
- **Version badge and changelog.** Click the version in the corner to see
  this page.

### Changed
- Commodity, terminal, and ship names are saved with the list's spelling
  (e.g. `RAFT`, `ArcCorp`), instead of being title-cased.

## 0.1.0 "Dockside"

### Added
- First alpha. Log commodity runs, track open cargo (in transit or stored)
  until it's sold, then close it with the sale price to see profit and
  margin.
- Two seeded crew accounts, a shared run table, and running totals of
  realised profit and aUEC tied up in open cargo.
- `GET /healthz` for status checks.
