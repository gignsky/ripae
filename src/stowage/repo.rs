//! SQL for voyages, loads, crates and shares. Runtime `sqlx::query` with
//! hand-written row mappers (see README → Decisions: no compile-time
//! `query!` so the build needs no live database).
//!
//! Reads assemble whole `Voyage` trees (voyage → loads → crates + shares)
//! in four queries; writes that touch more than one row run in a
//! transaction on the pool's single connection.

use anyhow::{anyhow, Result};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::collections::HashMap;

use super::model::{
    expand_boxes, BoxMix, Condition, Crate, CrewMember, CrewTerms, Load, LoadKind, PriceMode, Resale, Sale, Share, Ship,
    User, Voyage, Whereabouts,
};

// ---------------------------------------------------------------------------
// inputs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NewVoyage {
    pub ship_id: Option<i64>,
    pub bought_at: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewLoad {
    pub kind: LoadKind,
    pub commodity: String,
    pub buy_terminal: String,
    pub price_mode: PriceMode,
    pub buy_price_scu: f64,
    pub buy_price_lot: f64,
    pub stop_no: u32,
    pub destination: Option<String>,
    pub boxes: BoxMix,
    /// A haul's promised payout.
    pub reward: Option<f64>,
    /// Stamped onto every crate (the New Run form takes one status for all).
    pub whereabouts: Whereabouts,
    pub stored_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EditVoyage {
    pub ship_id: Option<i64>,
    pub bought_at: String,
    pub notes: Option<String>,
    /// Whose run it is.
    pub logged_by: i64,
    /// Also hand the old owner's unsold loads to the new one.
    pub move_loads: bool,
    pub entered_by: i64,
    /// Replaces the voyage's crew and their share terms; `None` leaves the
    /// crew as it is. The owner, buyers and sellers stay on it regardless.
    pub crew: Option<Vec<CrewTerms>>,
}

#[derive(Debug, Clone)]
pub struct EditLoad {
    pub kind: LoadKind,
    pub commodity: String,
    pub buy_terminal: String,
    pub price_mode: PriceMode,
    pub buy_price_scu: f64,
    pub buy_price_lot: f64,
    pub stop_no: u32,
    pub destination: Option<String>,
    pub reward: Option<f64>,
    /// `Some` moves every live crate there (the edit form's status field);
    /// `None` leaves the crates where they are (a split load).
    pub whereabouts: Option<Whereabouts>,
    pub stored_at: Option<String>,
    pub notes: Option<String>,
    /// Who bought it; on an open load also who holds it (`submitted_by`).
    pub bought_by: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveTarget {
    Voyage(i64),
    New,
}

// ---------------------------------------------------------------------------
// row mappers
// ---------------------------------------------------------------------------

fn row_to_ship(row: &SqliteRow, prefix: &str) -> Result<Option<Ship>> {
    let id: Option<i64> = row.try_get(format!("{prefix}id").as_str())?;
    let Some(id) = id else { return Ok(None) };
    Ok(Some(Ship {
        id,
        manufacturer: row.try_get(format!("{prefix}manufacturer").as_str())?,
        name: row.try_get(format!("{prefix}name").as_str())?,
        short: row.try_get(format!("{prefix}short").as_str())?,
        sc_cargo_slug: row.try_get(format!("{prefix}sc_cargo_slug").as_str())?,
        capacity_scu: row.try_get(format!("{prefix}capacity_scu").as_str())?,
        concept: row.try_get::<i64, _>(format!("{prefix}concept").as_str())? != 0,
    }))
}

fn row_to_voyage(row: &SqliteRow) -> Result<Voyage> {
    Ok(Voyage {
        id: row.try_get("id")?,
        logged_by: row.try_get("logged_by")?,
        logged_by_name: row.try_get("logged_by_name")?,
        entered_by: row.try_get("entered_by")?,
        entered_by_name: row.try_get("entered_by_name")?,
        ship: row_to_ship(row, "ship_")?,
        bought_at: row.try_get("bought_at")?,
        notes: row.try_get("notes")?,
        loads: Vec::new(),
        crew: Vec::new(),
    })
}

fn row_to_load(row: &SqliteRow) -> Result<Load> {
    let mode: String = row.try_get("price_mode")?;
    let kind: String = row.try_get("kind")?;
    let stop_no: i64 = row.try_get("stop_no")?;
    Ok(Load {
        id: row.try_get("id")?,
        voyage_id: row.try_get("voyage_id")?,
        submitted_by: row.try_get("submitted_by")?,
        submitted_by_name: row.try_get("submitted_by_name")?,
        bought_by: row.try_get("bought_by")?,
        bought_by_name: row.try_get("bought_by_name")?,
        kind: kind.parse().map_err(|e| anyhow!("bad load kind in db: {e}"))?,
        commodity: row.try_get("commodity")?,
        buy_terminal: row.try_get("buy_terminal")?,
        price_mode: mode.parse().map_err(|e| anyhow!("bad price_mode in db: {e}"))?,
        buy_price_scu: row.try_get("buy_price_scu")?,
        buy_price_lot: row.try_get("buy_price_lot")?,
        stop_no: stop_no.max(1) as u32,
        destination: row.try_get("destination")?,
        sold_at: row.try_get("sold_at")?,
        sell_terminal: row.try_get("sell_terminal")?,
        sell_price_scu: row.try_get("sell_price_scu")?,
        sell_price_lot: row.try_get("sell_price_lot")?,
        reward: row.try_get("reward")?,
        notes: row.try_get("notes")?,
        crates: Vec::new(),
        shares: Vec::new(),
    })
}

fn row_to_crate(row: &SqliteRow) -> Result<Crate> {
    let condition: String = row.try_get("condition")?;
    let whereabouts: String = row.try_get("whereabouts")?;
    let size: i64 = row.try_get("size_scu")?;
    Ok(Crate {
        id: row.try_get("id")?,
        load_id: row.try_get("load_id")?,
        size_scu: size as u32,
        condition: condition.parse().map_err(|e| anyhow!("bad condition in db: {e}"))?,
        whereabouts: whereabouts.parse().map_err(|e| anyhow!("bad whereabouts in db: {e}"))?,
        stored_at: row.try_get("stored_at")?,
        sold_at: row.try_get("sold_at")?,
    })
}

fn row_to_share(row: &SqliteRow) -> Result<Share> {
    let kind: String = row.try_get("kind")?;
    Ok(Share {
        id: row.try_get("id")?,
        load_id: row.try_get("load_id")?,
        to_user: row.try_get("to_user")?,
        to_user_name: row.try_get("to_user_name")?,
        paid_to: row.try_get("paid_to")?,
        paid_to_name: row.try_get("paid_to_name")?,
        kind: kind.parse().map_err(|e| anyhow!("bad share kind in db: {e}"))?,
        amount: row.try_get("amount")?,
        reason: row.try_get("reason")?,
        paid_auec: row.try_get("paid_auec")?,
    })
}

const SELECT_VOYAGE: &str = "SELECT v.id, v.logged_by, u.display_name AS logged_by_name,
    COALESCE(v.entered_by, v.logged_by) AS entered_by, e.display_name AS entered_by_name,
    v.bought_at, v.notes,
    s.id AS ship_id, s.manufacturer AS ship_manufacturer, s.name AS ship_name, s.short AS ship_short,
    s.sc_cargo_slug AS ship_sc_cargo_slug, s.capacity_scu AS ship_capacity_scu, s.concept AS ship_concept
    FROM voyages v JOIN users u ON u.id = v.logged_by
    JOIN users e ON e.id = COALESCE(v.entered_by, v.logged_by) LEFT JOIN ships s ON s.id = v.ship_id";

const SELECT_LOAD: &str = "SELECT l.id, l.voyage_id, l.submitted_by, u.display_name AS submitted_by_name,
    COALESCE(l.bought_by, l.submitted_by) AS bought_by, b.display_name AS bought_by_name,
    l.kind, l.commodity, l.buy_terminal, l.price_mode, l.buy_price_scu, l.buy_price_lot, l.stop_no,
    l.destination, l.sold_at, l.sell_terminal, l.sell_price_scu, l.sell_price_lot, l.reward, l.notes
    FROM loads l JOIN users u ON u.id = l.submitted_by
    JOIN users b ON b.id = COALESCE(l.bought_by, l.submitted_by)";

const SELECT_CRATE: &str =
    "SELECT id, load_id, size_scu, condition, whereabouts, stored_at, sold_at FROM crates";

const SELECT_SHARE: &str = "SELECT sh.id, sh.load_id, sh.to_user, tu.display_name AS to_user_name,
    sh.paid_to, pu.display_name AS paid_to_name, sh.kind, sh.amount, sh.reason, sh.paid_auec
    FROM shares sh JOIN users tu ON tu.id = sh.to_user JOIN users pu ON pu.id = sh.paid_to";

/// Crew rows, the run's owner first, then in the order they joined.
const SELECT_CREW: &str = "SELECT c.voyage_id, c.user_id, u.display_name AS name, c.pct, c.flat, c.paid_to,
    pu.display_name AS paid_to_name, c.reason
    FROM voyage_crew c JOIN voyages v ON v.id = c.voyage_id JOIN users u ON u.id = c.user_id
    LEFT JOIN users pu ON pu.id = c.paid_to";
const ORDER_CREW: &str = "ORDER BY c.voyage_id, (c.user_id = v.logged_by) DESC, c.id";

fn row_to_crew(row: &SqliteRow) -> Result<(i64, CrewMember)> {
    let user_id: i64 = row.try_get("user_id")?;
    let paid_to: Option<i64> = row.try_get("paid_to")?;
    let paid_to = paid_to.filter(|&p| p != user_id);
    Ok((
        row.try_get("voyage_id")?,
        CrewMember {
            terms: CrewTerms { user_id, pct: row.try_get("pct")?, flat: row.try_get("flat")?, paid_to, reason: row.try_get("reason")? },
            name: row.try_get("name")?,
            paid_to_name: if paid_to.is_some() { row.try_get("paid_to_name")? } else { None },
        },
    ))
}

fn attach_crew(voyages: &mut [Voyage], rows: Vec<SqliteRow>) -> Result<()> {
    let mut by_voyage: HashMap<i64, Vec<CrewMember>> = HashMap::new();
    for r in &rows {
        let (v, c) = row_to_crew(r)?;
        by_voyage.entry(v).or_default().push(c);
    }
    for v in voyages.iter_mut() {
        v.crew = by_voyage.remove(&v.id).unwrap_or_default();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// reads
// ---------------------------------------------------------------------------

/// Every voyage with its loads, crates, shares and crew. Voyages oldest
/// first (then by id); loads, crates and shares by id.
pub async fn list_voyages(pool: &SqlitePool) -> Result<Vec<Voyage>> {
    let mut voyages = list_voyages_without_crew(pool).await?;
    let crew = sqlx::query(&format!("{SELECT_CREW} {ORDER_CREW}")).fetch_all(pool).await?;
    attach_crew(&mut voyages, crew)?;
    Ok(voyages)
}

/// The voyage trees without their crew -- everything the money is worked
/// out from. Also what a database from before `voyage_crew` can be read with.
pub(crate) async fn list_voyages_without_crew(pool: &SqlitePool) -> Result<Vec<Voyage>> {
    let rows = sqlx::query(&format!("{SELECT_VOYAGE} ORDER BY v.bought_at, v.id"))
        .fetch_all(pool)
        .await?;
    let mut voyages: Vec<Voyage> = rows.iter().map(row_to_voyage).collect::<Result<_>>()?;
    let loads = sqlx::query(&format!("{SELECT_LOAD} ORDER BY l.id"))
        .fetch_all(pool)
        .await?
        .iter()
        .map(row_to_load)
        .collect::<Result<Vec<_>>>()?;
    let crates = sqlx::query(&format!("{SELECT_CRATE} ORDER BY id"))
        .fetch_all(pool)
        .await?
        .iter()
        .map(row_to_crate)
        .collect::<Result<Vec<_>>>()?;
    let shares = sqlx::query(&format!("{SELECT_SHARE} ORDER BY sh.id"))
        .fetch_all(pool)
        .await?
        .iter()
        .map(row_to_share)
        .collect::<Result<Vec<_>>>()?;
    assemble(&mut voyages, loads, crates, shares);
    Ok(voyages)
}

fn assemble(voyages: &mut [Voyage], loads: Vec<Load>, crates: Vec<Crate>, shares: Vec<Share>) {
    let mut by_load: HashMap<i64, Load> = loads.into_iter().map(|l| (l.id, l)).collect();
    for c in crates {
        if let Some(l) = by_load.get_mut(&c.load_id) {
            l.crates.push(c);
        }
    }
    for s in shares {
        if let Some(l) = by_load.get_mut(&s.load_id) {
            l.shares.push(s);
        }
    }
    let mut loads: Vec<Load> = by_load.into_values().collect();
    loads.sort_by_key(|l| l.id);
    let mut by_voyage: HashMap<i64, Vec<Load>> = HashMap::new();
    for l in loads {
        by_voyage.entry(l.voyage_id).or_default().push(l);
    }
    for v in voyages.iter_mut() {
        v.loads = by_voyage.remove(&v.id).unwrap_or_default();
    }
}

pub async fn find_voyage(pool: &SqlitePool, id: i64) -> Result<Option<Voyage>> {
    let row = sqlx::query(&format!("{SELECT_VOYAGE} WHERE v.id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let mut voyages = vec![row_to_voyage(&row)?];
    let loads = sqlx::query(&format!("{SELECT_LOAD} WHERE l.voyage_id = ? ORDER BY l.id"))
        .bind(id)
        .fetch_all(pool)
        .await?
        .iter()
        .map(row_to_load)
        .collect::<Result<Vec<_>>>()?;
    let crates = sqlx::query(&format!(
        "{SELECT_CRATE} WHERE load_id IN (SELECT id FROM loads WHERE voyage_id = ?) ORDER BY id"
    ))
    .bind(id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(row_to_crate)
    .collect::<Result<Vec<_>>>()?;
    let shares = sqlx::query(&format!(
        "{SELECT_SHARE} WHERE sh.load_id IN (SELECT id FROM loads WHERE voyage_id = ?) ORDER BY sh.id"
    ))
    .bind(id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(row_to_share)
    .collect::<Result<Vec<_>>>()?;
    assemble(&mut voyages, loads, crates, shares);
    let crew = sqlx::query(&format!("{SELECT_CREW} WHERE c.voyage_id = ? {ORDER_CREW}")).bind(id).fetch_all(pool).await?;
    attach_crew(&mut voyages, crew)?;
    Ok(voyages.pop())
}

/// A voyage's crew and their terms, the run's owner first.
pub async fn voyage_crew(pool: &SqlitePool, voyage_id: i64) -> Result<Vec<CrewMember>> {
    let rows = sqlx::query(&format!("{SELECT_CREW} WHERE c.voyage_id = ? {ORDER_CREW}"))
        .bind(voyage_id)
        .fetch_all(pool)
        .await?;
    rows.iter().map(|r| row_to_crew(r).map(|(_, c)| c)).collect()
}

/// Puts everyone a voyage names on its crew, without a share, if they
/// aren't already: the owner, every load's buyer and holder (a sold row's
/// seller), and whoever a share or a crew forfeit pays. Terms already set
/// are left alone. Migration 0012 did the same for every voyage.
async fn sync_crew(conn: &mut SqliteConnection, voyage_id: i64) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO voyage_crew (voyage_id, user_id)
         SELECT id, logged_by FROM voyages WHERE id = ?1
         UNION ALL SELECT voyage_id, submitted_by FROM loads WHERE voyage_id = ?1
         UNION ALL SELECT voyage_id, bought_by FROM loads WHERE voyage_id = ?1 AND bought_by IS NOT NULL
         UNION ALL SELECT l.voyage_id, s.to_user FROM shares s JOIN loads l ON l.id = s.load_id WHERE l.voyage_id = ?1
         UNION ALL SELECT l.voyage_id, s.paid_to FROM shares s JOIN loads l ON l.id = s.load_id WHERE l.voyage_id = ?1
         UNION ALL SELECT voyage_id, paid_to FROM voyage_crew WHERE voyage_id = ?1 AND paid_to IS NOT NULL",
    )
    .bind(voyage_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn find_load(pool: &SqlitePool, id: i64) -> Result<Option<Load>> {
    let row = sqlx::query(&format!("{SELECT_LOAD} WHERE l.id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let mut load = row_to_load(&row)?;
    load.crates = sqlx::query(&format!("{SELECT_CRATE} WHERE load_id = ? ORDER BY id"))
        .bind(id)
        .fetch_all(pool)
        .await?
        .iter()
        .map(row_to_crate)
        .collect::<Result<Vec<_>>>()?;
    load.shares = sqlx::query(&format!("{SELECT_SHARE} WHERE sh.load_id = ? ORDER BY sh.id"))
        .bind(id)
        .fetch_all(pool)
        .await?
        .iter()
        .map(row_to_share)
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(load))
}

pub async fn find_crate(pool: &SqlitePool, id: i64) -> Result<Option<Crate>> {
    let row = sqlx::query(&format!("{SELECT_CRATE} WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(row_to_crate).transpose()
}

pub async fn list_users(pool: &SqlitePool) -> Result<Vec<User>> {
    let rows = sqlx::query("SELECT id, handle, display_name, created_at, avatar_url FROM users ORDER BY id")
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|r| {
            Ok(User {
                id: r.try_get("id")?,
                handle: r.try_get("handle")?,
                display_name: r.try_get("display_name")?,
                created_at: r.try_get("created_at")?,
                avatar_url: r.try_get("avatar_url")?,
            })
        })
        .collect()
}

/// Case-insensitive lookup by the CSV spelling (`Hull-C`), trimmed.
pub async fn find_ship_by_name(pool: &SqlitePool, name: &str) -> Result<Option<Ship>> {
    let row = sqlx::query(
        "SELECT id AS ship_id, manufacturer AS ship_manufacturer, name AS ship_name, short AS ship_short,
         sc_cargo_slug AS ship_sc_cargo_slug, capacity_scu AS ship_capacity_scu, concept AS ship_concept
         FROM ships WHERE name = ?",
    )
    .bind(name.trim())
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(|r| row_to_ship(r, "ship_")).transpose().map(Option::flatten)
}

/// Where the crew last delivered -- the sell terminal of the most recent
/// sale -- used as the next run's default buy terminal.
pub async fn latest_sell_terminal(pool: &SqlitePool) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT sell_terminal FROM loads WHERE sold_at IS NOT NULL
         ORDER BY sold_at DESC, updated_at DESC, id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?)
}

/// The hull on the crew's most recently bought voyage, as the next default.
pub async fn latest_ship_name(pool: &SqlitePool) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT s.name FROM voyages v JOIN ships s ON s.id = v.ship_id
         ORDER BY v.bought_at DESC, v.id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?)
}

// ---------------------------------------------------------------------------
// writes
// ---------------------------------------------------------------------------

async fn insert_load(
    conn: &mut SqliteConnection,
    voyage_id: i64,
    submitted_by: i64,
    new: &NewLoad,
) -> Result<i64> {
    let row = sqlx::query(
        "INSERT INTO loads (voyage_id, submitted_by, bought_by, kind, commodity, buy_terminal, price_mode,
            buy_price_scu, buy_price_lot, stop_no, destination, reward, notes)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(voyage_id)
    .bind(submitted_by)
    .bind(submitted_by)
    .bind(new.kind.as_str())
    .bind(&new.commodity)
    .bind(&new.buy_terminal)
    .bind(new.price_mode.as_str())
    .bind(new.buy_price_scu)
    .bind(new.buy_price_lot)
    .bind(new.stop_no as i64)
    .bind(new.destination.as_deref())
    .bind(new.reward)
    .bind(new.notes.as_deref())
    .fetch_one(&mut *conn)
    .await?;
    let load_id: i64 = row.try_get("id")?;
    let stored_at = (new.whereabouts == Whereabouts::Stored).then(|| new.stored_at.clone()).flatten();
    for size in expand_boxes(&new.boxes) {
        sqlx::query(
            "INSERT INTO crates (load_id, size_scu, condition, whereabouts, stored_at)
             VALUES (?, ?, 'sound', ?, ?)",
        )
        .bind(load_id)
        .bind(size as i64)
        .bind(new.whereabouts.as_str())
        .bind(stored_at.as_deref())
        .execute(&mut *conn)
        .await?;
    }
    Ok(load_id)
}

/// New run: one voyage and its 1..n loads, each expanded into crates.
pub async fn create_voyage(
    pool: &SqlitePool,
    logged_by: i64,
    entered_by: i64,
    voyage: &NewVoyage,
    loads: &[NewLoad],
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "INSERT INTO voyages (logged_by, entered_by, ship_id, bought_at, notes) VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(logged_by)
    .bind(entered_by)
    .bind(voyage.ship_id)
    .bind(&voyage.bought_at)
    .bind(voyage.notes.as_deref())
    .fetch_one(&mut *tx)
    .await?;
    let voyage_id: i64 = row.try_get("id")?;
    for load in loads {
        insert_load(&mut tx, voyage_id, logged_by, load).await?;
    }
    sync_crew(&mut tx, voyage_id).await?;
    tx.commit().await?;
    Ok(voyage_id)
}

/// Add a load to an existing voyage (same ship and date).
pub async fn add_load(
    pool: &SqlitePool,
    voyage_id: i64,
    submitted_by: i64,
    load: &NewLoad,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let id = insert_load(&mut tx, voyage_id, submitted_by, load).await?;
    touch_voyage(&mut tx, voyage_id).await?;
    sync_crew(&mut tx, voyage_id).await?;
    tx.commit().await?;
    Ok(id)
}

async fn touch_voyage(conn: &mut SqliteConnection, voyage_id: i64) -> Result<()> {
    sqlx::query("UPDATE voyages SET updated_at = datetime('now') WHERE id = ?")
        .bind(voyage_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn update_voyage(pool: &SqlitePool, id: i64, edit: &EditVoyage) -> Result<()> {
    let mut tx = pool.begin().await?;
    if edit.move_loads {
        // a sold row's submitted_by is its seller, so only open loads follow the owner
        sqlx::query(
            "UPDATE loads SET submitted_by = ?1, bought_by = ?1, updated_at = datetime('now')
             WHERE voyage_id = ?2 AND sold_at IS NULL AND submitted_by = (SELECT logged_by FROM voyages WHERE id = ?2)",
        )
        .bind(edit.logged_by)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE voyages SET ship_id = ?, bought_at = ?, notes = ?, logged_by = ?, entered_by = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(edit.ship_id)
    .bind(&edit.bought_at)
    .bind(edit.notes.as_deref())
    .bind(edit.logged_by)
    .bind(edit.entered_by)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if let Some(crew) = &edit.crew {
        sqlx::query("DELETE FROM voyage_crew WHERE voyage_id = ?").bind(id).execute(&mut *tx).await?;
        for t in crew {
            sqlx::query(
                "INSERT INTO voyage_crew (voyage_id, user_id, pct, flat, paid_to, reason) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(t.user_id)
            .bind(t.pct)
            .bind(t.flat)
            .bind(t.paid_to.filter(|&p| p != t.user_id))
            .bind(t.reason.as_deref())
            .execute(&mut *tx)
            .await?;
        }
    }
    sync_crew(&mut tx, id).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn update_load(pool: &SqlitePool, id: i64, edit: &EditLoad) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE loads SET kind = ?, commodity = ?, buy_terminal = ?, price_mode = ?, buy_price_scu = ?,
            buy_price_lot = ?, stop_no = ?, destination = ?, reward = ?, notes = ?, bought_by = ?,
            submitted_by = CASE WHEN sold_at IS NULL THEN ? ELSE submitted_by END, updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(edit.kind.as_str())
    .bind(&edit.commodity)
    .bind(&edit.buy_terminal)
    .bind(edit.price_mode.as_str())
    .bind(edit.buy_price_scu)
    .bind(edit.buy_price_lot)
    .bind(edit.stop_no as i64)
    .bind(edit.destination.as_deref())
    .bind(edit.reward)
    .bind(edit.notes.as_deref())
    .bind(edit.bought_by)
    .bind(edit.bought_by)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if let Some(w) = edit.whereabouts {
        set_where_in(&mut tx, "load_id = ?", id, w, edit.stored_at.as_deref()).await?;
    }
    if let Some(v) = sqlx::query_scalar::<_, i64>("SELECT voyage_id FROM loads WHERE id = ?").bind(id).fetch_optional(&mut *tx).await? {
        sync_crew(&mut tx, v).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn set_where_in(
    conn: &mut SqliteConnection,
    filter: &str,
    id: i64,
    whereabouts: Whereabouts,
    stored_at: Option<&str>,
) -> Result<()> {
    let stored_at = (whereabouts == Whereabouts::Stored).then_some(stored_at).flatten();
    sqlx::query(&format!(
        "UPDATE crates SET whereabouts = ?, stored_at = ?
         WHERE {filter} AND condition IN ('sound','glitched')"
    ))
    .bind(whereabouts.as_str())
    .bind(stored_at)
    .bind(id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Merge a load into another voyage, or detach it into a voyage of its own
/// (same ship and date, logged by `actor`). A voyage left with no loads is
/// deleted. Returns the id of the voyage the load now belongs to.
pub async fn move_load(
    pool: &SqlitePool,
    load_id: i64,
    target: MoveTarget,
    actor: i64,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let from: i64 = sqlx::query_scalar("SELECT voyage_id FROM loads WHERE id = ?")
        .bind(load_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| anyhow!("no such load"))?;
    let to = match target {
        MoveTarget::Voyage(id) => {
            sqlx::query_scalar::<_, i64>("SELECT id FROM voyages WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| anyhow!("no such voyage"))?
        }
        MoveTarget::New => {
            let row = sqlx::query(
                "INSERT INTO voyages (logged_by, entered_by, ship_id, bought_at, notes)
                 SELECT ?, ?, ship_id, bought_at, NULL FROM voyages WHERE id = ? RETURNING id",
            )
            .bind(actor)
            .bind(actor)
            .bind(from)
            .fetch_one(&mut *tx)
            .await?;
            let id: i64 = row.try_get("id")?;
            // a detached load keeps the crew and split its voyage was flying
            sqlx::query(
                "INSERT INTO voyage_crew (voyage_id, user_id, pct, flat, paid_to, reason)
                 SELECT ?, user_id, pct, flat, paid_to, reason FROM voyage_crew WHERE voyage_id = ? ORDER BY id",
            )
            .bind(id)
            .bind(from)
            .execute(&mut *tx)
            .await?;
            id
        }
    };
    if to != from {
        sqlx::query("UPDATE loads SET voyage_id = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(to)
            .bind(load_id)
            .execute(&mut *tx)
            .await?;
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM loads WHERE voyage_id = ?")
            .bind(from)
            .fetch_one(&mut *tx)
            .await?;
        if left == 0 {
            sqlx::query("DELETE FROM voyages WHERE id = ?")
                .bind(from)
                .execute(&mut *tx)
                .await?;
        } else {
            touch_voyage(&mut tx, from).await?;
        }
        touch_voyage(&mut tx, to).await?;
        // whoever holds the load joins the crew it was merged into
        sync_crew(&mut tx, to).await?;
    }
    tx.commit().await?;
    Ok(to)
}

pub async fn set_crate_condition(pool: &SqlitePool, crate_id: i64, condition: Condition) -> Result<()> {
    sqlx::query("UPDATE crates SET condition = ? WHERE id = ? AND condition <> 'sold'")
        .bind(condition.as_str())
        .bind(crate_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_crate_where(
    pool: &SqlitePool,
    crate_id: i64,
    whereabouts: Whereabouts,
    stored_at: Option<&str>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    set_where_in(&mut tx, "id = ?", crate_id, whereabouts, stored_at).await?;
    tx.commit().await?;
    Ok(())
}

/// `[ALL ABOARD]` / `[ALL STORED]`: every live crate of the load.
pub async fn set_load_where(
    pool: &SqlitePool,
    load_id: i64,
    whereabouts: Whereabouts,
    stored_at: Option<&str>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    set_where_in(&mut tx, "load_id = ?", load_id, whereabouts, stored_at).await?;
    tx.commit().await?;
    Ok(())
}

/// `[WRITE OFF]`: every crate still in play on the load is lost, so the
/// load leaves Open Cargo without a sale. Returns how many crates went.
pub async fn write_off_load(pool: &SqlitePool, load_id: i64) -> Result<u64> {
    let done = sqlx::query(
        "UPDATE crates SET condition = 'lost' WHERE load_id = ? AND condition IN ('sound', 'glitched')",
    )
    .bind(load_id)
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Records a settled sale. When every sellable crate sold the load itself
/// closes; otherwise the sold crates move to a new load row on the same
/// voyage (the "split", RIPAE-0.2.md §4) and the original keeps the rest.
/// Returns the id of the sold load row.
pub async fn close_load(
    pool: &SqlitePool,
    load_id: i64,
    seller: i64,
    sale: &Sale,
    sold_at: &str,
    sell_terminal: &str,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    for id in &sale.written_off {
        sqlx::query("UPDATE crates SET condition = 'lost' WHERE id = ? AND condition = 'glitched'")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    let sold_id = if sale.closes_load() {
        sqlx::query(
            "UPDATE loads SET submitted_by = ?, sold_at = ?, sell_terminal = ?, sell_price_scu = ?,
                sell_price_lot = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(seller)
        .bind(sold_at)
        .bind(sell_terminal)
        .bind(sale.sell_price_scu)
        .bind(sale.sell_price_lot)
        .bind(load_id)
        .execute(&mut *tx)
        .await?;
        load_id
    } else {
        let row = sqlx::query(
            "INSERT INTO loads (voyage_id, submitted_by, bought_by, kind, commodity, buy_terminal, price_mode,
                buy_price_scu, buy_price_lot, stop_no, destination, reward, notes,
                sold_at, sell_terminal, sell_price_scu, sell_price_lot)
             SELECT voyage_id, ?, COALESCE(bought_by, submitted_by), kind, commodity, buy_terminal, price_mode,
                buy_price_scu, buy_price_lot, stop_no, destination, reward, notes, ?, ?, ?, ?
             FROM loads WHERE id = ? RETURNING id",
        )
        .bind(seller)
        .bind(sold_at)
        .bind(sell_terminal)
        .bind(sale.sell_price_scu)
        .bind(sale.sell_price_lot)
        .bind(load_id)
        .fetch_one(&mut *tx)
        .await?;
        let new_id: i64 = row.try_get("id")?;
        // the lot price on the kept part no longer describes it: recompute
        let kept_scu: u32 = sale.kept.iter().map(|c| c.size_scu).sum();
        sqlx::query(
            "UPDATE loads SET buy_price_lot = buy_price_scu * ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(kept_scu as f64)
        .bind(load_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE loads SET buy_price_lot = buy_price_scu * ? WHERE id = ?")
            .bind(sale.scu_sold as f64)
            .bind(new_id)
            .execute(&mut *tx)
            .await?;
        new_id
    };
    for c in &sale.sold {
        sqlx::query(
            "UPDATE crates SET load_id = ?, condition = 'sold', sold_at = ? WHERE id = ?",
        )
        .bind(sold_id)
        .bind(sold_at)
        .bind(c.id)
        .execute(&mut *tx)
        .await?;
    }
    for s in &sale.shares {
        sqlx::query(
            "INSERT INTO shares (load_id, to_user, paid_to, kind, amount, reason, paid_auec)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(sold_id)
        .bind(s.to_user)
        .bind(s.paid_to)
        .bind(s.kind.as_str())
        .bind(s.amount)
        .bind(s.reason.as_deref())
        .bind(s.paid)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(v) = sqlx::query_scalar::<_, i64>("SELECT voyage_id FROM loads WHERE id = ?").bind(sold_id).fetch_optional(&mut *tx).await? {
        sync_crew(&mut tx, v).await?;
    }
    tx.commit().await?;
    Ok(sold_id)
}

/// Corrects a recorded sale in place: seller, date, terminal, price, and the
/// shares (replaced wholesale, paid on the corrected profit). The crates
/// that sold stay as they are.
pub async fn update_sale(
    pool: &SqlitePool,
    load_id: i64,
    seller: i64,
    buyer: i64,
    sold_at: &str,
    sell_terminal: &str,
    sale: &Resale,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let done = sqlx::query(
        "UPDATE loads SET submitted_by = ?, bought_by = ?, sold_at = ?, sell_terminal = ?, sell_price_scu = ?,
            sell_price_lot = ?, updated_at = datetime('now') WHERE id = ? AND sold_at IS NOT NULL",
    )
    .bind(seller)
    .bind(buyer)
    .bind(sold_at)
    .bind(sell_terminal)
    .bind(sale.sell_price_scu)
    .bind(sale.sell_price_lot)
    .bind(load_id)
    .execute(&mut *tx)
    .await?;
    if done.rows_affected() == 0 {
        return Err(anyhow!("no such sale"));
    }
    sqlx::query("DELETE FROM shares WHERE load_id = ?").bind(load_id).execute(&mut *tx).await?;
    for s in &sale.shares {
        sqlx::query(
            "INSERT INTO shares (load_id, to_user, paid_to, kind, amount, reason, paid_auec)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(load_id)
        .bind(s.to_user)
        .bind(s.paid_to)
        .bind(s.kind.as_str())
        .bind(s.amount)
        .bind(s.reason.as_deref())
        .bind(s.paid)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(v) = sqlx::query_scalar::<_, i64>("SELECT voyage_id FROM loads WHERE id = ?").bind(load_id).fetch_optional(&mut *tx).await? {
        sync_crew(&mut tx, v).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Deletes a load (crates and shares cascade). A voyage left empty goes too.
pub async fn delete_load(pool: &SqlitePool, id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    let voyage_id: Option<i64> = sqlx::query_scalar("SELECT voyage_id FROM loads WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM loads WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if let Some(v) = voyage_id {
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM loads WHERE voyage_id = ?")
            .bind(v)
            .fetch_one(&mut *tx)
            .await?;
        if left == 0 {
            sqlx::query("DELETE FROM voyages WHERE id = ?")
                .bind(v)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Deletes a voyage and everything on it.
pub async fn delete_voyage(pool: &SqlitePool, id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM loads WHERE voyage_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM voyages WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::stowage::model::{board_credit, cash, settle, LoadStatus, SaleInput, ShareInput, ShareKind};

    async fn test_pool() -> (SqlitePool, i64, i64) {
        let pool = db::connect(":memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        let gig: i64 = sqlx::query_scalar(
            "INSERT INTO users (handle, display_name, password_hash) VALUES ('lordgig', 'Lord Gig', 'x') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let jt: i64 = sqlx::query_scalar(
            "INSERT INTO users (handle, display_name, password_hash) VALUES ('firstofficer', 'JT', 'x') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        (pool, gig, jt)
    }

    fn new_load(commodity: &str, boxes: BoxMix, dest: Option<&str>) -> NewLoad {
        let scu: u32 = boxes.iter().map(|(n, s)| n * s).sum();
        NewLoad {
            kind: LoadKind::Trade,
            commodity: commodity.into(),
            buy_terminal: "ArcCorp Mining Area 141".into(),
            price_mode: PriceMode::Scu,
            buy_price_scu: 10.0,
            buy_price_lot: 10.0 * scu as f64,
            stop_no: 1,
            destination: dest.map(Into::into),
            boxes,
            reward: None,
            whereabouts: Whereabouts::Aboard,
            stored_at: None,
            notes: None,
        }
    }

    fn new_voyage(ship_id: Option<i64>) -> NewVoyage {
        NewVoyage { ship_id, bought_at: "2026-11-08".into(), notes: None }
    }

    #[tokio::test]
    async fn free_load_round_trips_without_a_buy_terminal() {
        let (pool, gig, _) = test_pool().await;
        let mut free = new_load("Scrap", vec![(1, 8)], None);
        free.buy_terminal = String::new();
        free.buy_price_scu = 0.0;
        free.buy_price_lot = 0.0;
        let id = create_voyage(&pool, gig, gig, &new_voyage(None), &[free]).await.unwrap();
        let v = find_voyage(&pool, id).await.unwrap().unwrap();
        assert_eq!(v.loads[0].buy_terminal, "");
        assert_eq!(v.loads[0].buy_price_scu, 0.0);
    }

    #[tokio::test]
    async fn voyage_load_crate_round_trip() {
        let (pool, gig, _) = test_pool().await;
        let hull = find_ship_by_name(&pool, "hull-c").await.unwrap().unwrap();
        assert_eq!(hull.name, "Hull-C");
        assert_eq!(hull.sc_cargo_slug, "hull-c");

        let id = create_voyage(
            &pool,
            gig,
            gig,
            &new_voyage(Some(hull.id)),
            &[
                new_load("Laranite", vec![(2, 32), (1, 8)], Some("Area18 TDD")),
                new_load("Agricium", vec![(4, 8)], None),
            ],
        )
        .await
        .unwrap();
        let v = find_voyage(&pool, id).await.unwrap().unwrap();
        assert_eq!(v.logged_by_name, "Lord Gig");
        assert_eq!(v.ship.as_ref().map(|s| s.name.as_str()), Some("Hull-C"));
        assert_eq!(v.hull_label().as_deref(), Some("Lord Gig's Hull-C"));
        assert_eq!(v.loads.len(), 2);
        let laranite = &v.loads[0];
        assert_eq!(laranite.commodity, "Laranite");
        assert_eq!(laranite.crates.len(), 3);
        assert_eq!(laranite.scu(), 72);
        assert_eq!(laranite.boxes(), vec![(2, 32), (1, 8)]);
        assert_eq!(laranite.buy_price_lot, Some(720.0));
        assert_eq!(laranite.status(), LoadStatus::InTransit);
        assert_eq!(laranite.where_now(v.hull_label().as_deref()), "aboard Lord Gig's Hull-C");

        let all = list_voyages(&pool).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].loads[1].crates.len(), 4);
        assert_eq!(latest_ship_name(&pool).await.unwrap().as_deref(), Some("Hull-C"));
    }

    #[tokio::test]
    async fn derived_status_survives_a_reload() {
        let (pool, gig, _) = test_pool().await;
        let v = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Laranite", vec![(3, 8)], None)])
            .await
            .unwrap();
        let load = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        let [a, b, c] = [load.crates[0].id, load.crates[1].id, load.crates[2].id];

        set_crate_where(&pool, a, Whereabouts::Stored, Some("Everus Harbor")).await.unwrap();
        let l = find_load(&pool, load.id).await.unwrap().unwrap();
        assert_eq!(l.status(), LoadStatus::Split);
        assert_eq!(l.where_now(None), "2 aboard · no hull yet · 1 stored at Everus Harbor");

        set_crate_condition(&pool, b, Condition::Lost).await.unwrap();
        set_crate_condition(&pool, c, Condition::Glitched).await.unwrap();
        set_load_where(&pool, load.id, Whereabouts::Stored, Some("Everus Harbor")).await.unwrap();
        let l = find_load(&pool, load.id).await.unwrap().unwrap();
        assert_eq!(l.status(), LoadStatus::Stored);
        assert_eq!(l.scu(), 16);
        assert_eq!(l.written_off(), 80.0);
        assert_eq!(l.glitched_count(), 1);
        // the lost crate kept its place; all-stored only moves live crates
        assert_eq!(l.crates.iter().find(|c| c.id == b).unwrap().whereabouts, Whereabouts::Aboard);

        set_load_where(&pool, load.id, Whereabouts::Aboard, None).await.unwrap();
        let l = find_load(&pool, load.id).await.unwrap().unwrap();
        assert_eq!(l.status(), LoadStatus::InTransit);
        assert!(l.crates.iter().all(|c| c.stored_at.is_none()));
    }

    #[tokio::test]
    async fn move_load_between_voyages_and_remove_the_emptied_one() {
        let (pool, gig, jt) = test_pool().await;
        let a = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Laranite", vec![(1, 32)], None)])
            .await
            .unwrap();
        let b = create_voyage(
            &pool,
            jt,
            jt,
            &new_voyage(None),
            &[new_load("Agricium", vec![(1, 32)], None), new_load("Titanium", vec![(2, 8)], None)],
        )
        .await
        .unwrap();
        let titanium = find_voyage(&pool, b).await.unwrap().unwrap().loads[1].id;

        // merge Titanium into voyage A
        assert_eq!(move_load(&pool, titanium, MoveTarget::Voyage(a), gig).await.unwrap(), a);
        assert_eq!(find_voyage(&pool, a).await.unwrap().unwrap().loads.len(), 2);
        assert_eq!(find_voyage(&pool, b).await.unwrap().unwrap().loads.len(), 1);

        // detach it again into a voyage of its own
        let c = move_load(&pool, titanium, MoveTarget::New, gig).await.unwrap();
        assert_ne!(c, a);
        let vc = find_voyage(&pool, c).await.unwrap().unwrap();
        assert_eq!(vc.loads.len(), 1);
        assert_eq!(vc.logged_by, gig);
        assert_eq!(vc.bought_at, "2026-11-08");

        // moving B's last load empties B, which goes away
        let agricium = find_voyage(&pool, b).await.unwrap().unwrap().loads[0].id;
        move_load(&pool, agricium, MoveTarget::Voyage(a), gig).await.unwrap();
        assert!(find_voyage(&pool, b).await.unwrap().is_none());
        assert_eq!(list_voyages(&pool).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn cascade_on_voyage_delete() {
        let (pool, gig, jt) = test_pool().await;
        let v = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Laranite", vec![(2, 32)], None)])
            .await
            .unwrap();
        let load = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        let sale = settle(
            &load,
            &SaleInput {
                crate_ids: vec![load.crates[0].id],
                price_mode: PriceMode::Scu,
                price: 20.0,
                shares: vec![ShareInput { to_user: jt, paid_to: None, kind: ShareKind::Pct, amount: 50.0, reason: None }],
            },
        )
        .unwrap();
        close_load(&pool, load.id, gig, &sale, "2026-11-09", "Area18 TDD").await.unwrap();
        let count = |table: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                    .fetch_one(&pool)
                    .await
                    .unwrap()
            }
        };
        assert_eq!(count("loads").await, 2);
        assert_eq!(count("crates").await, 2);
        assert_eq!(count("shares").await, 1);

        delete_voyage(&pool, v).await.unwrap();
        assert_eq!(count("voyages").await, 0);
        assert_eq!(count("loads").await, 0);
        assert_eq!(count("crates").await, 0);
        assert_eq!(count("shares").await, 0);
    }

    #[tokio::test]
    async fn partial_close_splits_and_full_close_closes_in_place() {
        let (pool, gig, jt) = test_pool().await;
        let v = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Laranite", vec![(2, 32), (1, 8)], None)])
            .await
            .unwrap();
        let load = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        let first = load.crates[0].id;
        let sale = settle(
            &load,
            &SaleInput {
                crate_ids: vec![first],
                price_mode: PriceMode::Lot,
                price: 800.0, // 25/SCU on 32 SCU, cost 320 -> profit 480
                shares: vec![ShareInput { to_user: jt, paid_to: Some(gig), kind: ShareKind::Pct, amount: 25.0, reason: Some("escort".into()) }],
            },
        )
        .unwrap();
        let sold_id = close_load(&pool, load.id, jt, &sale, "2026-11-09", "Area18 TDD").await.unwrap();
        assert_ne!(sold_id, load.id, "a partial close splits off a new row");

        let v = find_voyage(&pool, v).await.unwrap().unwrap();
        assert_eq!(v.loads.len(), 2);
        let kept = v.loads.iter().find(|l| l.id == load.id).unwrap();
        let sold = v.loads.iter().find(|l| l.id == sold_id).unwrap();
        assert_eq!(kept.status(), LoadStatus::InTransit);
        assert_eq!(kept.scu(), 40);
        assert_eq!(kept.buy_price_lot, Some(400.0));
        assert_eq!(sold.status(), LoadStatus::Sold);
        assert_eq!(sold.submitted_by_name, "JT");
        assert_eq!(sold.scu_sold(), 32);
        assert_eq!(sold.sell_price_scu, Some(25.0));
        assert_eq!(sold.sell_price_lot, Some(800.0));
        assert_eq!(sold.profit(), Some(480.0));
        assert_eq!(sold.shares.len(), 1);
        assert_eq!(sold.shares[0].paid_auec, 120.0);
        assert_eq!(sold.shares[0].to_user_name, "JT");
        assert_eq!(sold.shares[0].paid_to_name, "Lord Gig");
        assert_eq!(sold.seller_keeps(), Some(360.0));
        assert_eq!(latest_sell_terminal(&pool).await.unwrap().as_deref(), Some("Area18 TDD"));

        // now sell the rest in place
        let rest = settle(
            kept,
            &SaleInput { crate_ids: kept.crates.iter().map(|c| c.id).collect(), price_mode: PriceMode::Scu, price: 5.0, shares: vec![] },
        )
        .unwrap();
        let closed = close_load(&pool, kept.id, gig, &rest, "2026-11-10", "Grim HEX").await.unwrap();
        assert_eq!(closed, kept.id);
        let v = find_voyage(&pool, v.id).await.unwrap().unwrap();
        assert!(v.is_fully_sold());
        assert_eq!(v.loads.iter().find(|l| l.id == kept.id).unwrap().profit(), Some(-200.0));
    }

    #[tokio::test]
    async fn a_run_kept_for_a_crewmate_is_theirs_to_buy_and_sell() {
        // Gig does the bookkeeping; the voyage, its load and the sale are JT's.
        let (pool, gig, jt) = test_pool().await;
        let v = create_voyage(&pool, jt, gig, &new_voyage(None), &[new_load("Laranite", vec![(1, 32)], None)])
            .await
            .unwrap();
        let voyage = find_voyage(&pool, v).await.unwrap().unwrap();
        assert_eq!(voyage.logged_by, jt);
        assert_eq!(voyage.entered_by, gig, "Gig typed it in, so Gig logged it");
        assert_eq!(voyage.entered_by_name, "Lord Gig");
        let load = &voyage.loads[0];
        assert_eq!(load.submitted_by, jt);
        let sale = settle(
            load,
            &SaleInput { crate_ids: vec![load.crates[0].id], price_mode: PriceMode::Scu, price: 20.0, shares: vec![] },
        )
        .unwrap();
        close_load(&pool, load.id, jt, &sale, "2026-11-09", "Area18 TDD").await.unwrap();
        let all = list_voyages(&pool).await.unwrap();
        assert_eq!(crate::stowage::model::board_credit(jt, &all), 320.0);
        assert_eq!(crate::stowage::model::board_credit(gig, &all), 0.0);
    }

    #[tokio::test]
    async fn delete_load_removes_an_emptied_voyage() {
        let (pool, gig, _) = test_pool().await;
        let v = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Laranite", vec![(1, 8)], None)])
            .await
            .unwrap();
        let load = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        delete_load(&pool, load.id).await.unwrap();
        assert!(find_voyage(&pool, v).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn edit_load_moves_live_crates_when_asked() {
        let (pool, gig, _) = test_pool().await;
        let v = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Laranite", vec![(2, 8)], None)])
            .await
            .unwrap();
        let load = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        update_load(
            &pool,
            load.id,
            &EditLoad {
                kind: LoadKind::Trade,
                reward: None,
                commodity: "Titanium".into(),
                buy_terminal: "Shubin Mining Facility SCD-1".into(),
                price_mode: PriceMode::Lot,
                buy_price_scu: 12.5,
                buy_price_lot: 200.0,
                stop_no: 3,
                destination: Some("Lorville TDD".into()),
                whereabouts: Some(Whereabouts::AwaitingPickup),
                stored_at: None,
                notes: Some("freight elevator 2".into()),
                bought_by: gig,
            },
        )
        .await
        .unwrap();
        let l = find_load(&pool, load.id).await.unwrap().unwrap();
        assert_eq!(l.commodity, "Titanium");
        assert_eq!(l.stop_no, 3);
        assert_eq!(l.price_mode, PriceMode::Lot);
        assert_eq!(l.status(), LoadStatus::AwaitingPickup);
        assert_eq!(l.notes.as_deref(), Some("freight elevator 2"));
    }

    #[tokio::test]
    async fn editing_a_voyage_corrects_who_logged_it() {
        // A run recorded before entered_by existed shows its owner; the edit fixes it.
        let (pool, gig, jt) = test_pool().await;
        let v = create_voyage(&pool, jt, jt, &new_voyage(None), &[new_load("Laranite", vec![(1, 8)], None)])
            .await
            .unwrap();
        let edit = EditVoyage { ship_id: None, bought_at: "2026-11-08".into(), notes: None, logged_by: jt, move_loads: false, entered_by: gig, crew: None };
        update_voyage(&pool, v, &edit).await.unwrap();
        let voyage = find_voyage(&pool, v).await.unwrap().unwrap();
        assert_eq!((voyage.logged_by, voyage.entered_by), (jt, gig));
        assert_eq!(voyage.loads[0].submitted_by, jt);
    }

    #[tokio::test]
    async fn changing_a_voyages_owner_moves_their_unsold_loads_only() {
        let (pool, gig, jt) = test_pool().await;
        let v = create_voyage(
            &pool,
            gig,
            gig,
            &new_voyage(None),
            &[new_load("Laranite", vec![(1, 8)], None), new_load("Agricium", vec![(1, 8)], None)],
        )
        .await
        .unwrap();
        let sold = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        let sale = settle(
            &sold,
            &SaleInput { crate_ids: sold.crates.iter().map(|c| c.id).collect(), price_mode: PriceMode::Scu, price: 20.0, shares: vec![] },
        )
        .unwrap();
        close_load(&pool, sold.id, gig, &sale, "2026-11-09", "Area18 TDD").await.unwrap();

        // by default only the run changes hands
        let edit = EditVoyage { ship_id: None, bought_at: "2026-11-08".into(), notes: None, logged_by: jt, move_loads: false, entered_by: gig, crew: None };
        update_voyage(&pool, v, &edit).await.unwrap();
        let voyage = find_voyage(&pool, v).await.unwrap().unwrap();
        assert_eq!((voyage.logged_by, voyage.logged_by_name.as_str(), voyage.entered_by), (jt, "JT", gig));
        let open = voyage.loads.iter().find(|l| l.commodity == "Agricium").unwrap();
        assert_eq!((open.submitted_by, open.bought_by), (gig, gig), "loads stay put unless asked");

        // asked to: the old owner's open loads follow, the sold one keeps its seller and buyer
        update_voyage(&pool, v, &EditVoyage { logged_by: gig, ..edit.clone() }).await.unwrap();
        update_voyage(&pool, v, &EditVoyage { move_loads: true, ..edit }).await.unwrap();
        let voyage = find_voyage(&pool, v).await.unwrap().unwrap();
        let by = |c: &str| voyage.loads.iter().find(|l| l.commodity == c).map(|l| (l.submitted_by, l.bought_by)).unwrap();
        assert_eq!(by("Laranite"), (gig, gig), "a sold row is left alone");
        assert_eq!(by("Agricium"), (jt, jt), "an open load follows the new owner");
    }

    #[tokio::test]
    async fn a_split_sale_remembers_who_bought_the_load() {
        let (pool, gig, jt) = test_pool().await;
        let v = create_voyage(&pool, gig, gig, &new_voyage(None), &[]).await.unwrap();
        let id = add_load(&pool, v, jt, &new_load("Laranite", vec![(2, 8)], None)).await.unwrap();
        let load = find_load(&pool, id).await.unwrap().unwrap();
        let input = SaleInput { crate_ids: vec![load.crates[0].id], price_mode: PriceMode::Scu, price: 20.0, shares: vec![] };
        let sold_id = close_load(&pool, id, gig, &settle(&load, &input).unwrap(), "2026-11-09", "Area18 TDD").await.unwrap();
        assert_ne!(sold_id, id);
        let sold = find_load(&pool, sold_id).await.unwrap().unwrap();
        assert_eq!((sold.submitted_by, sold.bought_by, sold.bought_by_name.as_str()), (gig, jt, "JT"));
    }

    #[tokio::test]
    async fn a_close_that_writes_off_the_glitched_crate_closes_the_load_whole() {
        let (pool, gig, _) = test_pool().await;
        let v = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Laranite", vec![(2, 8)], None)])
            .await
            .unwrap();
        let mut load = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        let (good, bad) = (load.crates[0].id, load.crates[1].id);
        set_crate_condition(&pool, bad, Condition::Glitched).await.unwrap();
        load.crates[1].condition = Condition::Glitched;
        let input = SaleInput { crate_ids: vec![good], price_mode: PriceMode::Scu, price: 20.0, shares: vec![] };
        let written_off = load.write_off_glitched(&input.crate_ids);
        assert_eq!(written_off, vec![bad]);
        let sale = Sale { written_off, ..settle(&load, &input).unwrap() };
        assert!(sale.closes_load());
        let sold = close_load(&pool, load.id, gig, &sale, "2026-11-09", "Area18 TDD").await.unwrap();
        assert_eq!(sold, load.id, "nothing left open, so no split");
        let v = find_voyage(&pool, v).await.unwrap().unwrap();
        let l = &v.loads[0];
        assert!(v.is_fully_sold() && l.was_sold());
        assert!(l.crates.iter().find(|c| c.id == bad).unwrap().is_lost());
        assert_eq!(l.profit(), Some(80.0), "the lost crate isn't charged to the sale");
    }

    #[tokio::test]
    async fn writing_off_a_load_takes_it_out_of_open_cargo_but_not_into_the_ledger() {
        let (pool, gig, _) = test_pool().await;
        let v = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Laranite", vec![(1, 8), (1, 4)], None)])
            .await
            .unwrap();
        let load = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        set_crate_condition(&pool, load.crates[0].id, Condition::Glitched).await.unwrap();
        assert_eq!(write_off_load(&pool, load.id).await.unwrap(), 2);
        let v = find_voyage(&pool, v).await.unwrap().unwrap();
        let l = &v.loads[0];
        assert!(!l.is_open() && !l.was_sold());
        assert!(l.crates.iter().all(|c| c.is_lost()));
        assert_eq!(v.open_loads().count(), 0);
        assert!(crate::stowage::views::ledger_rows(std::slice::from_ref(&v)).is_empty());
    }

    #[tokio::test]
    async fn update_sale_reprices_in_place_and_replaces_the_shares() {
        let (pool, gig, jt) = test_pool().await;
        let v = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Laranite", vec![(2, 8)], None)])
            .await
            .unwrap();
        let load = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        let pct = |amount| ShareInput { to_user: jt, paid_to: None, kind: ShareKind::Pct, amount, reason: None };
        let input = SaleInput { crate_ids: load.crates.iter().map(|c| c.id).collect(), price_mode: PriceMode::Scu, price: 15.0, shares: vec![pct(25.0)] };
        close_load(&pool, load.id, gig, &settle(&load, &input).unwrap(), "2026-11-09", "Area18 TDD").await.unwrap();
        let sold = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        assert_eq!(sold.profit(), Some(80.0));

        // the terminal showed the wrong price: it was 3,000 for the lot, sold by JT, Gig on 50 %
        let half = ShareInput { to_user: gig, ..pct(50.0) };
        let resale = crate::stowage::model::resettle(&sold, PriceMode::Lot, 3000.0, &[half]).unwrap();
        update_sale(&pool, sold.id, jt, jt, "2026-11-10", "Lorville TDD", &resale).await.unwrap();
        let l = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        assert_eq!(l.id, sold.id);
        assert_eq!((l.submitted_by, l.sold_at.as_deref(), l.sell_terminal.as_deref()), (jt, Some("2026-11-10"), Some("Lorville TDD")));
        assert_eq!((l.bought_by, l.bought_by_name.as_str()), (jt, "JT"), "the buyer is corrected too");
        assert_eq!(l.profit(), Some(2840.0));
        assert_eq!(l.shares.len(), 1, "the old share is replaced, not added to");
        assert_eq!((l.shares[0].to_user, l.shares[0].paid_auec), (gig, 1420.0));
        assert_eq!(l.scu_sold(), 16, "the crates that sold don't change");
        // an open load isn't a sale to edit
        let open = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Agricium", vec![(1, 8)], None)]).await.unwrap();
        let open = find_voyage(&pool, open).await.unwrap().unwrap().loads.remove(0);
        assert!(update_sale(&pool, open.id, gig, gig, "2026-11-10", "x", &resale).await.is_err());
    }

    #[tokio::test]
    async fn a_haul_rides_with_bought_cargo_and_its_payout_is_all_profit() {
        let (pool, gig, _) = test_pool().await;
        let mut haul = new_load("Processed Food", vec![(2, 8), (1, 4)], Some("Area18 TDD"));
        haul.kind = LoadKind::Haul;
        haul.buy_terminal = "Baijini Point".into();
        haul.price_mode = PriceMode::Lot;
        haul.buy_price_scu = 0.0;
        haul.buy_price_lot = 0.0;
        haul.reward = Some(45_000.0);
        let v = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Laranite", vec![(1, 8)], None), haul])
            .await
            .unwrap();
        let voyage = find_voyage(&pool, v).await.unwrap().unwrap();
        let (trade, haul) = (&voyage.loads[0], &voyage.loads[1]);
        assert_eq!(trade.kind, LoadKind::Trade);
        assert_eq!(haul.kind, LoadKind::Haul);
        assert_eq!(haul.reward, Some(45_000.0));
        assert_eq!(haul.cost(), 0.0);
        // the promised reward isn't counted until the delivery is recorded
        let t = crate::stowage::model::Totals::from_voyages([&voyage]);
        assert_eq!((t.committed, t.realised, t.open_loads), (80.0, 0.0, 2));

        // deliver part of it: the delivered row keeps the kind and reward
        let first = haul.crates[0].id;
        let sale = settle(haul, &SaleInput { crate_ids: vec![first], price_mode: PriceMode::Lot, price: 20_000.0, shares: vec![] })
            .unwrap();
        let sold_id = close_load(&pool, haul.id, gig, &sale, "2026-11-09", "Area18 TDD").await.unwrap();
        let delivered = find_load(&pool, sold_id).await.unwrap().unwrap();
        assert_eq!(delivered.kind, LoadKind::Haul);
        assert_eq!(delivered.reward, Some(45_000.0));
        assert_eq!(delivered.profit(), Some(20_000.0));
        // losing the rest of the contract's boxes costs nothing
        write_off_load(&pool, haul.id).await.unwrap();
        let rest = find_load(&pool, haul.id).await.unwrap().unwrap();
        assert!(!rest.is_open());
        assert_eq!(rest.written_off(), 0.0);
    }

    fn terms(user_id: i64, pct: f64, flat: f64, paid_to: Option<i64>, reason: Option<&str>) -> CrewTerms {
        CrewTerms { user_id, pct, flat, paid_to, reason: reason.map(Into::into) }
    }

    fn crew_of(v: &Voyage) -> Vec<CrewTerms> {
        v.crew.iter().map(|c| c.terms.clone()).collect()
    }

    #[tokio::test]
    async fn voyage_crew_round_trips_follows_a_detach_and_goes_with_the_voyage() {
        let (pool, gig, jt) = test_pool().await;
        let v = create_voyage(
            &pool,
            gig,
            gig,
            &new_voyage(None),
            &[new_load("Laranite", vec![(1, 8)], None), new_load("Agricium", vec![(1, 8)], None)],
        )
        .await
        .unwrap();
        // a new run's crew is its owner
        assert_eq!(crew_of(&find_voyage(&pool, v).await.unwrap().unwrap()), vec![CrewTerms::member(gig)]);

        let split = vec![terms(gig, 0.0, 0.0, None, None), terms(jt, 50.0, 1000.0, Some(gig), Some("crew"))];
        let edit = EditVoyage { ship_id: None, bought_at: "2026-11-08".into(), notes: None, logged_by: gig, move_loads: false, entered_by: gig, crew: Some(split.clone()) };
        update_voyage(&pool, v, &edit).await.unwrap();
        let saved = find_voyage(&pool, v).await.unwrap().unwrap();
        assert_eq!(crew_of(&saved), split);
        assert_eq!(saved.crew[1].name, "JT");
        assert_eq!(saved.crew[1].paid_to_name.as_deref(), Some("Lord Gig"));
        assert_eq!(voyage_crew(&pool, v).await.unwrap(), saved.crew);

        // a detached load's new voyage starts with the same crew and split
        let second = saved.loads[1].id;
        let detached = move_load(&pool, second, MoveTarget::New, gig).await.unwrap();
        assert_eq!(crew_of(&find_voyage(&pool, detached).await.unwrap().unwrap()), split);

        // saving again replaces the crew wholesale -- but the owner stays aboard
        update_voyage(&pool, v, &EditVoyage { crew: Some(vec![]), ..edit.clone() }).await.unwrap();
        assert_eq!(crew_of(&find_voyage(&pool, v).await.unwrap().unwrap()), vec![CrewTerms::member(gig)]);
        // and `None` leaves it alone
        update_voyage(&pool, v, &EditVoyage { crew: Some(split.clone()), ..edit.clone() }).await.unwrap();
        update_voyage(&pool, v, &EditVoyage { crew: None, ..edit }).await.unwrap();
        assert_eq!(crew_of(&find_voyage(&pool, v).await.unwrap().unwrap()), split);

        delete_voyage(&pool, detached).await.unwrap();
        delete_voyage(&pool, v).await.unwrap();
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM voyage_crew").fetch_one(&pool).await.unwrap();
        assert_eq!(left, 0, "a voyage's crew goes with it");
    }

    #[tokio::test]
    async fn sellers_buyers_and_merged_holders_join_the_crew() {
        let (pool, gig, jt) = test_pool().await;
        let v = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Laranite", vec![(2, 8)], None)]).await.unwrap();
        let load = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        // JT sells one crate of Gig's load
        let input = SaleInput { crate_ids: vec![load.crates[0].id], price_mode: PriceMode::Scu, price: 20.0, shares: vec![] };
        close_load(&pool, load.id, jt, &settle(&load, &input).unwrap(), "2026-11-09", "Area18 TDD").await.unwrap();
        let ids = |v: &Voyage| v.crew.iter().map(|c| c.terms.user_id).collect::<Vec<_>>();
        assert_eq!(ids(&find_voyage(&pool, v).await.unwrap().unwrap()), vec![gig, jt]);

        // JT's own run, merged into Gig's: JT's load brings JT along
        let w = create_voyage(&pool, jt, jt, &new_voyage(None), &[new_load("Gold", vec![(1, 8)], None)]).await.unwrap();
        let other = create_voyage(&pool, gig, gig, &new_voyage(None), &[new_load("Tin", vec![(1, 8)], None)]).await.unwrap();
        let jt_load = find_voyage(&pool, w).await.unwrap().unwrap().loads[0].id;
        move_load(&pool, jt_load, MoveTarget::Voyage(other), gig).await.unwrap();
        assert_eq!(ids(&find_voyage(&pool, other).await.unwrap().unwrap()), vec![gig, jt]);
    }

    /// Migration 0012 on a database from before it: every settled share,
    /// and so every member's credit and cash, is untouched; every share
    /// holder and every owner, buyer and seller is on the crew, on the
    /// terms they had.
    #[tokio::test]
    async fn migration_0012_keeps_every_figure_and_moves_shares_to_the_crew() {
        let pool = db::connect(":memory:").await.unwrap();
        let migrations = sqlx::migrate!("./migrations");
        for m in migrations.iter().filter(|m| m.version < 12) {
            sqlx::raw_sql(&m.sql).execute(&pool).await.unwrap();
        }
        sqlx::raw_sql(
            "INSERT INTO users (id, handle, display_name, password_hash) VALUES
               (1, 'lordgig', 'Lord Gig', 'x'), (2, 'firstofficer', 'JT', 'x'), (3, 'crew03', 'Crew-03', 'x');
             -- Voyage 1, Gig's: a voyage-wide split (JT 25 % forfeited to Gig, Crew-03 a 1,000 flat
             -- fee), one load sold on it, one still open with a lost crate
             INSERT INTO voyages (id, logged_by, entered_by, bought_at) VALUES (1, 1, 1, '2026-11-01');
             INSERT INTO voyage_shares (voyage_id, to_user, paid_to, kind, amount, reason) VALUES
               (1, 2, 1, 'pct', 25, 'escort'), (1, 3, 3, 'flat', 1000, 'fuel');
             INSERT INTO loads (id, voyage_id, submitted_by, bought_by, commodity, buy_terminal, price_mode, buy_price_scu,
                 buy_price_lot, sold_at, sell_terminal, sell_price_scu, sell_price_lot) VALUES
               (1, 1, 1, 1, 'Laranite', 'ArcCorp Mining Area 141', 'scu', 10, 640, '2026-11-02', 'Area18 TDD', 30, 1920),
               (2, 1, 1, 1, 'Agricium', 'ArcCorp Mining Area 141', 'scu', 100, 1200, NULL, NULL, NULL, NULL);
             INSERT INTO crates (load_id, size_scu, condition, whereabouts, sold_at) VALUES
               (1, 32, 'sold', 'aboard', '2026-11-02'), (1, 32, 'sold', 'aboard', '2026-11-02'),
               (2, 8, 'sound', 'aboard', NULL), (2, 4, 'lost', 'aboard', NULL);
             INSERT INTO shares (load_id, to_user, paid_to, kind, amount, reason, paid_auec) VALUES
               (1, 2, 1, 'pct', 25, 'escort', 320), (1, 3, 3, 'flat', 1000, 'fuel', 1000);
             -- Voyage 2, JT's: no voyage-wide split; shares set per sale. JT sold the first load
             -- (Crew-03 on 10 %), Gig the later one (Crew-03 on 20 % forfeited to JT, JT on 30 %);
             -- a third load sold at a loss with a flat fee that paid nothing
             INSERT INTO voyages (id, logged_by, entered_by, bought_at) VALUES (2, 2, 1, '2026-11-03');
             INSERT INTO loads (id, voyage_id, submitted_by, bought_by, commodity, buy_terminal, price_mode, buy_price_scu,
                 buy_price_lot, sold_at, sell_terminal, sell_price_scu, sell_price_lot) VALUES
               (3, 2, 2, 2, 'Gold', 'Levski', 'scu', 5, 40, '2026-11-04', 'Grim HEX', 15, 120),
               (4, 2, 1, 2, 'Tin', 'Levski', 'lot', 2.5, 40, '2026-11-06', 'Grim HEX', 10, 160),
               (5, 2, 2, 2, 'Iron', 'Levski', 'scu', 9, 72, '2026-11-05', 'Grim HEX', 1, 8);
             INSERT INTO crates (load_id, size_scu, condition, whereabouts, sold_at) VALUES
               (3, 8, 'sold', 'aboard', '2026-11-04'), (4, 16, 'sold', 'aboard', '2026-11-06'),
               (5, 8, 'sold', 'aboard', '2026-11-05');
             INSERT INTO shares (load_id, to_user, paid_to, kind, amount, reason, paid_auec) VALUES
               (3, 3, 3, 'pct', 10, NULL, 8),
               (4, 3, 2, 'pct', 20, 'gunner', 24), (4, 2, 2, 'pct', 30, NULL, 36),
               (5, 1, 1, 'flat', 500, NULL, 0);
             -- Voyage 3, Crew-03's, nothing sold yet: JT listed twice on the voyage-wide split
             INSERT INTO voyages (id, logged_by, entered_by, bought_at) VALUES (3, 3, 3, '2026-11-07');
             INSERT INTO voyage_shares (voyage_id, to_user, paid_to, kind, amount, reason) VALUES
               (3, 2, 2, 'pct', 10, NULL), (3, 2, 2, 'flat', 50, NULL);
             INSERT INTO loads (id, voyage_id, submitted_by, bought_by, commodity, buy_terminal, price_mode, buy_price_scu,
                 buy_price_lot) VALUES (6, 3, 3, 3, 'Gold', 'Levski', 'scu', 5, 40);
             INSERT INTO crates (load_id, size_scu, condition, whereabouts) VALUES (6, 8, 'sound', 'aboard');",
        )
        .execute(&pool)
        .await
        .unwrap();

        let users = [1i64, 2, 3];
        let before = list_voyages_without_crew(&pool).await.unwrap();
        let figures = |vs: &[Voyage]| users.iter().map(|&u| (board_credit(u, vs), cash(u, vs))).collect::<Vec<_>>();
        let shares = |vs: &[Voyage]| vs.iter().flat_map(|v| v.loads.iter()).flat_map(|l| l.shares.clone()).collect::<Vec<_>>();
        // what a close on voyage 1's open load came up with before: the voyage split less the seller's own
        let old_close: Vec<ShareInput> = vec![
            ShareInput { to_user: 2, paid_to: Some(1), kind: ShareKind::Pct, amount: 25.0, reason: Some("escort".into()) },
            ShareInput { to_user: 3, paid_to: None, kind: ShareKind::Flat, amount: 1000.0, reason: Some("fuel".into()) },
        ];

        for m in migrations.iter().filter(|m| m.version >= 12) {
            sqlx::raw_sql(&m.sql).execute(&pool).await.unwrap();
        }
        let after = list_voyages(&pool).await.unwrap();
        assert_eq!(figures(&after), figures(&before), "credit and cash of every member are unchanged");
        assert_eq!(figures(&after), vec![(-380.0, -60.0), (364.0, 68.0), (1032.0, 1008.0)]);
        assert_eq!(shares(&after), shares(&before), "every settled share is untouched");
        let vshares: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name = 'voyage_shares'").fetch_one(&pool).await.unwrap();
        assert_eq!(vshares, 0, "voyage_shares is gone");

        let v1 = after.iter().find(|v| v.id == 1).unwrap();
        assert_eq!(
            crew_of(v1),
            vec![CrewTerms::member(1), terms(2, 25.0, 0.0, Some(1), Some("escort")), terms(3, 0.0, 1000.0, None, Some("fuel"))]
        );
        // ... and a close on it now pays exactly what it would have before
        let open = v1.loads.iter().find(|l| l.is_open()).unwrap();
        let sell = |shares: Vec<ShareInput>| {
            let input = SaleInput { crate_ids: vec![open.crates[0].id], price_mode: PriceMode::Scu, price: 500.0, shares };
            let sale = settle(open, &input).unwrap();
            let mut paid: Vec<(i64, i64, i64)> = sale.shares.iter().map(|s| (s.to_user, s.paid_to, (s.paid * 100.0).round() as i64)).collect();
            paid.sort_unstable();
            (paid, sale.seller_keeps)
        };
        assert_eq!(sell(v1.shares_for(1)), sell(old_close));

        // voyage 2 took the terms of each member's latest sale; its owner, sellers and payees are aboard
        let v2 = after.iter().find(|v| v.id == 2).unwrap();
        assert_eq!(
            crew_of(v2),
            vec![terms(2, 30.0, 0.0, None, None), terms(1, 0.0, 500.0, None, None), terms(3, 20.0, 0.0, Some(2), Some("gunner"))]
        );
        // a member listed twice keeps both parts of their split
        let v3 = after.iter().find(|v| v.id == 3).unwrap();
        assert_eq!(crew_of(v3), vec![CrewTerms::member(3), terms(2, 10.0, 50.0, None, None)]);
    }
}
