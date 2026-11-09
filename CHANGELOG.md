# Changelog

Every notable change to Ripae. Newest first. This file is also what the
version badge in the corner of the site shows.

## 0.3.1

### Changed
- **Log out is tucked away.** The top bar's *Log out* is now *Close*,
  moved up from the bottom of the screen. It brings the splash back up,
  and the splash is now the only place to log out, so a stray click on
  the top bar no longer signs you out.

## 0.3.0

### Added
- **Sign in with Discord.** The splash's gate is now *Sign in with
  Discord*: anyone in the NISC Discord server can get in, and the first
  time you do, Ripae makes your account. There's no password to remember
  and none to ask an officer for.
- **Your Discord name and face.** Every time you sign in, your profile is
  brought up to date from Discord: your Discord username is your handle,
  your nickname in the NISC server (or your Discord display name, if you
  haven't set one) is the name on the ledger and the leaderboard, and your
  avatar sits on your service record, in phosphor green like everything
  else.

### Changed
- **Accounts from before Discord are claimed once.** If you had a Ripae
  account with a handle and password, open *Crew from before Discord?
  Claim your account* on the gate and enter them once; Discord then links
  that account to you, with all its voyages and standings. After that you
  sign in with Discord, and the old password no longer works.

## 0.2.9

### Added
- **Change whose run it is.** A voyage's *Edit* form (in Open Cargo, and
  the *Voyage* button on the ledger) now has a *Crew member* choice, so a
  run logged under the wrong person can be put right. It changes only the
  run unless you tick *Move the old crew member's unsold loads too*; loads
  that have already sold are never moved.
- **Correct who bought a sold load.** The ledger's *Sale* form has a
  *Bought by* choice (*Hauled by* on a delivery) next to *Sold by*. A sale
  used to forget who bought the cargo once it was sold; it now remembers.
  Sales recorded before this version show the run's owner as the buyer,
  since the real one wasn't kept -- check them and correct any that are
  wrong.

### Fixed
- **Bought by on a load's Edit form.** The *Bought by* choice when editing
  an open load was empty and its value was ignored; it now lists the crew
  and saves.
- **Ledger commands spilling out of the panel.** The *Sale* and *Voyage*
  buttons on each ledger row now stack, so the last column stays inside
  the panel.

## 0.2.8

### Added
- **Hauling contracts.** A voyage can now carry hauling-mission cargo
  alongside the commodities you bought. Every load line in *New Run* and
  *Add load* starts with a *Cargo* choice: pick *Hauling contract* and the
  buy price gives way to the *Contract reward*, the terminals read *Pick up
  at* and *Deliver to*, and the load is marked *HAUL* in Open Cargo with
  its reward where the cost would be. *Deliver* records the payout the way
  *Close* records a sale -- the reward is filled in, the crates and shares
  work the same -- and all of it is profit. The reward only counts once
  the delivery is recorded, and losing a contract's crates costs nothing.
- **Profit shares for a whole voyage.** A voyage's *Edit* form (in Open
  Cargo, and the *Voyage* button on the ledger) has a *Split profit*
  section. Set the split once and every load you close on that voyage
  comes up with it filled in; change it on any one sale if that one splits
  differently. The run's owner keeps the remainder. A detached load takes
  the split with it.

### Changed
- **Even shares by default.** A new percentage share starts as an even cut
  between everyone splitting the profit -- one crewmate is 50 %, two are
  33.33 % each -- and evens out again as shares are added or removed, until
  you type your own figure. It also goes to the first crew member who
  doesn't have a share yet.
- **Cost per SCU in Open Cargo.** Each load's *Cost* now shows what it cost
  per SCU underneath the total.

### Fixed
- **Add load was one long column.** The form under a voyage's *Add load*
  stacked every field in a single narrow column; it now lays the fields
  out across the row like the other forms.

## 0.2.7

### Fixed
- **Detach only where it means something.** A load's *Detach* button now
  shows only when its voyage has another open load to leave behind. On a
  voyage with a single load it used to just move the load to a new voyage
  number.

### Changed
- **Write off moved into the crate manifest.** *Write off* is no longer on
  every load row; open *Crates* and it sits in the manifest header next to
  *All aboard* and *All stored*. It still asks before anything is lost.

## 0.2.6

### Changed
- **Lost cargo counts as a loss.** What a lost or written-off crate cost
  now comes off the overall profit: *Realised* on Open Cargo and *Org
  realised* on the board, whether the load is still open, sold, or
  written off whole. The same amount comes off the lifetime profit and
  cash of whoever's cargo it was, and their service record shows it next
  to *Crates lost*. A sale's own profit, and the shares paid on it, still
  count only the crates that sold.

## 0.2.5

### Changed
- **MU-TH-UR screen.** Everything that paints in now comes up a line at a
  time: each row, heading, field and tile flickers on in turn, top to
  bottom, with a brief phosphor flare, and the RIPAE banner wipes down row
  by row. The glass hums too: the scanlines breathe, a faint refresh band
  rolls down the screen and the picture slips sync now and then. All of it
  is off when your system asks for reduced motion.

### Fixed
- **From balance showed one box instead of two.** After an update the
  browser could keep running the old script, which didn't know the
  *From balance* mode and showed *Paid for the lot* instead of the
  before/after balances. The stylesheet and scripts are now tagged with
  the version, so each release loads fresh.

## 0.2.4

### Added
- **Edit a recorded sale.** Every ledger row has a *Sale* button that
  reopens the sale: who sold it, the date, the sell terminal, the price
  (per SCU, per lot or from your balance) and the profit shares. Profit
  and every share are worked out again on the corrected figures, with the
  same live preview as the close form. Which crates sold doesn't change.

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
- **Edit closed voyages.** Every ledger row has a *Voyage N* button that
  opens the voyage's ship, date, notes and *Logged by* in an overlay, so a
  voyage can be corrected after every load has sold and it's left Open
  Cargo.

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
