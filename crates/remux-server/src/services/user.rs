use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// Window used to decide whether a device session is "active" (matches the
/// SessionsCard default in the dashboard UI — 16 minutes).
const ACTIVE_WINDOW_MINUTES: i64 = 16;

/// Aggregated statistics for a single user, shown on the user-detail page.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserStats {
    pub user_id: Uuid,
    pub username: String,
    pub is_admin: bool,
    pub is_disabled: bool,
    pub total_plays: i64,
    pub played_items: i64,
    pub favorite_items: i64,
    pub resume_items: i64,
    /// Sum of in-progress positions (seconds). An approximation of watch time
    /// based on what is stored in `user_media_state.playback_position`.
    pub watch_time_seconds: i64,
    pub last_played_at: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub active_device_count: i64,
}

/// One row of "recently played" history for a user.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserRecentItem {
    pub media_id: Uuid,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub play_count: i64,
    pub playback_position: i64,
    pub favorite: bool,
    pub last_played_at: Option<DateTime<Utc>>,
}

/// A leaderboard entry: a user + their global play count.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TopUserStat {
    pub user_id: Uuid,
    pub username: String,
    pub total_plays: i64,
    pub last_activity_at: Option<DateTime<Utc>>,
}

/// Dashboard-wide user statistics, shown as the dashboard widget.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UsersOverviewStats {
    pub total_users: i64,
    pub admin_users: i64,
    pub disabled_users: i64,
    pub active_24h: i64,
    pub active_7d: i64,
    pub total_plays: i64,
    pub top_users: Vec<TopUserStat>,
}

pub struct UserService;

impl UserService {
    /// Compute per-user aggregates (plays, played/favorite counts, watch time,
    /// last activity/login timestamps, active device count).
    pub async fn user_stats(db: &SqlitePool, user_id: &Uuid) -> Result<UserStats> {
        let since = Utc::now() - Duration::minutes(ACTIVE_WINDOW_MINUTES);

        let agg = sqlx::query(
            r#"
            SELECT
                u.id                                        AS user_id,
                u.username                                  AS username,
                u.is_admin                                  AS is_admin,
                COALESCE(json_extract(u.policy, '$.IsDisabled'), 0) AS is_disabled,
                COALESCE(SUM(s.play_count), 0)              AS total_plays,
                COUNT(CASE WHEN s.play_count > 0 THEN 1 END) AS played_items,
                COUNT(CASE WHEN s.favorite   = 1 THEN 1 END) AS favorite_items,
                COUNT(CASE WHEN s.playback_position > 0 THEN 1 END) AS resume_items,
                COALESCE(SUM(s.playback_position), 0)       AS watch_time_seconds,
                MAX(s.last_played_at)                       AS last_played_at,
                u.last_login_at                             AS last_login_at,
                u.last_activity_at                          AS last_activity_at,
                COALESCE((
                    SELECT COUNT(*) FROM devices d
                    WHERE d.user_id = u.id AND d.last_activity_at >= ?
                ), 0)                                        AS active_device_count
            FROM users u
            LEFT JOIN user_media_state s ON s.user_id = u.id
            WHERE u.id = ?
            GROUP BY u.id
            "#,
        )
        .bind(since)
        .bind(user_id)
        .fetch_one(db)
        .await?;

        Ok(UserStats {
            user_id: agg
                .try_get::<Uuid, _>("user_id")
                .unwrap_or(*user_id),
            username: agg
                .try_get("username")
                .unwrap_or_default(),
            is_admin: agg
                .try_get::<i64, _>("is_admin")
                .unwrap_or(0)
                != 0,
            is_disabled: agg
                .try_get::<i64, _>("is_disabled")
                .unwrap_or(0)
                != 0,
            total_plays: agg
                .try_get("total_plays")
                .unwrap_or(0),
            played_items: agg
                .try_get("played_items")
                .unwrap_or(0),
            favorite_items: agg
                .try_get("favorite_items")
                .unwrap_or(0),
            resume_items: agg
                .try_get("resume_items")
                .unwrap_or(0),
            watch_time_seconds: agg
                .try_get("watch_time_seconds")
                .unwrap_or(0),
            last_played_at: agg
                .try_get::<Option<String>, _>("last_played_at")
                .ok()
                .flatten()
                .and_then(|s| {
                    s.parse()
                        .ok()
                }),
            last_login_at: agg
                .try_get::<Option<String>, _>("last_login_at")
                .ok()
                .flatten()
                .and_then(|s| {
                    s.parse()
                        .ok()
                }),
            last_activity_at: agg
                .try_get::<Option<String>, _>("last_activity_at")
                .ok()
                .flatten()
                .and_then(|s| {
                    s.parse()
                        .ok()
                }),
            active_device_count: agg
                .try_get("active_device_count")
                .unwrap_or(0),
        })
    }

    /// Up to `limit` most-recently-played items for a user (newest first).
    pub async fn recent_items(
        db: &SqlitePool,
        user_id: &Uuid,
        limit: u32,
    ) -> Result<Vec<UserRecentItem>> {
        let rows = sqlx::query(
            r#"
            SELECT s.media_id, m.title, m.kind,
                   s.play_count, s.playback_position, s.favorite, s.last_played_at
            FROM user_media_state s
            LEFT JOIN media m ON m.id = s.media_id
            WHERE s.user_id = ? AND s.last_played_at IS NOT NULL
            ORDER BY s.last_played_at DESC
            LIMIT ?
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(db)
        .await?;

        let map = |r: sqlx::sqlite::SqliteRow| UserRecentItem {
            media_id: r
                .try_get("media_id")
                .unwrap_or_default(),
            title: r
                .try_get("title")
                .ok()
                .flatten(),
            kind: r
                .try_get("kind")
                .ok()
                .flatten(),
            play_count: r
                .try_get("play_count")
                .unwrap_or(0),
            playback_position: r
                .try_get("playback_position")
                .unwrap_or(0),
            favorite: r
                .try_get("favorite")
                .unwrap_or(false),
            last_played_at: r
                .try_get::<Option<String>, _>("last_played_at")
                .ok()
                .flatten()
                .and_then(|s| {
                    s.parse()
                        .ok()
                }),
        };

        Ok(rows
            .into_iter()
            .map(map)
            .collect())
    }

    /// Dashboard-wide aggregate: total / admin / disabled / recently-active
    /// counts, total plays across all users, and the top-N leaderboard.
    pub async fn overview(db: &SqlitePool, top_n: u32) -> Result<UsersOverviewStats> {
        let now_24h = Utc::now() - Duration::hours(24);
        let now_7d = Utc::now() - Duration::days(7);

        let totals = sqlx::query(
            r#"
            SELECT
                COUNT(*)                                                                 AS total_users,
                COUNT(CASE WHEN is_admin = 1 THEN 1 END)                                 AS admin_users,
                COUNT(CASE WHEN json_extract(policy, '$.IsDisabled') = 1 THEN 1 END)     AS disabled_users,
                COUNT(CASE WHEN last_activity_at >= ?1 THEN 1 END)                       AS active_24h,
                COUNT(CASE WHEN last_activity_at >= ?2 THEN 1 END)                       AS active_7d,
                (SELECT COALESCE(SUM(play_count), 0) FROM user_media_state)              AS total_plays
            FROM users
            "#,
        )
        .bind(now_24h)
        .bind(now_7d)
        .fetch_one(db)
        .await?;

        let top = sqlx::query(
            r#"
            SELECT u.id AS user_id, u.username, COALESCE(SUM(s.play_count), 0) AS total_plays,
                   u.last_activity_at
            FROM users u
            LEFT JOIN user_media_state s ON s.user_id = u.id
            GROUP BY u.id
            ORDER BY total_plays DESC, u.username ASC
            LIMIT ?
            "#,
        )
        .bind(top_n)
        .fetch_all(db)
        .await?;

        let top_users = top
            .into_iter()
            .map(|r| TopUserStat {
                user_id: r
                    .try_get("user_id")
                    .unwrap_or_default(),
                username: r
                    .try_get("username")
                    .unwrap_or_default(),
                total_plays: r
                    .try_get("total_plays")
                    .unwrap_or(0),
                last_activity_at: r
                    .try_get::<Option<String>, _>("last_activity_at")
                    .ok()
                    .flatten()
                    .and_then(|s| {
                        s.parse()
                            .ok()
                    }),
            })
            .collect();

        Ok(UsersOverviewStats {
            total_users: totals
                .try_get("total_users")
                .unwrap_or(0),
            admin_users: totals
                .try_get("admin_users")
                .unwrap_or(0),
            disabled_users: totals
                .try_get("disabled_users")
                .unwrap_or(0),
            active_24h: totals
                .try_get("active_24h")
                .unwrap_or(0),
            active_7d: totals
                .try_get("active_7d")
                .unwrap_or(0),
            total_plays: totals
                .try_get("total_plays")
                .unwrap_or(0),
            top_users,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `recent_items` query must order newest-first and join media titles.
    /// Uses an in-memory pool with the squash migration applied.
    #[tokio::test]
    async fn recent_items_orders_newest_first() {
        let pool = test_pool().await;

        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash) VALUES (?, ?, '')",
        )
        .bind(user_id)
        .bind("recent_test")
        .execute(&pool)
        .await
        .unwrap();

        let m1 = Uuid::new_v4();
        let m2 = Uuid::new_v4();
        for (id, title) in [(m1, "A"), (m2, "B")] {
            sqlx::query(
                "INSERT INTO media (id, title, kind, created_at, updated_at) VALUES (?, ?, 'movie', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            )
            .bind(id)
            .bind(title)
            .execute(&pool)
            .await
            .unwrap();
        }

        sqlx::query(
            "INSERT INTO user_media_state (user_id, media_id, play_count, last_played_at) \
             VALUES (?, ?, 1, '2026-01-01T10:00:00Z'), (?, ?, 2, '2026-01-02T10:00:00Z')",
        )
        .bind(user_id)
        .bind(m1)
        .bind(user_id)
        .bind(m2)
        .execute(&pool)
        .await
        .unwrap();

        let recent = UserService::recent_items(&pool, &user_id, 10)
            .await
            .unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(
            recent[0]
                .title
                .as_deref(),
            Some("B")
        );
        assert_eq!(
            recent[1]
                .title
                .as_deref(),
            Some("A")
        );
    }

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .unwrap();
        // Apply just the two tables we need for these tests.
        sqlx::query(
            r#"
            CREATE TABLE users (
                id TEXT PRIMARY KEY, username TEXT UNIQUE, password_hash TEXT NOT NULL DEFAULT '',
                aio_url TEXT, configuration TEXT, is_admin INTEGER NOT NULL DEFAULT 0, policy TEXT,
                last_login_at TEXT, last_activity_at TEXT
            );
            CREATE TABLE media (id TEXT PRIMARY KEY, title TEXT, kind TEXT, created_at TEXT, updated_at TEXT);
            CREATE TABLE user_media_state (
                user_id BLOB NOT NULL, media_id BLOB NOT NULL, media_raw TEXT,
                favorite INT NOT NULL DEFAULT 0, play_count INT NOT NULL DEFAULT 0,
                played_at DATETIME, playback_position INT NOT NULL DEFAULT 0, stream_id BLOB,
                subtitle_idx INT, audio_idx INT, last_played_at DATETIME,
                PRIMARY KEY (user_id, media_id)
            );
            CREATE TABLE devices (
                user_id TEXT NOT NULL, id TEXT NOT NULL, access_token TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL, app_name TEXT NOT NULL, app_version TEXT NOT NULL,
                last_activity_at TEXT, capabilities TEXT, remote_ip TEXT,
                PRIMARY KEY (user_id, id)
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }
}
