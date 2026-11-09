//! Fixed pick-lists (commodities, terminals, ships) from migration 0002.
//! Forms may only submit names that exist here; the stored spelling is
//! always the table's, whatever case the user typed.

use anyhow::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Commodity,
    Terminal,
    Ship,
}

impl Kind {
    fn table(&self) -> &'static str {
        match self {
            Kind::Commodity => "commodities",
            Kind::Terminal => "terminals",
            Kind::Ship => "ships",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Kind::Commodity => "commodity",
            Kind::Terminal => "terminal",
            Kind::Ship => "ship",
        }
    }
}

/// Every name of `kind`, alphabetical -- feeds the page's pick-lists.
pub async fn names(pool: &SqlitePool, kind: Kind) -> Result<Vec<String>> {
    let sql = format!("SELECT name FROM {} ORDER BY name", kind.table());
    Ok(sqlx::query_scalar(&sql).fetch_all(pool).await?)
}

/// The table's own spelling of `raw` (trimmed, case-insensitive match via
/// the column's NOCASE collation), or `None` if it isn't on the list.
pub async fn canonical(pool: &SqlitePool, kind: Kind, raw: &str) -> Result<Option<String>> {
    let sql = format!("SELECT name FROM {} WHERE name = ?", kind.table());
    Ok(sqlx::query_scalar(&sql)
        .bind(raw.trim())
        .fetch_optional(pool)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    async fn test_pool() -> SqlitePool {
        let pool = db::connect(":memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn canonical_matches_case_insensitively_and_returns_table_spelling() {
        let pool = test_pool().await;
        assert_eq!(
            canonical(&pool, Kind::Terminal, "  arccorp mining area 141 ")
                .await
                .unwrap(),
            Some("ArcCorp Mining Area 141".to_string())
        );
        assert_eq!(
            canonical(&pool, Kind::Ship, "raft").await.unwrap(),
            Some("RAFT".to_string())
        );
    }

    #[tokio::test]
    async fn canonical_rejects_unknown_names() {
        let pool = test_pool().await;
        assert_eq!(
            canonical(&pool, Kind::Commodity, "Unobtanium").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn names_are_seeded_and_sorted() {
        let pool = test_pool().await;
        let ships = names(&pool, Kind::Ship).await.unwrap();
        assert!(ships.contains(&"Hull A".to_string()));
        let mut sorted = ships.clone();
        sorted.sort_by_key(|s| s.to_lowercase());
        assert_eq!(ships, sorted);
    }
}
