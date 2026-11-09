//! Pick-lists (commodities, terminals, ships) from migration 0002. Since
//! 0.2 they are *suggestions, not gates* (ruled 8 Nov 2026): a name on the
//! list is stored with the list's spelling, any other name is stored as
//! typed (trimmed). The 0.4 UEX import is where names get reconciled.
//! // TODO(0.4): UEX import fills `uex_id` and reconciles off-list names.

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

/// The list's own spelling of `raw` when it matches (trimmed,
/// case-insensitive via the column's NOCASE collation), else `raw` trimmed.
/// Never refuses a name.
pub async fn canonical(pool: &SqlitePool, kind: Kind, raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    let sql = format!("SELECT name FROM {} WHERE name = ?", kind.table());
    let hit: Option<String> = sqlx::query_scalar(&sql)
        .bind(trimmed)
        .fetch_optional(pool)
        .await?;
    Ok(hit.unwrap_or_else(|| trimmed.to_string()))
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
            "ArcCorp Mining Area 141"
        );
        assert_eq!(canonical(&pool, Kind::Ship, "hull-c").await.unwrap(), "Hull-C");
    }

    #[tokio::test]
    async fn canonical_keeps_unknown_names_as_typed() {
        let pool = test_pool().await;
        assert_eq!(
            canonical(&pool, Kind::Commodity, "  Unobtanium ").await.unwrap(),
            "Unobtanium"
        );
        assert_eq!(
            canonical(&pool, Kind::Terminal, "Lorville CBD").await.unwrap(),
            "Lorville CBD"
        );
    }

    #[tokio::test]
    async fn names_are_seeded_and_sorted() {
        let pool = test_pool().await;
        let ships = names(&pool, Kind::Ship).await.unwrap();
        assert_eq!(ships.len(), 80);
        assert!(ships.contains(&"Hull-C".to_string()));
        let mut sorted = ships.clone();
        sorted.sort_by_key(|s| s.to_lowercase());
        assert_eq!(ships, sorted);
    }
}
