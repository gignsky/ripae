//! The generic half of test mode: snapshot the ledger tables, diff two
//! snapshots row by row on the primary key, check a change set against
//! live for conflicts, and apply it to live.
//!
//! Nothing here knows what a voyage is. Tables are read with `SELECT *`
//! into column-name → value maps, foreign keys come from
//! `pragma_foreign_key_list`, so a column added by a later migration is
//! diffed and merged without touching this file. A new *table* only takes
//! part once it's listed in [`LEDGER_TABLES`].

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use sqlx::sqlite::SqliteRow;
use sqlx::{Column, Row, SqliteConnection, SqlitePool, TypeInfo, ValueRef};

/// The mergeable tables, parents before children. Users, sessions, auth
/// and the pick-lists are deliberately absent: they're never diffed and
/// never written back.
pub const LEDGER_TABLES: &[&str] = &["voyages", "voyage_stops", "loads", "crates", "shares", "voyage_crew"];

/// One SQLite value, exactly as stored (no float formatting round trip).
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Val {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Val::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Val::Int(i) => Some(*i as f64),
            Val::Real(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Val::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Val::Null)
    }
}

/// A row by column name. The primary key is in it too.
pub type RowMap = BTreeMap<String, Val>;

/// Every ledger table's rows by primary key.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub tables: BTreeMap<String, BTreeMap<i64, RowMap>>,
}

impl Snapshot {
    pub fn row(&self, table: &str, id: i64) -> Option<&RowMap> {
        self.tables.get(table).and_then(|t| t.get(&id))
    }

    pub fn rows(&self, table: &str) -> impl Iterator<Item = (&i64, &RowMap)> {
        self.tables.get(table).into_iter().flat_map(|t| t.iter())
    }
}

/// `table.col REFERENCES parent(id)`, both tables mergeable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fk {
    pub table: String,
    pub col: String,
    pub parent: String,
}

/// What one table gained, changed and lost (sandbox versus its baseline).
#[derive(Debug, Clone, Default)]
pub struct TableChanges {
    pub inserted: BTreeMap<i64, RowMap>,
    /// `(before, after)`.
    pub updated: BTreeMap<i64, (RowMap, RowMap)>,
    pub deleted: BTreeMap<i64, RowMap>,
}

impl TableChanges {
    pub fn is_empty(&self) -> bool {
        self.inserted.is_empty() && self.updated.is_empty() && self.deleted.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChangeSet {
    pub tables: BTreeMap<String, TableChanges>,
}

impl ChangeSet {
    pub fn table(&self, name: &str) -> &TableChanges {
        static EMPTY: std::sync::OnceLock<TableChanges> = std::sync::OnceLock::new();
        self.tables.get(name).unwrap_or_else(|| EMPTY.get_or_init(TableChanges::default))
    }

    /// Rows touched, all tables.
    pub fn len(&self) -> usize {
        self.tables.values().map(|t| t.inserted.len() + t.updated.len() + t.deleted.len()).sum()
    }
}

/// Why a merge was refused, row by row.
#[derive(Debug, Clone, PartialEq)]
pub enum Conflict {
    /// Test mode changed or deleted the row, and live changed it too.
    ChangedInLive { table: String, id: i64 },
    /// Test mode changed or deleted the row; live deleted it.
    DeletedInLive { table: String, id: i64 },
    /// A row test mode wrote points at a parent live has since deleted.
    ParentGone { table: String, id: i64, parent: String, parent_id: i64 },
    /// Test mode deleted a row that live has since hung a new child on.
    ChildInLive { table: String, id: i64, child: String, child_id: i64 },
}

/// Rows equal on the columns both have (a migration between snapshots may
/// have added one).
fn same(a: &RowMap, b: &RowMap) -> bool {
    a.iter().all(|(k, v)| b.get(k).map_or(true, |w| w == v))
}

/// Insert / update / delete sets of `after` against `before`, by key.
pub fn diff(before: &Snapshot, after: &Snapshot) -> ChangeSet {
    let mut out = ChangeSet::default();
    for &t in LEDGER_TABLES {
        let empty = BTreeMap::new();
        let b = before.tables.get(t).unwrap_or(&empty);
        let a = after.tables.get(t).unwrap_or(&empty);
        let mut ch = TableChanges::default();
        for (id, row) in a {
            match b.get(id) {
                None => {
                    ch.inserted.insert(*id, row.clone());
                }
                Some(old) if !same(old, row) => {
                    ch.updated.insert(*id, (old.clone(), row.clone()));
                }
                Some(_) => {}
            }
        }
        for (id, row) in b {
            if !a.contains_key(id) {
                ch.deleted.insert(*id, row.clone());
            }
        }
        if !ch.is_empty() {
            out.tables.insert(t.to_string(), ch);
        }
    }
    out
}

/// Everything that stops `changes` (sandbox versus `base`) from landing on
/// `live` without clobbering someone's work. Empty = safe to apply.
pub fn conflicts(changes: &ChangeSet, base: &Snapshot, live: &Snapshot, fks: &[Fk]) -> Vec<Conflict> {
    let mut out = Vec::new();
    for (t, ch) in &changes.tables {
        // rows test mode changed: live must still hold the baseline (or
        // already hold exactly what test mode wants)
        for (id, (_, after)) in &ch.updated {
            match (live.row(t, *id), base.row(t, *id)) {
                (None, _) => out.push(Conflict::DeletedInLive { table: t.clone(), id: *id }),
                (Some(l), Some(b)) if !same(b, l) && !same(after, l) => {
                    out.push(Conflict::ChangedInLive { table: t.clone(), id: *id })
                }
                _ => {}
            }
        }
        for (id, before) in &ch.deleted {
            match live.row(t, *id) {
                // already gone in live too: nothing to do, not a conflict
                None => {}
                Some(l) if !same(before, l) => out.push(Conflict::ChangedInLive { table: t.clone(), id: *id }),
                Some(_) => {}
            }
        }
    }
    for fk in fks {
        let child = changes.table(&fk.table);
        let parent = changes.table(&fk.parent);
        // a written row's parent must survive: inserted by the same merge,
        // or still in live
        let written = child.inserted.iter().chain(child.updated.iter().map(|(id, (_, a))| (id, a)));
        for (id, row) in written {
            let Some(pid) = row.get(&fk.col).and_then(Val::as_i64) else { continue };
            if parent.inserted.contains_key(&pid) || live.row(&fk.parent, pid).is_some() {
                continue;
            }
            out.push(Conflict::ParentGone { table: fk.table.clone(), id: *id, parent: fk.parent.clone(), parent_id: pid });
        }
        // a deleted parent must not take a live child down with it, unless
        // test mode itself deleted or re-pointed that child
        for pid in parent.deleted.keys() {
            for (cid, row) in live.rows(&fk.table) {
                if row.get(&fk.col).and_then(Val::as_i64) != Some(*pid) {
                    continue;
                }
                if child.deleted.contains_key(cid) || child.updated.contains_key(cid) {
                    continue;
                }
                out.push(Conflict::ChildInLive {
                    table: fk.parent.clone(),
                    id: *pid,
                    child: fk.table.clone(),
                    child_id: *cid,
                });
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// reading and writing
// ---------------------------------------------------------------------------

fn val(row: &SqliteRow, i: usize) -> Result<Val> {
    let raw = row.try_get_raw(i)?;
    if raw.is_null() {
        return Ok(Val::Null);
    }
    // the value's own storage class, not the column's declared type
    let ty = raw.type_info().name().to_string();
    Ok(match ty.as_str() {
        "INTEGER" | "BOOLEAN" => Val::Int(row.try_get_unchecked(i)?),
        "REAL" | "NUMERIC" => Val::Real(row.try_get_unchecked(i)?),
        "BLOB" => Val::Blob(row.try_get_unchecked(i)?),
        _ => Val::Text(row.try_get_unchecked(i)?),
    })
}

async fn table_exists(conn: &mut SqliteConnection, table: &str) -> Result<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
        .bind(table)
        .fetch_one(&mut *conn)
        .await?;
    Ok(n > 0)
}

/// The integer primary key column of a ledger table.
async fn pk(conn: &mut SqliteConnection, table: &str) -> Result<String> {
    let cols: Vec<(String, i64)> = sqlx::query_as("SELECT name, pk FROM pragma_table_info(?)")
        .bind(table)
        .fetch_all(&mut *conn)
        .await?;
    let keys: Vec<_> = cols.into_iter().filter(|(_, pk)| *pk > 0).map(|(n, _)| n).collect();
    match keys.as_slice() {
        [one] => Ok(one.clone()),
        _ => bail!("{table}: test mode needs a single-column primary key"),
    }
}

fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Every ledger table the database has.
pub async fn snapshot(conn: &mut SqliteConnection) -> Result<Snapshot> {
    let mut snap = Snapshot::default();
    for &t in LEDGER_TABLES {
        if !table_exists(conn, t).await? {
            continue;
        }
        let key = pk(conn, t).await?;
        let rows = sqlx::query(&format!("SELECT * FROM {}", ident(t))).fetch_all(&mut *conn).await?;
        let mut table = BTreeMap::new();
        for r in &rows {
            let mut row = RowMap::new();
            for (i, c) in r.columns().iter().enumerate() {
                row.insert(c.name().to_string(), val(r, i)?);
            }
            let Some(id) = row.get(&key).and_then(Val::as_i64) else {
                bail!("{t}: a row without an integer key");
            };
            table.insert(id, row);
        }
        snap.tables.insert(t.to_string(), table);
    }
    Ok(snap)
}

pub async fn snapshot_pool(pool: &SqlitePool) -> Result<Snapshot> {
    let mut conn = pool.acquire().await?;
    snapshot(&mut conn).await
}

/// Foreign keys between ledger tables, as the schema declares them.
pub async fn foreign_keys(conn: &mut SqliteConnection) -> Result<Vec<Fk>> {
    let mut out = Vec::new();
    for &t in LEDGER_TABLES {
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT \"table\", \"from\" FROM pragma_foreign_key_list(?)")
            .bind(t)
            .fetch_all(&mut *conn)
            .await?;
        for (parent, col) in rows {
            if LEDGER_TABLES.contains(&parent.as_str()) {
                out.push(Fk { table: t.to_string(), col, parent });
            }
        }
    }
    Ok(out)
}

fn bind<'q>(
    q: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    v: &Val,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    match v.clone() {
        Val::Null => q.bind(None::<i64>),
        Val::Int(i) => q.bind(i),
        Val::Real(f) => q.bind(f),
        Val::Text(s) => q.bind(s),
        Val::Blob(b) => q.bind(b),
    }
}

/// New ids live gave the rows test mode inserted, per table.
pub type IdMap = BTreeMap<String, BTreeMap<i64, i64>>;

/// A row's foreign keys pointed at the ids live gave their parents.
fn remap(table: &str, row: &RowMap, fks: &[Fk], ids: &IdMap) -> RowMap {
    let mut row = row.clone();
    for fk in fks.iter().filter(|f| f.table == table) {
        if let Some(Val::Int(old)) = row.get(&fk.col) {
            if let Some(new) = ids.get(&fk.parent).and_then(|m| m.get(old)) {
                row.insert(fk.col.clone(), Val::Int(*new));
            }
        }
    }
    row
}

async fn delete_rows(conn: &mut SqliteConnection, table: &str, changes: &ChangeSet) -> Result<()> {
    let ch = changes.table(table);
    if ch.deleted.is_empty() {
        return Ok(());
    }
    let key = pk(conn, table).await?;
    let sql = format!("DELETE FROM {} WHERE {} = ?", ident(table), ident(&key));
    for id in ch.deleted.keys() {
        sqlx::query(&sql).bind(*id).execute(&mut *conn).await?;
    }
    Ok(())
}

/// Writes `changes` to `conn` (inside the caller's transaction): deletes
/// from tables nothing points at, then inserts parent-first under fresh ids
/// (live may have used the sandbox's ids since it was made), then updates,
/// then the remaining deletes child-first. Returns the id map.
///
/// Leaf rows go first because a rewrite in the sandbox (delete a voyage's
/// stops, insert them again) lands as deletes plus inserts, and the new
/// rows would hit a unique key the old ones still hold. A parent's delete
/// has to wait: its children may be re-pointed by the updates, and an
/// `ON DELETE CASCADE` would take them first.
pub async fn apply(conn: &mut SqliteConnection, changes: &ChangeSet, fks: &[Fk]) -> Result<IdMap> {
    let leaf = |t: &str| !fks.iter().any(|f| f.parent == t);
    for &t in LEDGER_TABLES.iter().rev().filter(|t| leaf(t)) {
        delete_rows(conn, t, changes).await?;
    }
    let mut ids = IdMap::new();
    for &t in LEDGER_TABLES {
        let ch = changes.table(t);
        if ch.inserted.is_empty() {
            continue;
        }
        let key = pk(conn, t).await?;
        for (old, row) in &ch.inserted {
            let row = remap(t, row, fks, &ids);
            let cols: Vec<&String> = row.keys().filter(|c| **c != key).collect();
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({})",
                ident(t),
                cols.iter().map(|c| ident(c)).collect::<Vec<_>>().join(", "),
                vec!["?"; cols.len()].join(", ")
            );
            let mut q = sqlx::query(&sql);
            for c in &cols {
                q = bind(q, &row[*c]);
            }
            let new = q.execute(&mut *conn).await?.last_insert_rowid();
            ids.entry(t.to_string()).or_default().insert(*old, new);
        }
    }
    for &t in LEDGER_TABLES {
        let ch = changes.table(t);
        if ch.updated.is_empty() {
            continue;
        }
        let key = pk(conn, t).await?;
        for (id, (before, after)) in &ch.updated {
            // only what test mode changed; anything else stays as live has it
            let cols: Vec<&String> = after
                .keys()
                .filter(|c| **c != key && before.get(*c) != after.get(*c))
                .collect();
            let after = remap(t, after, fks, &ids);
            if cols.is_empty() {
                continue;
            }
            let sql = format!(
                "UPDATE {} SET {} WHERE {} = ?",
                ident(t),
                cols.iter().map(|c| format!("{} = ?", ident(c))).collect::<Vec<_>>().join(", "),
                ident(&key)
            );
            let mut q = sqlx::query(&sql);
            for c in &cols {
                q = bind(q, &after[*c]);
            }
            q.bind(*id).execute(&mut *conn).await?;
        }
    }
    for &t in LEDGER_TABLES.iter().rev().filter(|t| !leaf(t)) {
        delete_rows(conn, t, changes).await?;
    }
    Ok(ids)
}

pub enum Outcome {
    Merged { rows: usize },
    Refused(Vec<Conflict>),
}

/// Diff `sandbox` against `base` and write it to `live` in one
/// transaction, or refuse (writing nothing) if live moved underneath.
pub async fn merge(live: &SqlitePool, base: &Snapshot, sandbox: &Snapshot) -> Result<Outcome> {
    let changes = diff(base, sandbox);
    let mut tx = live.begin().await?;
    let fks = foreign_keys(&mut tx).await?;
    let now = snapshot(&mut tx).await?;
    let found = conflicts(&changes, base, &now, &fks);
    if !found.is_empty() {
        tx.rollback().await?;
        return Ok(Outcome::Refused(found));
    }
    apply(&mut tx, &changes, &fks).await?;
    // belt and braces: nothing may dangle once it's written
    let dangling = sqlx::query("PRAGMA foreign_key_check").fetch_all(&mut *tx).await?;
    if let Some(first) = dangling.first() {
        let table: String = first.try_get(0).unwrap_or_default();
        tx.rollback().await?;
        bail!("merge would leave {} dangling reference(s) in {table}", dangling.len());
    }
    tx.commit().await?;
    Ok(Outcome::Merged { rows: changes.len() })
}
