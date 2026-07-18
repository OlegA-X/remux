use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// A single row in the admin-facing activity log.
///
/// `user_id` is optional because some events (e.g. failed auth) are not tied
/// to a known account. Custom columns (`ip_address`, `device_id`, `client`)
/// are stored flat and projected into the Jellyfin-shaped DTO by the API
/// handler rather than serialized directly.
#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct ActivityLog {
    pub id: i64,
    pub log_type: String,
    pub name: String,
    pub short_overview: Option<String>,
    pub user_id: Option<Uuid>,
    pub date: String,
    pub severity: String,
    pub ip_address: Option<String>,
    pub device_id: Option<String>,
    pub client: Option<String>,
}

/// Input for [`ActivityLog::record`]. Everything except `log_type` / `name`
/// is optional; `date` and `severity` default to "now" / "Info".
#[derive(Debug, Clone, Default)]
pub struct ActivityLogEntry {
    pub log_type: String,
    pub name: String,
    pub short_overview: Option<String>,
    pub user_id: Option<Uuid>,
    pub ip_address: Option<String>,
    pub device_id: Option<String>,
    pub client: Option<String>,
}

/// Filter for [`ActivityLog::get_by_filter`].
#[derive(Debug, Clone, default2::Default, Serialize, Deserialize)]
pub struct ActivityLogFilter {
    pub user_id: Option<Uuid>,
    pub log_type: Option<String>,
    /// Only entries at or after this RFC3339 timestamp.
    pub since: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub total_count: bool,
}

impl ActivityLog {
    /// Insert a new activity-log entry. Returns the new row id.
    pub async fn record(db: &SqlitePool, entry: &ActivityLogEntry) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO activity_log (log_type, name, short_overview, user_id, date, severity, ip_address, device_id, client)
            VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), COALESCE(NULLIF(?, ''), 'Info'), ?, ?, ?)
            "#,
        )
        .bind(&entry.log_type)
        .bind(&entry.name)
        .bind(&entry.short_overview)
        .bind(entry.user_id)
        .bind("Info")
        .bind(&entry.ip_address)
        .bind(&entry.device_id)
        .bind(&entry.client)
        .execute(db)
        .await?;
        Ok(result.last_insert_rowid())
    }

    /// Query activity-log rows matching `filter`, newest first.
    pub async fn get_by_filter(
        db: &SqlitePool,
        filter: &ActivityLogFilter,
    ) -> Result<crate::db::FilterResult<Self>> {
        let mut count_qb =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM activity_log WHERE 1=1");
        let mut records_qb =
            sqlx::QueryBuilder::new("SELECT * FROM activity_log WHERE 1=1");

        for qb in [&mut count_qb, &mut records_qb] {
            if let Some(user_id) = &filter.user_id {
                qb.push(" AND user_id = ")
                    .push_bind(user_id);
            }
            if let Some(log_type) = &filter.log_type {
                qb.push(" AND log_type = ")
                    .push_bind(log_type);
            }
            if let Some(since) = &filter.since {
                qb.push(" AND date >= ")
                    .push_bind(since);
            }
        }

        records_qb.push(" ORDER BY id DESC");

        if let Some(limit) = &filter.limit {
            records_qb
                .push(" LIMIT ")
                .push_bind(limit);
        }
        if let Some(offset) = &filter.offset {
            records_qb
                .push(" OFFSET ")
                .push_bind(offset);
        }

        let (count, records) = tokio::join!(
            async {
                count_qb
                    .build()
                    .fetch_one(db)
                    .await
                    .map(|r| r.get::<i64, _>(0) as usize)
            },
            async {
                records_qb
                    .build_query_as::<Self>()
                    .fetch_all(db)
                    .await
            }
        );

        Ok(crate::db::FilterResult {
            records: records?,
            total_count: if filter.total_count { count? } else { 0 },
        })
    }
}
