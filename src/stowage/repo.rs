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
    expand_boxes, BoxMix, Condition, Crate, Load, PriceMode, Sale, Share, Ship, User, Voyage,
    Whereabouts,
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
    pub commodity: String,
    pub buy_terminal: String,
    pub price_mode: PriceMode,
    pub buy_price_scu: f64,
    pub buy_price_lot: f64,
    pub stop_no: u32,
    pub destination: Option<String>,
    pub boxes: BoxMix,
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
}

#[derive(Debug, Clone)]
pub struct EditLoad {
    pub commodity: String,
    pub buy_terminal: String,
    pub price_mode: PriceMode,
    pub buy_price_scu: f64,
    pub buy_price_lot: f64,
    pub stop_no: u32,
    pub destination: Option<String>,
    /// `Some` moves every live crate there (the edit form's status field);
    /// `None` leaves the crates where they are (a split load).
    pub whereabouts: Option<Whereabouts>,
    pub stored_at: Option<String>,
    pub notes: Option<String>,
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
        ship: row_to_ship(row, "ship_")?,
        bought_at: row.try_get("bought_at")?,
        notes: row.try_get("notes")?,
        loads: Vec::new(),
    })
}

fn row_to_load(row: &SqliteRow) -> Result<Load> {
    let mode: String = row.try_get("price_mode")?;
    let stop_no: i64 = row.try_get("stop_no")?;
    Ok(Load {
        id: row.try_get("id")?,
        voyage_id: row.try_get("voyage_id")?,
        submitted_by: row.try_get("submitted_by")?,
        submitted_by_name: row.try_get("submitted_by_name")?,
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
    v.bought_at, v.notes,
    s.id AS ship_id, s.manufacturer AS ship_manufacturer, s.name AS ship_name, s.short AS ship_short,
    s.sc_cargo_slug AS ship_sc_cargo_slug, s.capacity_scu AS ship_capacity_scu, s.concept AS ship_concept
    FROM voyages v JOIN users u ON u.id = v.logged_by LEFT JOIN ships s ON s.id = v.ship_id";

const SELECT_LOAD: &str = "SELECT l.id, l.voyage_id, l.submitted_by, u.display_name AS submitted_by_name,
    l.commodity, l.buy_terminal, l.price_mode, l.buy_price_scu, l.buy_price_lot, l.stop_no,
    l.destination, l.sold_at, l.sell_terminal, l.sell_price_scu, l.sell_price_lot, l.notes
    FROM loads l JOIN users u ON u.id = l.submitted_by";

const SELECT_CRATE: &str =
    "SELECT id, load_id, size_scu, condition, whereabouts, stored_at, sold_at FROM crates";

const SELECT_SHARE: &str = "SELECT sh.id, sh.load_id, sh.to_user, tu.display_name AS to_user_name,
    sh.paid_to, pu.display_name AS paid_to_name, sh.kind, sh.amount, sh.reason, sh.paid_auec
    FROM shares sh JOIN users tu ON tu.id = sh.to_user JOIN users pu ON pu.id = sh.paid_to";

// ---------------------------------------------------------------------------
// reads
// ---------------------------------------------------------------------------

/// Every voyage with its loads, crates and shares. Voyages oldest first
/// (then by id); loads, crates and shares by id.
pub async fn list_voyages(pool: &SqlitePool) -> Result<Vec<Voyage>> {
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
    Ok(voyages.pop())
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
    let rows = sqlx::query("SELECT id, handle, display_name, created_at FROM users ORDER BY id")
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|r| {
            Ok(User {
                id: r.try_get("id")?,
                handle: r.try_get("handle")?,
                display_name: r.try_get("display_name")?,
                created_at: r.try_get("created_at")?,
            })
        })
        .collect()
}

pub async fn find_user(pool: &SqlitePool, id: i64) -> Result<Option<User>> {
    Ok(list_users(pool).await?.into_iter().find(|u| u.id == id))
}

pub async fn list_ships(pool: &SqlitePool) -> Result<Vec<Ship>> {
    let rows = sqlx::query(
        "SELECT id AS ship_id, manufacturer AS ship_manufacturer, name AS ship_name, short AS ship_short,
         sc_cargo_slug AS ship_sc_cargo_slug, capacity_scu AS ship_capacity_scu, concept AS ship_concept
         FROM ships ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|r| row_to_ship(r, "ship_").map(|s| s.expect("ships.id is NOT NULL")))
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
        "INSERT INTO loads (voyage_id, submitted_by, commodity, buy_terminal, price_mode,
            buy_price_scu, buy_price_lot, stop_no, destination, notes)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(voyage_id)
    .bind(submitted_by)
    .bind(&new.commodity)
    .bind(&new.buy_terminal)
    .bind(new.price_mode.as_str())
    .bind(new.buy_price_scu)
    .bind(new.buy_price_lot)
    .bind(new.stop_no as i64)
    .bind(new.destination.as_deref())
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
    voyage: &NewVoyage,
    loads: &[NewLoad],
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "INSERT INTO voyages (logged_by, ship_id, bought_at, notes) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(logged_by)
    .bind(voyage.ship_id)
    .bind(&voyage.bought_at)
    .bind(voyage.notes.as_deref())
    .fetch_one(&mut *tx)
    .await?;
    let voyage_id: i64 = row.try_get("id")?;
    for load in loads {
        insert_load(&mut tx, voyage_id, logged_by, load).await?;
    }
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
    sqlx::query(
        "UPDATE voyages SET ship_id = ?, bought_at = ?, notes = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(edit.ship_id)
    .bind(&edit.bought_at)
    .bind(edit.notes.as_deref())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_load(pool: &SqlitePool, id: i64, edit: &EditLoad) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE loads SET commodity = ?, buy_terminal = ?, price_mode = ?, buy_price_scu = ?,
            buy_price_lot = ?, stop_no = ?, destination = ?, notes = ?, updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(&edit.commodity)
    .bind(&edit.buy_terminal)
    .bind(edit.price_mode.as_str())
    .bind(edit.buy_price_scu)
    .bind(edit.buy_price_lot)
    .bind(edit.stop_no as i64)
    .bind(edit.destination.as_deref())
    .bind(edit.notes.as_deref())
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if let Some(w) = edit.whereabouts {
        set_where_in(&mut tx, "load_id = ?", id, w, edit.stored_at.as_deref()).await?;
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
                "INSERT INTO voyages (logged_by, ship_id, bought_at, notes)
                 SELECT ?, ship_id, bought_at, NULL FROM voyages WHERE id = ? RETURNING id",
            )
            .bind(actor)
            .bind(from)
            .fetch_one(&mut *tx)
            .await?;
            row.try_get("id")?
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
            "INSERT INTO loads (voyage_id, submitted_by, commodity, buy_terminal, price_mode,
                buy_price_scu, buy_price_lot, stop_no, destination, notes,
                sold_at, sell_terminal, sell_price_scu, sell_price_lot)
             SELECT voyage_id, ?, commodity, buy_terminal, price_mode,
                buy_price_scu, buy_price_lot, stop_no, destination, notes, ?, ?, ?, ?
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
    tx.commit().await?;
    Ok(sold_id)
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
    use crate::stowage::model::{settle, LoadStatus, SaleInput, ShareInput, ShareKind};

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
            commodity: commodity.into(),
            buy_terminal: "ArcCorp Mining Area 141".into(),
            price_mode: PriceMode::Scu,
            buy_price_scu: 10.0,
            buy_price_lot: 10.0 * scu as f64,
            stop_no: 1,
            destination: dest.map(Into::into),
            boxes,
            whereabouts: Whereabouts::Aboard,
            stored_at: None,
            notes: None,
        }
    }

    fn new_voyage(ship_id: Option<i64>) -> NewVoyage {
        NewVoyage { ship_id, bought_at: "2026-11-08".into(), notes: None }
    }

    #[tokio::test]
    async fn voyage_load_crate_round_trip() {
        let (pool, gig, _) = test_pool().await;
        let hull = find_ship_by_name(&pool, "hull-c").await.unwrap().unwrap();
        assert_eq!(hull.name, "Hull-C");
        assert_eq!(hull.sc_cargo_slug, "hull-c");
        assert_eq!(list_ships(&pool).await.unwrap().len(), 80);

        let id = create_voyage(
            &pool,
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
        let v = create_voyage(&pool, gig, &new_voyage(None), &[new_load("Laranite", vec![(3, 8)], None)])
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
        let a = create_voyage(&pool, gig, &new_voyage(None), &[new_load("Laranite", vec![(1, 32)], None)])
            .await
            .unwrap();
        let b = create_voyage(
            &pool,
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
        let v = create_voyage(&pool, gig, &new_voyage(None), &[new_load("Laranite", vec![(2, 32)], None)])
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
        let v = create_voyage(&pool, gig, &new_voyage(None), &[new_load("Laranite", vec![(2, 32), (1, 8)], None)])
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
    async fn delete_load_removes_an_emptied_voyage() {
        let (pool, gig, _) = test_pool().await;
        let v = create_voyage(&pool, gig, &new_voyage(None), &[new_load("Laranite", vec![(1, 8)], None)])
            .await
            .unwrap();
        let load = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        delete_load(&pool, load.id).await.unwrap();
        assert!(find_voyage(&pool, v).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn edit_load_moves_live_crates_when_asked() {
        let (pool, gig, _) = test_pool().await;
        let v = create_voyage(&pool, gig, &new_voyage(None), &[new_load("Laranite", vec![(2, 8)], None)])
            .await
            .unwrap();
        let load = find_voyage(&pool, v).await.unwrap().unwrap().loads.remove(0);
        update_load(
            &pool,
            load.id,
            &EditLoad {
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
}
