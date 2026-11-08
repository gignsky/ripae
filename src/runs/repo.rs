use anyhow::{anyhow, Result};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use super::model::{CloseRun, EditRun, NewRun, Run, Status};

fn row_to_run(row: SqliteRow) -> Result<Run> {
    let status: String = row.try_get("status")?;
    Ok(Run {
        id: row.try_get("id")?,
        submitted_by: row.try_get("submitted_by")?,
        status: status
            .parse::<Status>()
            .map_err(|e| anyhow!("bad status in db: {e}"))?,
        bought_at: row.try_get("bought_at")?,
        commodity: row.try_get("commodity")?,
        scu: row.try_get("scu")?,
        buy_terminal: row.try_get("buy_terminal")?,
        buy_price_scu: row.try_get("buy_price_scu")?,
        destination: row.try_get("destination")?,
        stored_at: row.try_get("stored_at")?,
        ship: row.try_get("ship")?,
        sold_at: row.try_get("sold_at")?,
        sell_terminal: row.try_get("sell_terminal")?,
        sell_price_scu: row.try_get("sell_price_scu")?,
        scu_sold: row.try_get("scu_sold")?,
        notes: row.try_get("notes")?,
    })
}

const SELECT_RUN: &str = "SELECT id, submitted_by, status, bought_at, commodity, scu,
    buy_terminal, buy_price_scu, destination, stored_at, ship, sold_at,
    sell_terminal, sell_price_scu, scu_sold, notes FROM runs";

pub async fn create(pool: &SqlitePool, submitted_by: i64, new_run: &NewRun) -> Result<i64> {
    let commodity = super::model::normalise_name(&new_run.commodity);
    let ship = new_run.ship.as_deref().map(super::model::normalise_name);
    let row = sqlx::query(
        "INSERT INTO runs (submitted_by, bought_at, commodity, scu, buy_terminal, buy_price_scu, destination, ship)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(submitted_by)
    .bind(&new_run.bought_at)
    .bind(&commodity)
    .bind(new_run.scu)
    .bind(&new_run.buy_terminal)
    .bind(new_run.buy_price_scu)
    .bind(new_run.destination.as_deref())
    .bind(ship.as_deref())
    .fetch_one(pool)
    .await?;
    Ok(row.try_get("id")?)
}

pub async fn list_all(pool: &SqlitePool) -> Result<Vec<Run>> {
    let rows = sqlx::query(SELECT_RUN).fetch_all(pool).await?;
    rows.into_iter().map(row_to_run).collect()
}

pub async fn find(pool: &SqlitePool, id: i64) -> Result<Option<Run>> {
    let row = sqlx::query(&format!("{SELECT_RUN} WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.map(row_to_run).transpose()
}

pub async fn update(pool: &SqlitePool, id: i64, edit: &EditRun) -> Result<()> {
    let commodity = super::model::normalise_name(&edit.commodity);
    let ship = edit.ship.as_deref().map(super::model::normalise_name);
    sqlx::query(
        "UPDATE runs SET status = ?, bought_at = ?, commodity = ?, scu = ?, buy_terminal = ?,
            buy_price_scu = ?, destination = ?, stored_at = ?, ship = ?, notes = ?,
            updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(edit.status.as_str())
    .bind(&edit.bought_at)
    .bind(&commodity)
    .bind(edit.scu)
    .bind(&edit.buy_terminal)
    .bind(edit.buy_price_scu)
    .bind(edit.destination.as_deref())
    .bind(edit.stored_at.as_deref())
    .bind(ship.as_deref())
    .bind(edit.notes.as_deref())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn close(pool: &SqlitePool, id: i64, close: &CloseRun) -> Result<()> {
    sqlx::query(
        "UPDATE runs SET status = 'sold', sold_at = ?, sell_terminal = ?, sell_price_scu = ?,
            scu_sold = ?, updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(&close.sold_at)
    .bind(&close.sell_terminal)
    .bind(close.sell_price_scu)
    .bind(close.scu_sold)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM runs WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    async fn test_pool_with_user() -> (SqlitePool, i64) {
        let pool = db::connect(":memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        let row = sqlx::query(
            "INSERT INTO users (handle, display_name, password_hash) VALUES ('lordgig', 'Lord Gig', 'x') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        (pool, row.try_get("id").unwrap())
    }

    #[tokio::test]
    async fn create_then_find_round_trips_and_normalises_names() {
        let (pool, user_id) = test_pool_with_user().await;
        let new_run = NewRun {
            bought_at: "2026-01-01".into(),
            commodity: "  laranite  ".into(),
            scu: 100.0,
            buy_terminal: "ArcCorp Mining Area 141".into(),
            buy_price_scu: 10.0,
            destination: Some("Everus Harbor".into()),
            ship: Some("hull a".into()),
        };
        let id = create(&pool, user_id, &new_run).await.unwrap();
        let run = find(&pool, id).await.unwrap().unwrap();
        assert_eq!(run.commodity, "Laranite");
        assert_eq!(run.ship, Some("Hull A".to_string()));
        assert_eq!(run.status, Status::InTransit);
        assert!(run.status.is_open());
    }

    #[tokio::test]
    async fn close_flips_status_to_sold() {
        let (pool, user_id) = test_pool_with_user().await;
        let id = create(
            &pool,
            user_id,
            &NewRun {
                bought_at: "2026-01-01".into(),
                commodity: "Laranite".into(),
                scu: 100.0,
                buy_terminal: "ArcCorp Mining Area 141".into(),
                buy_price_scu: 10.0,
                destination: None,
                ship: None,
            },
        )
        .await
        .unwrap();

        close(
            &pool,
            id,
            &CloseRun {
                sold_at: "2026-01-02".into(),
                sell_terminal: "Everus Harbor".into(),
                sell_price_scu: 15.0,
                scu_sold: 100.0,
            },
        )
        .await
        .unwrap();

        let run = find(&pool, id).await.unwrap().unwrap();
        assert_eq!(run.status, Status::Sold);
        assert_eq!(run.profit(), Some(500.0));
    }

    #[tokio::test]
    async fn delete_removes_the_run() {
        let (pool, user_id) = test_pool_with_user().await;
        let id = create(
            &pool,
            user_id,
            &NewRun {
                bought_at: "2026-01-01".into(),
                commodity: "Laranite".into(),
                scu: 100.0,
                buy_terminal: "ArcCorp Mining Area 141".into(),
                buy_price_scu: 10.0,
                destination: None,
                ship: None,
            },
        )
        .await
        .unwrap();
        delete(&pool, id).await.unwrap();
        assert!(find(&pool, id).await.unwrap().is_none());
    }
}
