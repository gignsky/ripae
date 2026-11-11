//! SQL for the fleet (`hulls`, migration 0014). Same house style as
//! `stowage::repo`: runtime queries, hand-written row mappers, multi-row
//! writes in a transaction. The rules themselves live in `model`.

use anyhow::{anyhow, Result};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::model::{apply, contents, resolve, Action, HullKind, HullStatus, Node};

#[derive(Debug, Clone, PartialEq)]
pub struct Hull {
    pub id: i64,
    pub owner_id: i64,
    pub owner_name: String,
    /// The model on the `ships` list, when it is on it.
    pub ship_id: Option<i64>,
    /// The model's name: the list's spelling, or as typed.
    pub model: String,
    pub manufacturer: Option<String>,
    pub capacity_scu: Option<i64>,
    pub concept: bool,
    pub nickname: Option<String>,
    pub kind: HullKind,
    /// Its own status; a carried hull's real one is its carrier's (`model::resolve`).
    pub status: HullStatus,
    pub location: Option<String>,
    pub carried_in: Option<i64>,
    pub loaner: bool,
    pub loaner_for: Option<i64>,
    pub insurance_claim: Option<f64>,
    pub insurance_type: Option<String>,
    pub notes: Option<String>,
    /// 0.4 hangar import: where the row came from, and its id there.
    pub source: Option<String>,
    pub external_ref: Option<String>,
}

impl Hull {
    pub fn node(&self) -> Node {
        Node { id: self.id, status: self.status, location: self.location.clone(), carried_in: self.carried_in }
    }

    pub fn label(&self) -> String {
        super::model::hull_label(&self.model, self.nickname.as_deref())
    }
}

/// What the add / edit form writes.
#[derive(Debug, Clone, PartialEq)]
pub struct HullInput {
    pub owner_id: i64,
    pub ship_id: Option<i64>,
    pub model: String,
    pub nickname: Option<String>,
    pub kind: HullKind,
    pub status: HullStatus,
    pub location: Option<String>,
    pub carried_in: Option<i64>,
    pub loaner: bool,
    pub loaner_for: Option<i64>,
    pub insurance_claim: Option<f64>,
    pub insurance_type: Option<String>,
    pub notes: Option<String>,
}

const SELECT_HULL: &str = "SELECT h.id, h.owner_id, u.display_name AS owner_name, h.ship_id,
    COALESCE(s.name, h.model) AS model, s.manufacturer, s.capacity_scu, COALESCE(s.concept, 0) AS concept,
    h.nickname, h.kind, h.status, h.location, h.carried_in, h.loaner, h.loaner_for,
    h.insurance_claim, h.insurance_type, h.notes, h.source, h.external_ref
    FROM hulls h JOIN users u ON u.id = h.owner_id LEFT JOIN ships s ON s.id = h.ship_id";

fn row_to_hull(r: &SqliteRow) -> Result<Hull> {
    let kind: String = r.try_get("kind")?;
    let status: String = r.try_get("status")?;
    Ok(Hull {
        id: r.try_get("id")?,
        owner_id: r.try_get("owner_id")?,
        owner_name: r.try_get("owner_name")?,
        ship_id: r.try_get("ship_id")?,
        model: r.try_get("model")?,
        manufacturer: r.try_get("manufacturer")?,
        capacity_scu: r.try_get("capacity_scu")?,
        concept: r.try_get::<i64, _>("concept")? != 0,
        nickname: r.try_get("nickname")?,
        kind: HullKind::parse(&kind).ok_or_else(|| anyhow!("bad hull kind {kind}"))?,
        status: HullStatus::parse(&status).ok_or_else(|| anyhow!("bad hull status {status}"))?,
        location: r.try_get("location")?,
        carried_in: r.try_get("carried_in")?,
        loaner: r.try_get::<i64, _>("loaner")? != 0,
        loaner_for: r.try_get("loaner_for")?,
        insurance_claim: r.try_get("insurance_claim")?,
        insurance_type: r.try_get("insurance_type")?,
        notes: r.try_get("notes")?,
        source: r.try_get("source")?,
        external_ref: r.try_get("external_ref")?,
    })
}

/// Every hull, by owner then id.
pub async fn list_hulls(pool: &SqlitePool) -> Result<Vec<Hull>> {
    sqlx::query(&format!("{SELECT_HULL} ORDER BY h.owner_id, h.id"))
        .fetch_all(pool)
        .await?
        .iter()
        .map(row_to_hull)
        .collect()
}

pub async fn find_hull(pool: &SqlitePool, id: i64) -> Result<Option<Hull>> {
    sqlx::query(&format!("{SELECT_HULL} WHERE h.id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(row_to_hull)
        .transpose()
}

async fn nodes_in(conn: &mut SqliteConnection) -> Result<Vec<Node>> {
    let rows: Vec<(i64, String, Option<String>, Option<i64>)> =
        sqlx::query_as("SELECT id, status, location, carried_in FROM hulls").fetch_all(&mut *conn).await?;
    rows.into_iter()
        .map(|(id, s, location, carried_in)| {
            let status = HullStatus::parse(&s).ok_or_else(|| anyhow!("bad hull status {s}"))?;
            Ok(Node { id, status, location, carried_in })
        })
        .collect()
}

pub async fn create_hull(pool: &SqlitePool, h: &HullInput) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO hulls (owner_id, ship_id, model, nickname, kind, status, location, carried_in, loaner, loaner_for,
                            insurance_claim, insurance_type, notes)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(h.owner_id)
    .bind(h.ship_id)
    .bind(&h.model)
    .bind(h.nickname.as_deref())
    .bind(h.kind.as_str())
    .bind(h.status.as_str())
    .bind(h.location.as_deref())
    .bind(h.carried_in)
    .bind(h.loaner as i64)
    .bind(h.loaner_for)
    .bind(h.insurance_claim)
    .bind(h.insurance_type.as_deref())
    .bind(h.notes.as_deref())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// The edit form. Destroying a carrier destroys what it carries (see `set_destroyed`).
pub async fn update_hull(pool: &SqlitePool, id: i64, h: &HullInput) -> Result<()> {
    let mut tx = pool.begin().await?;
    let before: Option<String> = sqlx::query_scalar("SELECT status FROM hulls WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let before = before.ok_or_else(|| anyhow!("no such hull"))?;
    sqlx::query(
        "UPDATE hulls SET owner_id = ?, ship_id = ?, model = ?, nickname = ?, kind = ?, status = ?, location = ?,
            carried_in = ?, loaner = ?, loaner_for = ?, insurance_claim = ?, insurance_type = ?, notes = ?,
            updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(h.owner_id)
    .bind(h.ship_id)
    .bind(&h.model)
    .bind(h.nickname.as_deref())
    .bind(h.kind.as_str())
    .bind(h.status.as_str())
    .bind(h.location.as_deref())
    .bind(h.carried_in)
    .bind(h.loaner as i64)
    .bind(h.loaner_for)
    .bind(h.insurance_claim)
    .bind(h.insurance_type.as_deref())
    .bind(h.notes.as_deref())
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if h.status == HullStatus::Destroyed && before != HullStatus::Destroyed.as_str() {
        set_destroyed(&mut tx, id).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// `id` destroyed, and everything riding in it with it (a Pisces in an
/// Ironclad that blows up needs its own claim). All of them unloaded.
async fn set_destroyed(conn: &mut SqliteConnection, id: i64) -> Result<()> {
    let nodes = nodes_in(conn).await?;
    let mut ids = contents(&nodes, id);
    ids.push(id);
    for h in ids {
        sqlx::query(
            "UPDATE hulls SET status = 'destroyed', carried_in = NULL, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(h)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// A quick action. `Ok(Err(msg))` is a refusal to show the user.
pub async fn apply_action(
    pool: &SqlitePool,
    id: i64,
    action: Action,
    location: Option<&str>,
) -> Result<std::result::Result<HullStatus, String>> {
    let mut tx = pool.begin().await?;
    let nodes = nodes_in(&mut tx).await?;
    let Some(r) = resolve(&nodes, id) else { return Err(anyhow!("no such hull")) };
    let carried = !r.carriers.is_empty();
    let to = match apply(r.status, action, carried) {
        Ok(s) => s,
        Err(e) => return Ok(Err(e)),
    };
    if to == HullStatus::Destroyed {
        set_destroyed(&mut tx, id).await?;
    } else {
        // an action on a carried hull unloads it; a blank location keeps
        // the one it has (or its carrier's, when it was riding)
        let loc = location.map(str::trim).filter(|l| !l.is_empty()).map(String::from).or(r.location);
        sqlx::query(
            "UPDATE hulls SET status = ?, location = ?, carried_in = NULL, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(to.as_str())
        .bind(loc)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Ok(to))
}

/// Removes a hull. What it carried stays where it was: moved into the
/// hull's own carrier, or given its status and location.
pub async fn delete_hull(pool: &SqlitePool, id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    delete_in(&mut tx, id).await?;
    tx.commit().await?;
    Ok(())
}

async fn delete_in(conn: &mut SqliteConnection, id: i64) -> Result<()> {
    let nodes = nodes_in(conn).await?;
    let Some(me) = nodes.iter().find(|n| n.id == id) else { return Ok(()) };
    let r = resolve(&nodes, id).ok_or_else(|| anyhow!("no such hull"))?;
    match me.carried_in {
        Some(outer) => {
            sqlx::query("UPDATE hulls SET carried_in = ? WHERE carried_in = ?")
                .bind(outer)
                .bind(id)
                .execute(&mut *conn)
                .await?;
        }
        None => {
            sqlx::query("UPDATE hulls SET carried_in = NULL, status = ?, location = ? WHERE carried_in = ?")
                .bind(r.status.as_str())
                .bind(r.location)
                .bind(id)
                .execute(&mut *conn)
                .await?;
        }
    }
    sqlx::query("DELETE FROM hulls WHERE id = ?").bind(id).execute(&mut *conn).await?;
    Ok(())
}

/// The pledge was delivered and its loaners go back: removes every loaner
/// lent for `pledge`. Returns how many.
pub async fn remove_loaners(pool: &SqlitePool, pledge: i64) -> Result<usize> {
    let mut tx = pool.begin().await?;
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM hulls WHERE loaner = 1 AND loaner_for = ?")
        .bind(pledge)
        .fetch_all(&mut *tx)
        .await?;
    for id in &ids {
        delete_in(&mut tx, *id).await?;
    }
    tx.commit().await?;
    Ok(ids.len())
}

/// The pledge was delivered and its loaners stay (bought outright, say):
/// they become ordinary hulls.
pub async fn keep_loaners(pool: &SqlitePool, pledge: i64) -> Result<u64> {
    Ok(sqlx::query("UPDATE hulls SET loaner = 0, loaner_for = NULL, updated_at = datetime('now') WHERE loaner_for = ?")
        .bind(pledge)
        .execute(pool)
        .await?
        .rows_affected())
}

/// The hull flying a voyage goes where the voyage is: pulled out at the
/// route stop it last reached (unloaded from any carrier; what it carries
/// follows). A hull on order or destroyed is left alone, as is a voyage
/// with no hull. Called by `stowage::repo` inside its own transactions
/// (new run, Next stop, Edit voyage).
pub async fn follow_voyage_in(conn: &mut SqliteConnection, voyage_id: i64) -> Result<()> {
    let row: Option<(Option<i64>, i64)> = sqlx::query_as("SELECT hull_id, at_stop FROM voyages WHERE id = ?")
        .bind(voyage_id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some((Some(hull), at)) = row else { return Ok(()) };
    let stop: Option<String> = sqlx::query_scalar("SELECT terminal FROM voyage_stops WHERE voyage_id = ? AND seq = ?")
        .bind(voyage_id)
        .bind(at)
        .fetch_optional(&mut *conn)
        .await?;
    let stop = stop.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    sqlx::query(
        "UPDATE hulls SET status = 'out', location = COALESCE(?, location), carried_in = NULL, updated_at = datetime('now')
         WHERE id = ? AND status IN ('stored', 'out')",
    )
    .bind(stop)
    .bind(hull)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Locations already used on hulls (for the location pick-list, next to
/// the terminals), alphabetical.
pub async fn location_names(pool: &SqlitePool) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT DISTINCT location FROM hulls WHERE location IS NOT NULL AND TRIM(location) <> '' ORDER BY location COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?)
}

/// Models typed in that aren't on the ships list.
pub async fn off_list_models(pool: &SqlitePool) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar("SELECT DISTINCT model FROM hulls WHERE ship_id IS NULL ORDER BY model COLLATE NOCASE")
        .fetch_all(pool)
        .await?)
}

/// `(hulls, pulled out)` for the splash. Carried hulls count where their
/// carrier is.
pub async fn counts(pool: &SqlitePool) -> Result<(usize, usize)> {
    let mut conn = pool.acquire().await?;
    let nodes = nodes_in(&mut conn).await?;
    let out = nodes
        .iter()
        .filter(|n| resolve(&nodes, n.id).is_some_and(|r| r.status == HullStatus::Out))
        .count();
    Ok((nodes.len(), out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::stowage::model::{parse_boxes, LoadKind, PriceMode, Whereabouts};
    use crate::stowage::repo as stowage;

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

    fn input(owner: i64, model: &str, status: HullStatus, loc: Option<&str>) -> HullInput {
        HullInput {
            owner_id: owner,
            ship_id: None,
            model: model.into(),
            nickname: None,
            kind: HullKind::Ship,
            status,
            location: loc.map(String::from),
            carried_in: None,
            loaner: false,
            loaner_for: None,
            insurance_claim: None,
            insurance_type: None,
            notes: None,
        }
    }

    async fn ship_id(pool: &SqlitePool, name: &str) -> i64 {
        stowage::find_ship_by_name(pool, name).await.unwrap().unwrap().id
    }

    #[tokio::test]
    async fn a_hull_round_trips_with_its_model_from_the_list() {
        let (pool, gig, _) = test_pool().await;
        let mut h = input(gig, "ironclad", HullStatus::Stored, Some("Area18 TDD"));
        h.ship_id = Some(ship_id(&pool, "Ironclad").await);
        h.nickname = Some("Big Iron".into());
        h.insurance_claim = Some(120_000.0);
        h.insurance_type = Some("LTI".into());
        let id = create_hull(&pool, &h).await.unwrap();
        let got = find_hull(&pool, id).await.unwrap().unwrap();
        assert_eq!(got.model, "Ironclad", "the list's spelling wins");
        assert_eq!(got.capacity_scu, Some(2204));
        assert_eq!(got.label(), "Big Iron (Ironclad)");
        assert_eq!(got.owner_name, "Lord Gig");
        assert_eq!(got.insurance_claim, Some(120_000.0));
        assert_eq!((got.source, got.external_ref), (None, None));
        // an off-list model is kept as typed, with no capacity
        let id = create_hull(&pool, &input(gig, "Ursa Rover", HullStatus::Stored, None)).await.unwrap();
        let got = find_hull(&pool, id).await.unwrap().unwrap();
        assert_eq!((got.model.as_str(), got.capacity_scu), ("Ursa Rover", None));
        assert_eq!(off_list_models(&pool).await.unwrap(), vec!["Ursa Rover".to_string()]);
    }

    #[tokio::test]
    async fn quick_actions_store_what_the_rules_decide() {
        let (pool, gig, _) = test_pool().await;
        let id = create_hull(&pool, &input(gig, "Hull-C", HullStatus::Planned, None)).await.unwrap();
        assert!(apply_action(&pool, id, Action::PullOut, Some("Area18 TDD")).await.unwrap().is_err());
        assert_eq!(apply_action(&pool, id, Action::Delivered, Some("Area18 TDD")).await.unwrap(), Ok(HullStatus::Stored));
        apply_action(&pool, id, Action::PullOut, Some("Lorville TDD")).await.unwrap().unwrap();
        let h = find_hull(&pool, id).await.unwrap().unwrap();
        assert_eq!((h.status, h.location.as_deref()), (HullStatus::Out, Some("Lorville TDD")));
        // a blank location keeps the one it has
        apply_action(&pool, id, Action::Store, Some(" ")).await.unwrap().unwrap();
        let h = find_hull(&pool, id).await.unwrap().unwrap();
        assert_eq!((h.status, h.location.as_deref()), (HullStatus::Stored, Some("Lorville TDD")));
        apply_action(&pool, id, Action::Destroyed, None).await.unwrap().unwrap();
        apply_action(&pool, id, Action::Claim, Some("Everus Harbor")).await.unwrap().unwrap();
        let h = find_hull(&pool, id).await.unwrap().unwrap();
        assert_eq!((h.status, h.location.as_deref()), (HullStatus::Stored, Some("Everus Harbor")));
        assert_eq!(counts(&pool).await.unwrap(), (1, 0));
    }

    #[tokio::test]
    async fn carried_hulls_follow_their_carrier_and_go_down_with_it() {
        let (pool, gig, _) = test_pool().await;
        let iron = create_hull(&pool, &input(gig, "Ironclad", HullStatus::Out, Some("Area18 TDD"))).await.unwrap();
        let mut p = input(gig, "C8 Pisces", HullStatus::Stored, Some("Levski"));
        p.carried_in = Some(iron);
        let pisces = create_hull(&pool, &p).await.unwrap();
        assert_eq!(counts(&pool).await.unwrap(), (2, 2), "the Pisces is out with the Ironclad");
        // storing the carrier stores its contents where it is
        apply_action(&pool, iron, Action::Store, Some("Lorville TDD")).await.unwrap().unwrap();
        let hulls = list_hulls(&pool).await.unwrap();
        let nodes: Vec<Node> = hulls.iter().map(Hull::node).collect();
        let r = resolve(&nodes, pisces).unwrap();
        assert_eq!((r.status, r.location.as_deref()), (HullStatus::Stored, Some("Lorville TDD")));
        // pulling the Pisces out unloads it
        apply_action(&pool, pisces, Action::PullOut, None).await.unwrap().unwrap();
        let h = find_hull(&pool, pisces).await.unwrap().unwrap();
        assert_eq!((h.status, h.location.as_deref(), h.carried_in), (HullStatus::Out, Some("Lorville TDD"), None));
        // load it again and blow the carrier up: both need a claim
        sqlx::query("UPDATE hulls SET carried_in = ? WHERE id = ?").bind(iron).bind(pisces).execute(&pool).await.unwrap();
        apply_action(&pool, iron, Action::PullOut, Some("Lorville TDD")).await.unwrap().unwrap();
        apply_action(&pool, iron, Action::Destroyed, None).await.unwrap().unwrap();
        let h = find_hull(&pool, pisces).await.unwrap().unwrap();
        assert_eq!((h.status, h.carried_in), (HullStatus::Destroyed, None));
    }

    #[tokio::test]
    async fn removing_a_carrier_leaves_its_contents_where_they_were() {
        let (pool, gig, _) = test_pool().await;
        let iron = create_hull(&pool, &input(gig, "Ironclad", HullStatus::Out, Some("Area18 TDD"))).await.unwrap();
        let mut p = input(gig, "C8 Pisces", HullStatus::Stored, Some("Levski"));
        p.carried_in = Some(iron);
        let pisces = create_hull(&pool, &p).await.unwrap();
        delete_hull(&pool, iron).await.unwrap();
        let h = find_hull(&pool, pisces).await.unwrap().unwrap();
        assert_eq!((h.status, h.location.as_deref(), h.carried_in), (HullStatus::Out, Some("Area18 TDD"), None));
    }

    #[tokio::test]
    async fn loaners_go_or_stay_with_their_pledge() {
        let (pool, gig, _) = test_pool().await;
        let pledge = create_hull(&pool, &input(gig, "Hull-D [Concept]", HullStatus::Planned, None)).await.unwrap();
        let mut l = input(gig, "Hull-C", HullStatus::Stored, Some("Area18 TDD"));
        l.loaner = true;
        l.loaner_for = Some(pledge);
        create_hull(&pool, &l).await.unwrap();
        create_hull(&pool, &l).await.unwrap();
        let other = create_hull(&pool, &input(gig, "Ironclad", HullStatus::Stored, None)).await.unwrap();
        assert_eq!(remove_loaners(&pool, pledge).await.unwrap(), 2);
        assert_eq!(list_hulls(&pool).await.unwrap().len(), 2);
        let k = create_hull(&pool, &l).await.unwrap();
        assert_eq!(keep_loaners(&pool, pledge).await.unwrap(), 1);
        let h = find_hull(&pool, k).await.unwrap().unwrap();
        assert!(!h.loaner && h.loaner_for.is_none());
        // removing the pledge itself unlinks (the FK sets NULL), never cascades
        let mut l2 = input(gig, "Hull-C", HullStatus::Stored, None);
        l2.loaner = true;
        l2.loaner_for = Some(pledge);
        let l2 = create_hull(&pool, &l2).await.unwrap();
        delete_hull(&pool, pledge).await.unwrap();
        let h = find_hull(&pool, l2).await.unwrap().unwrap();
        assert!(h.loaner && h.loaner_for.is_none());
        assert!(find_hull(&pool, other).await.unwrap().is_some());
    }

    fn load(dest_stop: u32) -> stowage::NewLoad {
        stowage::NewLoad {
            kind: LoadKind::Trade,
            commodity: "Laranite".into(),
            buy_terminal: String::new(),
            price_mode: PriceMode::Scu,
            buy_price_scu: 100.0,
            buy_price_lot: 800.0,
            pickup_stop: 0,
            drop_stop: Some(dest_stop),
            destination: None,
            boxes: parse_boxes("1x8").unwrap(),
            reward: None,
            whereabouts: Whereabouts::Aboard,
            stored_at: None,
            notes: None,
        }
    }

    #[tokio::test]
    async fn a_voyage_takes_its_hull_along_the_route() {
        let (pool, gig, _) = test_pool().await;
        let iron_ship = ship_id(&pool, "Ironclad").await;
        let mut h = input(gig, "Ironclad", HullStatus::Stored, Some("Levski"));
        h.ship_id = Some(iron_ship);
        let iron = create_hull(&pool, &h).await.unwrap();
        let mut p = input(gig, "C8 Pisces", HullStatus::Stored, Some("Grim HEX"));
        p.carried_in = Some(iron);
        let pisces = create_hull(&pool, &p).await.unwrap();
        let v = stowage::NewVoyage {
            ship_id: Some(iron_ship),
            hull_id: Some(iron),
            bought_at: "2026-11-10".into(),
            notes: None,
            route: vec!["ArcCorp Mining Area 141".into(), "Area18 TDD".into(), "Lorville TDD".into()],
        };
        let vid = stowage::create_voyage(&pool, gig, gig, &v, &[load(2)]).await.unwrap();
        let got = find_hull(&pool, iron).await.unwrap().unwrap();
        assert_eq!((got.status, got.location.as_deref()), (HullStatus::Out, Some("ArcCorp Mining Area 141")));
        stowage::advance_voyage(&pool, vid).await.unwrap();
        let hulls = list_hulls(&pool).await.unwrap();
        let nodes: Vec<Node> = hulls.iter().map(Hull::node).collect();
        assert_eq!(resolve(&nodes, iron).unwrap().location.as_deref(), Some("Area18 TDD"));
        assert_eq!(resolve(&nodes, pisces).unwrap().location.as_deref(), Some("Area18 TDD"), "the Pisces rides along");
        let voyage = stowage::find_voyage(&pool, vid).await.unwrap().unwrap();
        assert_eq!(voyage.hull_id, Some(iron));
        // a destroyed hull isn't dragged back out by the voyage
        apply_action(&pool, iron, Action::Destroyed, None).await.unwrap().unwrap();
        stowage::advance_voyage(&pool, vid).await.unwrap();
        assert_eq!(find_hull(&pool, iron).await.unwrap().unwrap().status, HullStatus::Destroyed);
        // removing the hull leaves the voyage flying no hull, ship kept
        delete_hull(&pool, iron).await.unwrap();
        let voyage = stowage::find_voyage(&pool, vid).await.unwrap().unwrap();
        assert_eq!(voyage.hull_id, None);
        assert_eq!(voyage.ship.map(|s| s.name), Some("Ironclad".to_string()));
    }

    #[tokio::test]
    async fn voyages_from_before_hulls_keep_working() {
        // a voyage written the old way (no hull_id column value) reads back
        // with no hull and its ship, and advancing it touches no hull
        let (pool, gig, _) = test_pool().await;
        let hull_c = ship_id(&pool, "Hull-C").await;
        let vid: i64 = sqlx::query_scalar(
            "INSERT INTO voyages (logged_by, ship_id, bought_at) VALUES (?, ?, '2026-11-01') RETURNING id",
        )
        .bind(gig)
        .bind(hull_c)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO voyage_stops (voyage_id, seq, terminal) VALUES (?, 0, 'Levski'), (?, 1, 'Grim HEX')")
            .bind(vid)
            .bind(vid)
            .execute(&pool)
            .await
            .unwrap();
        let other = create_hull(&pool, &input(gig, "Hull-C", HullStatus::Stored, Some("Levski"))).await.unwrap();
        stowage::advance_voyage(&pool, vid).await.unwrap();
        let v = stowage::find_voyage(&pool, vid).await.unwrap().unwrap();
        assert_eq!((v.hull_id, v.at_stop), (None, 1));
        assert_eq!(v.ship.unwrap().capacity_scu, 4608);
        assert_eq!(find_hull(&pool, other).await.unwrap().unwrap().status, HullStatus::Stored);
    }
}
