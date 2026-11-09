# Changelog

Every notable change to Ripae. Newest first. This file is also what the
version badge in the corner of the site shows.

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
