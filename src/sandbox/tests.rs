use super::describe::{conflict_lines, describe, Names};
use super::merge::{self, conflicts, diff, Conflict, Outcome, Snapshot, Val};
use super::*;

/// A live database file, its sandbox registry, and the directory holding
/// both (removed on drop).
struct World {
    live: SqlitePool,
    boxes: Sandboxes,
    root: PathBuf,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

async fn world() -> World {
    let root = std::env::temp_dir().join(format!("ripae-sbtest-{}", new_id(unix_now())));
    std::fs::create_dir_all(&root).unwrap();
    let live = db::connect(root.join("live.db").to_str().unwrap()).await.unwrap();
    db::migrate(&live).await.unwrap();
    for (h, n) in [("lordgig", "Lord Gig"), ("firstofficer", "JT")] {
        sqlx::query("INSERT INTO users (handle, display_name, password_hash) VALUES (?, ?, 'x')")
            .bind(h)
            .bind(n)
            .execute(&live)
            .await
            .unwrap();
    }
    let boxes = Sandboxes::new(root.join("sandboxes")).unwrap();
    World { live, boxes, root }
}

async fn exec(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query(sql).execute(pool).await.unwrap().last_insert_rowid()
}

async fn voyage(pool: &SqlitePool, ship: Option<&str>) -> i64 {
    let ship = match ship {
        Some(s) => format!("(SELECT id FROM ships WHERE name = '{s}')"),
        None => "NULL".into(),
    };
    exec(pool, &format!("INSERT INTO voyages (logged_by, entered_by, ship_id, bought_at) VALUES (1, 1, {ship}, '2026-11-10')")).await
}

/// An open load of `crates` × `size` SCU at 2,800/SCU.
async fn load(pool: &SqlitePool, voyage: i64, commodity: &str, crates: usize, size: i64) -> i64 {
    let id = exec(
        pool,
        &format!(
            "INSERT INTO loads (voyage_id, submitted_by, bought_by, commodity, buy_terminal, price_mode, buy_price_scu) \
             VALUES ({voyage}, 1, 1, '{commodity}', 'ArcCorp Mining Area 141', 'scu', 2800)"
        ),
    )
    .await;
    for _ in 0..crates {
        exec(pool, &format!("INSERT INTO crates (load_id, size_scu) VALUES ({id}, {size})")).await;
    }
    id
}

async fn count(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

async fn snaps(w: &World, id: &str) -> (Snapshot, Snapshot, Arc<Sandbox>) {
    let sb = w.boxes.get(id).await.expect("sandbox open");
    (merge::snapshot_pool(&sb.base).await.unwrap(), merge::snapshot_pool(&sb.pool).await.unwrap(), sb)
}

async fn names(pool: &SqlitePool) -> Names {
    super::names(pool).await
}

// ---------------------------------------------------------------------------
// ids and the registry
// ---------------------------------------------------------------------------

#[test]
fn ids_carry_their_birth_and_refuse_anything_path_like() {
    let id = new_id(1_700_000_000);
    assert_eq!(id_created(&id), Some(1_700_000_000));
    assert!(!expired(&id, 1_700_000_000 + TTL.as_secs() - 1));
    assert!(expired(&id, 1_700_000_000 + TTL.as_secs()));
    for bad in ["", "../live", "123-abc", "123-../../../../etc/passwd0000000000", "x-0123456789abcdef0123456789abcdef"] {
        assert_eq!(id_created(bad), None, "{bad}");
    }
    assert_eq!(file_id(&format!("{id}.base.db-journal")), Some(id.as_str()));
    assert_eq!(file_id(&format!("{id}.db")), Some(id.as_str()));
}

#[tokio::test]
async fn a_sandbox_is_a_separate_copy_and_discard_deletes_it() {
    let w = world().await;
    let v = voyage(&w.live, Some("Caterpillar")).await;
    load(&w.live, v, "Laranite", 1, 32).await;
    let id = w.boxes.create(&w.live).await.unwrap();
    let sb = w.boxes.get(&id).await.unwrap();
    assert_eq!(count(&sb.pool, "SELECT COUNT(*) FROM loads").await, 1, "the copy has live's books");
    exec(&sb.pool, "DELETE FROM crates").await;
    exec(&sb.pool, "DELETE FROM loads").await;
    assert_eq!(count(&w.live, "SELECT COUNT(*) FROM loads").await, 1, "live untouched");
    let (f, b) = w.boxes.paths(&id);
    assert!(f.exists() && b.exists());
    w.boxes.destroy(&id).await;
    assert!(!f.exists() && !b.exists());
    assert!(w.boxes.get(&id).await.is_none());
}

#[tokio::test]
async fn the_sweep_deletes_expired_and_orphaned_sandboxes_and_keeps_fresh_ones() {
    let w = world().await;
    let fresh = w.boxes.create(&w.live).await.unwrap();
    let old = new_id(unix_now() - TTL.as_secs() - 5);
    let (of, ob) = w.boxes.paths(&old);
    std::fs::copy(w.boxes.paths(&fresh).0, &of).unwrap();
    std::fs::copy(w.boxes.paths(&fresh).1, &ob).unwrap();
    let orphan = new_id(unix_now());
    let (orf, _) = w.boxes.paths(&orphan);
    std::fs::write(&orf, b"").unwrap();
    assert_eq!(w.boxes.sweep().await, 2);
    assert!(!of.exists() && !ob.exists() && !orf.exists());
    assert!(w.boxes.get(&fresh).await.is_some());
    // an expired id is never handed out, even before the sweep
    assert!(w.boxes.get(&old).await.is_none());
}

#[tokio::test]
async fn a_sandbox_made_before_a_restart_reopens() {
    let w = world().await;
    let id = w.boxes.create(&w.live).await.unwrap();
    let again = Sandboxes::new(w.boxes.dir.clone()).unwrap(); // a new process
    assert!(again.get(&id).await.is_some());
}

#[tokio::test]
async fn the_copies_carry_no_sessions() {
    let w = world().await;
    let store = tower_sessions_sqlx_store::SqliteStore::new(w.live.clone());
    store.migrate().await.unwrap();
    exec(&w.live, "INSERT INTO tower_sessions (id, data, expiry_date) VALUES ('s', x'00', 9999999999)").await;
    let id = w.boxes.create(&w.live).await.unwrap();
    let sb = w.boxes.get(&id).await.unwrap();
    assert_eq!(count(&sb.pool, "SELECT COUNT(*) FROM tower_sessions").await, 0);
    assert_eq!(count(&sb.base, "SELECT COUNT(*) FROM tower_sessions").await, 0);
}

// ---------------------------------------------------------------------------
// diff
// ---------------------------------------------------------------------------

fn snap(rows: &[(&str, i64, &[(&str, Val)])]) -> Snapshot {
    let mut s = Snapshot::default();
    for (t, id, cols) in rows {
        let mut r: merge::RowMap = cols.iter().map(|(c, v)| (c.to_string(), v.clone())).collect();
        r.insert("id".into(), Val::Int(*id));
        s.tables.entry(t.to_string()).or_default().insert(*id, r);
    }
    s
}

#[test]
fn diff_sorts_rows_into_inserted_updated_and_deleted_by_key() {
    let base = snap(&[
        ("voyages", 1, &[("notes", Val::Null)]),
        ("voyages", 2, &[("notes", Val::Text("x".into()))]),
        ("loads", 7, &[("buy_price_scu", Val::Real(2800.0))]),
    ]);
    let after = snap(&[
        ("voyages", 1, &[("notes", Val::Null)]),
        ("voyages", 3, &[("notes", Val::Null)]),
        ("loads", 7, &[("buy_price_scu", Val::Real(2800.5))]),
    ]);
    let d = diff(&base, &after);
    assert_eq!(d.table("voyages").inserted.keys().collect::<Vec<_>>(), [&3]);
    assert_eq!(d.table("voyages").deleted.keys().collect::<Vec<_>>(), [&2]);
    assert!(d.table("voyages").updated.is_empty(), "an untouched row is no change");
    assert_eq!(d.table("loads").updated.keys().collect::<Vec<_>>(), [&7]);
    assert_eq!(d.len(), 3);
    assert_eq!(diff(&base, &base).len(), 0);
}

#[test]
fn a_change_live_already_has_is_not_a_conflict() {
    let base = snap(&[("loads", 7, &[("notes", Val::Null)])]);
    let after = snap(&[("loads", 7, &[("notes", Val::Text("both".into()))])]);
    let d = diff(&base, &after);
    assert!(conflicts(&d, &base, &after, &[]).is_empty());
    let other = snap(&[("loads", 7, &[("notes", Val::Text("theirs".into()))])]);
    assert_eq!(conflicts(&d, &base, &other, &[]), [Conflict::ChangedInLive { table: "loads".into(), id: 7 }]);
    assert_eq!(conflicts(&d, &base, &Snapshot::default(), &[]), [Conflict::DeletedInLive { table: "loads".into(), id: 7 }]);
}

// ---------------------------------------------------------------------------
// merge, end to end
// ---------------------------------------------------------------------------

#[tokio::test]
async fn keep_applies_inserts_updates_and_deletes_under_fresh_ids() {
    let w = world().await;
    let v1 = voyage(&w.live, Some("Caterpillar")).await;
    let l1 = load(&w.live, v1, "Laranite", 2, 16).await;
    let v2 = voyage(&w.live, None).await;
    load(&w.live, v2, "Gold", 1, 4).await;
    let id = w.boxes.create(&w.live).await.unwrap();
    let sb = w.boxes.get(&id).await.unwrap();

    // in test mode: a new voyage with two loads, a price fix, a crate lost,
    // and the Gold voyage deleted
    let nv = voyage(&sb.pool, Some("Hull-C")).await;
    let na = load(&sb.pool, nv, "Agricium", 2, 32).await;
    load(&sb.pool, nv, "Titanium", 1, 8).await;
    exec(&sb.pool, &format!("UPDATE loads SET buy_price_scu = 3000 WHERE id = {l1}")).await;
    exec(&sb.pool, &format!("UPDATE crates SET condition = 'lost' WHERE id = (SELECT MIN(id) FROM crates WHERE load_id = {l1})")).await;
    exec(&sb.pool, &format!("DELETE FROM crates WHERE load_id IN (SELECT id FROM loads WHERE voyage_id = {v2})")).await;
    exec(&sb.pool, &format!("DELETE FROM loads WHERE voyage_id = {v2}")).await;
    exec(&sb.pool, &format!("DELETE FROM voyages WHERE id = {v2}")).await;
    // test mode's crew edits are not ledger data
    exec(&sb.pool, "UPDATE users SET display_name = 'Hacked' WHERE id = 1").await;

    // meanwhile on live, someone else logs a run: it takes the id the
    // sandbox's new voyage has
    let theirs = voyage(&w.live, None).await;
    assert_eq!(theirs, nv, "the ids collide; the merge must not care");
    load(&w.live, theirs, "Iron", 1, 1).await;

    let (base, after, _) = snaps(&w, &id).await;
    let Outcome::Merged { rows } = merge::merge(&w.live, &base, &after).await.unwrap() else { panic!("refused") };
    assert!(rows > 0);

    assert_eq!(count(&w.live, "SELECT COUNT(*) FROM voyages").await, 3, "v1, theirs, and the new one");
    assert_eq!(count(&w.live, &format!("SELECT COUNT(*) FROM voyages WHERE id = {v2}")).await, 0, "Gold voyage gone");
    let new_v: i64 = count(&w.live, "SELECT id FROM voyages WHERE ship_id = (SELECT id FROM ships WHERE name = 'Hull-C')").await;
    assert_ne!(new_v, nv, "the new voyage got a fresh id");
    assert_eq!(count(&w.live, &format!("SELECT COUNT(*) FROM loads WHERE voyage_id = {new_v}")).await, 2);
    assert_eq!(count(&w.live, &format!("SELECT COUNT(*) FROM loads WHERE voyage_id = {theirs}")).await, 1, "their Iron load stays theirs");
    let new_a: i64 = count(&w.live, &format!("SELECT id FROM loads WHERE commodity = 'Agricium' AND voyage_id = {new_v}")).await;
    assert_ne!(new_a, na);
    assert_eq!(count(&w.live, &format!("SELECT SUM(size_scu) FROM crates WHERE load_id = {new_a}")).await, 64, "crates follow their load");
    let price: f64 = sqlx::query_scalar(&format!("SELECT buy_price_scu FROM loads WHERE id = {l1}")).fetch_one(&w.live).await.unwrap();
    assert_eq!(price, 3000.0);
    assert_eq!(count(&w.live, &format!("SELECT COUNT(*) FROM crates WHERE load_id = {l1} AND condition = 'lost'")).await, 1);
    let name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE id = 1").fetch_one(&w.live).await.unwrap();
    assert_eq!(name, "Lord Gig", "users are never merged");
}

#[tokio::test]
async fn keep_refuses_a_row_live_changed_too_and_writes_nothing() {
    let w = world().await;
    let v = voyage(&w.live, Some("Caterpillar")).await;
    let l = load(&w.live, v, "Laranite", 1, 32).await;
    let id = w.boxes.create(&w.live).await.unwrap();
    let sb = w.boxes.get(&id).await.unwrap();
    exec(&sb.pool, &format!("UPDATE loads SET buy_price_scu = 3000 WHERE id = {l}")).await;
    voyage(&sb.pool, None).await; // an innocent insert alongside
    exec(&w.live, &format!("UPDATE loads SET buy_price_scu = 2900 WHERE id = {l}")).await;

    let (base, after, _) = snaps(&w, &id).await;
    let Outcome::Refused(found) = merge::merge(&w.live, &base, &after).await.unwrap() else { panic!("merged over live") };
    assert_eq!(found, [Conflict::ChangedInLive { table: "loads".into(), id: l }]);
    let price: f64 = sqlx::query_scalar(&format!("SELECT buy_price_scu FROM loads WHERE id = {l}")).fetch_one(&w.live).await.unwrap();
    assert_eq!(price, 2900.0, "live keeps its own change");
    assert_eq!(count(&w.live, "SELECT COUNT(*) FROM voyages").await, 1, "and nothing else landed either");
    let now = merge::snapshot_pool(&w.live).await.unwrap();
    let lines = conflict_lines(&found, &base, &after, &now, &names(&sb.pool).await);
    assert_eq!(lines, [format!("The Laranite load on Voyage {v} was changed in live since you entered test mode.")]);
}

#[tokio::test]
async fn keep_refuses_deleting_a_voyage_live_has_added_to() {
    let w = world().await;
    let v = voyage(&w.live, Some("Caterpillar")).await;
    let l = load(&w.live, v, "Laranite", 1, 32).await;
    let id = w.boxes.create(&w.live).await.unwrap();
    let sb = w.boxes.get(&id).await.unwrap();
    exec(&sb.pool, &format!("DELETE FROM loads WHERE id = {l}")).await; // crates cascade
    exec(&sb.pool, &format!("DELETE FROM voyages WHERE id = {v}")).await;
    let theirs = load(&w.live, v, "Gold", 1, 4).await;

    let (base, after, _) = snaps(&w, &id).await;
    let Outcome::Refused(found) = merge::merge(&w.live, &base, &after).await.unwrap() else { panic!("merged") };
    assert!(found.contains(&Conflict::ChildInLive { table: "voyages".into(), id: v, child: "loads".into(), child_id: theirs }), "{found:?}");
    assert_eq!(count(&w.live, "SELECT COUNT(*) FROM loads").await, 2);
}

#[tokio::test]
async fn keep_refuses_adding_to_a_voyage_live_deleted() {
    let w = world().await;
    let v = voyage(&w.live, Some("Caterpillar")).await;
    let id = w.boxes.create(&w.live).await.unwrap();
    let sb = w.boxes.get(&id).await.unwrap();
    let added = load(&sb.pool, v, "Laranite", 1, 32).await;
    exec(&w.live, &format!("DELETE FROM voyages WHERE id = {v}")).await;

    let (base, after, _) = snaps(&w, &id).await;
    let Outcome::Refused(found) = merge::merge(&w.live, &base, &after).await.unwrap() else { panic!("merged") };
    assert!(found.contains(&Conflict::ParentGone { table: "loads".into(), id: added, parent: "voyages".into(), parent_id: v }), "{found:?}");
    assert_eq!(count(&w.live, "SELECT COUNT(*) FROM loads").await, 0);
}

// ---------------------------------------------------------------------------
// describe
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_diff_reads_like_the_ledger() {
    let w = world().await;
    let v = voyage(&w.live, Some("Caterpillar")).await;
    let l = load(&w.live, v, "Laranite", 1, 32).await;
    let s = exec(&w.live, &format!("INSERT INTO shares (load_id, to_user, paid_to, kind, amount, paid_auec) VALUES ({l}, 2, 2, 'pct', 25, 0)")).await;
    let gone = voyage(&w.live, None).await;
    load(&w.live, gone, "Gold", 2, 4).await;
    let id = w.boxes.create(&w.live).await.unwrap();
    let sb = w.boxes.get(&id).await.unwrap();

    let nv = voyage(&sb.pool, Some("Caterpillar")).await;
    for c in ["Agricium", "Titanium", "Iron"] {
        load(&sb.pool, nv, c, 1, 8).await;
    }
    exec(&sb.pool, &format!("UPDATE crates SET condition = 'sold', sold_at = '2026-11-11' WHERE load_id = {l}")).await;
    exec(&sb.pool, &format!("UPDATE loads SET sold_at = '2026-11-11', sell_terminal = 'Area18 TDD', sell_price_scu = 38.5625, submitted_by = 1 WHERE id = {l}")).await;
    exec(&sb.pool, &format!("UPDATE shares SET amount = 30 WHERE id = {s}")).await;
    exec(&sb.pool, &format!("DELETE FROM crates WHERE load_id IN (SELECT id FROM loads WHERE voyage_id = {gone})")).await;
    exec(&sb.pool, &format!("DELETE FROM loads WHERE voyage_id = {gone}")).await;
    exec(&sb.pool, &format!("DELETE FROM voyages WHERE id = {gone}")).await;

    let (base, after, _) = snaps(&w, &id).await;
    let groups = describe(&diff(&base, &after), &base, &after, &names(&sb.pool).await);
    let titles: Vec<&str> = groups.iter().map(|g| g.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            format!("Voyage {v} · Caterpillar · 2026-11-10"),
            format!("Voyage {gone} deleted · no hull · 2026-11-10 · 1 load · 8 SCU"),
            "New voyage · Caterpillar · 2026-11-10 · 3 loads".to_string(),
        ]
    );
    assert_eq!(
        groups[0].lines,
        ["Load Laranite 32 SCU sold at Area18 TDD for 1,234 aUEC", "Share for JT on Laranite changed 25% → 30%"]
    );
    assert!(groups[1].lines.is_empty(), "a deleted voyage's loads are in its title");
    assert_eq!(groups[2].lines[0], "New load Agricium 8 SCU bought at ArcCorp Mining Area 141 for 22,400 aUEC");
    assert_eq!(groups[2].kind, "new");
}

#[tokio::test]
async fn crate_changes_are_summed_per_load() {
    let w = world().await;
    let v = voyage(&w.live, None).await;
    let l = load(&w.live, v, "Laranite", 3, 8).await;
    let id = w.boxes.create(&w.live).await.unwrap();
    let sb = w.boxes.get(&id).await.unwrap();
    exec(&sb.pool, &format!("UPDATE crates SET whereabouts = 'stored', stored_at = 'Everus Harbor' WHERE load_id = {l}")).await;
    exec(&sb.pool, &format!("UPDATE crates SET condition = 'lost' WHERE id = (SELECT MIN(id) FROM crates WHERE load_id = {l})")).await;
    exec(&sb.pool, &format!("INSERT INTO crates (load_id, size_scu) VALUES ({l}, 32)")).await;
    let (base, after, _) = snaps(&w, &id).await;
    let groups = describe(&diff(&base, &after), &base, &after, &names(&sb.pool).await);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].lines, ["Laranite crates: 1 crate added (32 SCU) · 1 crate lost · 3 crates stored at Everus Harbor"]);
}

#[tokio::test]
async fn an_untouched_sandbox_describes_nothing_and_merges_nothing() {
    let w = world().await;
    let v = voyage(&w.live, None).await;
    load(&w.live, v, "Laranite", 1, 8).await;
    let id = w.boxes.create(&w.live).await.unwrap();
    let (base, after, sb) = snaps(&w, &id).await;
    assert!(describe(&diff(&base, &after), &base, &after, &names(&sb.pool).await).is_empty());
    let Outcome::Merged { rows } = merge::merge(&w.live, &base, &after).await.unwrap() else { panic!() };
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn a_partial_close_reads_as_a_sale_and_what_was_kept() {
    let w = world().await;
    let v = voyage(&w.live, None).await;
    let l = load(&w.live, v, "Agricium", 2, 32).await;
    exec(&w.live, &format!("UPDATE loads SET price_mode = 'lot', buy_price_lot = 179200 WHERE id = {l}")).await;
    let id = w.boxes.create(&w.live).await.unwrap();
    let sb = w.boxes.get(&id).await.unwrap();
    // what close_load does with one of two crates: a new sold row takes it
    let sold = exec(
        &sb.pool,
        &format!(
            "INSERT INTO loads (voyage_id, submitted_by, bought_by, commodity, buy_terminal, price_mode, buy_price_scu, buy_price_lot, \
             sold_at, sell_terminal, sell_price_scu) VALUES ({v}, 1, 1, 'Agricium', 'ArcCorp Mining Area 141', 'lot', 2800, 89600, \
             '2026-11-11', 'Area18 TDD', 3000)"
        ),
    )
    .await;
    exec(&sb.pool, &format!("UPDATE crates SET load_id = {sold}, condition = 'sold' WHERE id = (SELECT MIN(id) FROM crates WHERE load_id = {l})")).await;
    exec(&sb.pool, &format!("UPDATE loads SET buy_price_lot = 89600 WHERE id = {l}")).await;
    let (base, after, _) = snaps(&w, &id).await;
    let groups = describe(&diff(&base, &after), &base, &after, &names(&sb.pool).await);
    assert_eq!(
        groups[0].lines,
        ["Load Agricium 32 SCU sold at Area18 TDD for 96,000 aUEC", "Load Agricium: 32 SCU kept open (the rest sold)"]
    );
    let Outcome::Merged { .. } = merge::merge(&w.live, &base, &after).await.unwrap() else { panic!() };
    assert_eq!(count(&w.live, "SELECT COUNT(*) FROM crates WHERE condition = 'sold' AND load_id = (SELECT MAX(id) FROM loads)").await, 1);
}
