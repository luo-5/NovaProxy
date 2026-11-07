use crate::model::{
    AppSettings, DnsSettings, GroupType, LatencyRecord, NodeGroup, ProxyNode, RoutingSettings,
    RuleAction, RuleItem, RuleType, Subscription,
};
use chrono::Utc;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

const MIGRATION_001: &str = include_str!("migrations/001_initial.sql");
const MIGRATION_002: &str = include_str!("migrations/002_routing.sql");
const APP_SETTINGS_KEY: &str = "app_settings";
const ROUTING_SETTINGS_KEY: &str = "routing_settings";
const DNS_SETTINGS_KEY: &str = "dns_settings";

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Entity not found: {0}")]
    NotFound(String),
}

pub type StorageResult<T> = Result<T, StorageError>;

struct Migration {
    version: i32,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "001_initial",
        sql: MIGRATION_001,
    },
    Migration {
        version: 2,
        name: "002_routing",
        sql: MIGRATION_002,
    },
];

/// Thread-safe SQLite storage manager
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Opens or creates the SQLite database at the given file path and applies pending migrations
    pub fn open(path: impl AsRef<Path>) -> StorageResult<Self> {
        let path_ref = path.as_ref();
        if let Some(parent) = path_ref.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path_ref)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.initialize()?;
        Ok(db)
    }

    /// Opens an in-memory SQLite database (useful for testing)
    pub fn open_in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.initialize()?;
        Ok(db)
    }

    fn initialize(&self) -> StorageResult<()> {
        let mut conn = self.conn.lock();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                applied_at INTEGER NOT NULL
            )",
            [],
        )?;

        for migration in MIGRATIONS {
            let applied: Option<i32> = conn
                .query_row(
                    "SELECT version FROM schema_migrations WHERE version = ?1",
                    params![migration.version],
                    |row| row.get(0),
                )
                .optional()?;

            if applied.is_none() {
                let tx = conn.transaction()?;
                tx.execute_batch(migration.sql)?;
                tx.execute(
                    "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                    params![migration.version, migration.name, Utc::now().timestamp()],
                )?;
                tx.commit()?;
            }
        }

        Ok(())
    }

    /// Returns the list of applied migration versions
    pub fn applied_migrations(&self) -> StorageResult<Vec<i32>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT version FROM schema_migrations ORDER BY version ASC")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut versions = Vec::new();
        for v in rows {
            versions.push(v?);
        }
        Ok(versions)
    }

    // ==================== Subscriptions CRUD ====================

    pub fn upsert_subscription(&self, sub: &Subscription) -> StorageResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO subscriptions (
                id, name, url, enabled, user_agent, update_interval_mins,
                use_proxy, updated_at, upload_bytes, download_bytes, total_bytes,
                expire_at, home_url, sort_order
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                url = excluded.url,
                enabled = excluded.enabled,
                user_agent = excluded.user_agent,
                update_interval_mins = excluded.update_interval_mins,
                use_proxy = excluded.use_proxy,
                updated_at = excluded.updated_at,
                upload_bytes = excluded.upload_bytes,
                download_bytes = excluded.download_bytes,
                total_bytes = excluded.total_bytes,
                expire_at = excluded.expire_at,
                home_url = excluded.home_url,
                sort_order = excluded.sort_order",
            params![
                sub.id,
                sub.name,
                sub.url,
                sub.enabled as i32,
                sub.user_agent,
                sub.update_interval_mins,
                sub.use_proxy as i32,
                sub.updated_at,
                sub.upload.map(|v| v as i64),
                sub.download.map(|v| v as i64),
                sub.total.map(|v| v as i64),
                sub.expire,
                sub.home_url,
                sub.sort_order,
            ],
        )?;
        Ok(())
    }

    pub fn get_subscription(&self, id: &str) -> StorageResult<Option<Subscription>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, name, url, enabled, user_agent, update_interval_mins,
                    use_proxy, updated_at, upload_bytes, download_bytes, total_bytes,
                    expire_at, home_url, sort_order
             FROM subscriptions WHERE id = ?1",
            params![id],
            Self::map_subscription_row,
        )
        .optional()
        .map_err(StorageError::from)
    }

    pub fn list_subscriptions(&self) -> StorageResult<Vec<Subscription>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, url, enabled, user_agent, update_interval_mins,
                    use_proxy, updated_at, upload_bytes, download_bytes, total_bytes,
                    expire_at, home_url, sort_order
             FROM subscriptions ORDER BY sort_order ASC, created_at ASC",
        )?;
        let rows = stmt.query_map([], Self::map_subscription_row)?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row?);
        }
        Ok(list)
    }

    pub fn delete_subscription(&self, id: &str) -> StorageResult<bool> {
        let conn = self.conn.lock();
        let affected = conn.execute("DELETE FROM subscriptions WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    fn map_subscription_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Subscription> {
        let enabled_int: i32 = row.get(3)?;
        let use_proxy_int: i32 = row.get(6)?;
        let upload_i64: Option<i64> = row.get(8)?;
        let download_i64: Option<i64> = row.get(9)?;
        let total_i64: Option<i64> = row.get(10)?;

        Ok(Subscription {
            id: row.get(0)?,
            name: row.get(1)?,
            url: row.get(2)?,
            enabled: enabled_int != 0,
            user_agent: row.get(4)?,
            update_interval_mins: row.get(5)?,
            use_proxy: use_proxy_int != 0,
            updated_at: row.get(7)?,
            upload: upload_i64.map(|v| v.max(0) as u64),
            download: download_i64.map(|v| v.max(0) as u64),
            total: total_i64.map(|v| v.max(0) as u64),
            expire: row.get(11)?,
            home_url: row.get(12)?,
            sort_order: row.get(13)?,
        })
    }

    // ==================== Groups CRUD ====================

    pub fn upsert_group(&self, group: &NodeGroup) -> StorageResult<()> {
        let node_ids_json = serde_json::to_string(&group.node_ids)?;
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO groups (id, name, group_type, node_ids_json, test_url, interval_secs, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                group_type = excluded.group_type,
                node_ids_json = excluded.node_ids_json,
                test_url = excluded.test_url,
                interval_secs = excluded.interval_secs,
                sort_order = excluded.sort_order",
            params![
                group.id,
                group.name,
                group.group_type.as_str(),
                node_ids_json,
                group.test_url,
                group.interval_secs,
                group.sort_order,
            ],
        )?;
        Ok(())
    }

    pub fn get_group(&self, id: &str) -> StorageResult<Option<NodeGroup>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, name, group_type, node_ids_json, test_url, interval_secs, sort_order
             FROM groups WHERE id = ?1",
            params![id],
            Self::map_group_row,
        )
        .optional()
        .map_err(StorageError::from)
    }

    pub fn list_groups(&self) -> StorageResult<Vec<NodeGroup>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, group_type, node_ids_json, test_url, interval_secs, sort_order
             FROM groups ORDER BY sort_order ASC, created_at ASC",
        )?;
        let rows = stmt.query_map([], Self::map_group_row)?;
        let mut list = Vec::new();
        for row in rows {
            list.push(row?);
        }
        Ok(list)
    }

    pub fn delete_group(&self, id: &str) -> StorageResult<bool> {
        let conn = self.conn.lock();
        let affected = conn.execute("DELETE FROM groups WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    fn map_group_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NodeGroup> {
        let group_type_str: String = row.get(2)?;
        let node_ids_json: String = row.get(3)?;
        let node_ids: Vec<String> = serde_json::from_str(&node_ids_json).unwrap_or_default();

        Ok(NodeGroup {
            id: row.get(0)?,
            name: row.get(1)?,
            group_type: GroupType::from_str_lossy(&group_type_str),
            node_ids,
            test_url: row.get(4)?,
            interval_secs: row.get(5)?,
            sort_order: row.get(6)?,
        })
    }

    // ==================== Nodes CRUD ====================

    pub fn upsert_node(&self, node: &ProxyNode) -> StorageResult<()> {
        let fingerprint = node.compute_fingerprint();
        let node_json = serde_json::to_string(node)?;
        let now = Utc::now().timestamp();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO nodes (
                id, name, server, port, protocol, subscription_id, group_id,
                fingerprint, latency_ms, sort_order, node_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                server = excluded.server,
                port = excluded.port,
                protocol = excluded.protocol,
                subscription_id = excluded.subscription_id,
                group_id = excluded.group_id,
                fingerprint = excluded.fingerprint,
                latency_ms = excluded.latency_ms,
                sort_order = excluded.sort_order,
                node_json = excluded.node_json,
                updated_at = excluded.updated_at",
            params![
                node.id,
                node.name,
                node.server,
                node.port,
                node.protocol.as_str(),
                node.subscription_id,
                node.group_id,
                fingerprint,
                node.latency,
                node.sort_order,
                node_json,
                now,
            ],
        )?;
        Ok(())
    }

    pub fn get_node(&self, id: &str) -> StorageResult<Option<ProxyNode>> {
        let conn = self.conn.lock();
        let row: Option<(String, Option<i64>, i32)> = conn
            .query_row(
                "SELECT node_json, latency_ms, sort_order FROM nodes WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;

        match row {
            Some((json_str, latency_ms, sort_order)) => {
                let mut node: ProxyNode = serde_json::from_str(&json_str)?;
                node.latency = latency_ms;
                node.sort_order = sort_order;
                Ok(Some(node))
            }
            None => Ok(None),
        }
    }

    pub fn list_nodes(&self) -> StorageResult<Vec<ProxyNode>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT node_json, latency_ms, sort_order
             FROM nodes ORDER BY sort_order ASC, created_at ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, i32>(2)?,
            ))
        })?;
        let mut nodes = Vec::new();
        for row in rows {
            let (json_str, latency_ms, sort_order) = row?;
            let mut node: ProxyNode = serde_json::from_str(&json_str)?;
            node.latency = latency_ms;
            node.sort_order = sort_order;
            nodes.push(node);
        }
        Ok(nodes)
    }

    pub fn list_nodes_by_subscription(
        &self,
        subscription_id: &str,
    ) -> StorageResult<Vec<ProxyNode>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT node_json, latency_ms, sort_order
             FROM nodes WHERE subscription_id = ?1 ORDER BY sort_order ASC, created_at ASC",
        )?;
        let rows = stmt.query_map(params![subscription_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, i32>(2)?,
            ))
        })?;
        let mut nodes = Vec::new();
        for row in rows {
            let (json_str, latency_ms, sort_order) = row?;
            let mut node: ProxyNode = serde_json::from_str(&json_str)?;
            node.latency = latency_ms;
            node.sort_order = sort_order;
            nodes.push(node);
        }
        Ok(nodes)
    }

    pub fn delete_node(&self, id: &str) -> StorageResult<bool> {
        let conn = self.conn.lock();
        let affected = conn.execute("DELETE FROM nodes WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// Incrementally synchronizes nodes for a subscription inside a single transaction:
    /// - Matches existing nodes by deterministic SHA-256 fingerprint
    /// - Preserves existing node `id`, custom `name`, `sort_order`, `latency`, and `group_id`
    /// - Removes obsolete nodes no longer present in the subscription
    pub fn sync_subscription_nodes(
        &self,
        subscription_id: &str,
        incoming_nodes: Vec<ProxyNode>,
    ) -> StorageResult<Vec<ProxyNode>> {
        let existing = self.list_nodes_by_subscription(subscription_id)?;
        let merged =
            crate::parser::merge_subscription_nodes(subscription_id, &existing, incoming_nodes);

        let keep_ids: std::collections::HashSet<&str> =
            merged.iter().map(|n| n.id.as_str()).collect();

        let now = Utc::now().timestamp();
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;

        for old in &existing {
            if !keep_ids.contains(old.id.as_str()) {
                tx.execute("DELETE FROM nodes WHERE id = ?1", params![old.id])?;
            }
        }

        for node in &merged {
            let fingerprint = node.compute_fingerprint();
            let node_json = serde_json::to_string(node)?;
            tx.execute(
                "INSERT INTO nodes (
                    id, name, server, port, protocol, subscription_id, group_id,
                    fingerprint, latency_ms, sort_order, node_json, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    server = excluded.server,
                    port = excluded.port,
                    protocol = excluded.protocol,
                    subscription_id = excluded.subscription_id,
                    group_id = excluded.group_id,
                    fingerprint = excluded.fingerprint,
                    latency_ms = excluded.latency_ms,
                    sort_order = excluded.sort_order,
                    node_json = excluded.node_json,
                    updated_at = excluded.updated_at",
                params![
                    node.id,
                    node.name,
                    node.server,
                    node.port,
                    node.protocol.as_str(),
                    node.subscription_id,
                    node.group_id,
                    fingerprint,
                    node.latency,
                    node.sort_order,
                    node_json,
                    now,
                ],
            )?;
        }

        tx.commit()?;
        Ok(merged)
    }

    /// Imports manually added or clipboard-pasted nodes with fingerprint deduplication
    pub fn import_manual_nodes(
        &self,
        incoming_nodes: Vec<ProxyNode>,
    ) -> StorageResult<Vec<ProxyNode>> {
        let deduped = crate::parser::deduplicate_nodes(incoming_nodes);
        let all_existing = self.list_nodes()?;
        let mut existing_manual_by_fp = std::collections::HashMap::new();
        let mut max_sort_order = all_existing
            .iter()
            .map(|n| n.sort_order)
            .max()
            .unwrap_or(-1);

        for node in &all_existing {
            if node.subscription_id.is_none() {
                existing_manual_by_fp.insert(node.compute_fingerprint(), node);
            }
        }

        let mut saved = Vec::with_capacity(deduped.len());
        for mut node in deduped {
            let fp = node.compute_fingerprint();
            if let Some(prev) = existing_manual_by_fp.get(&fp) {
                node.id = prev.id.clone();
                if !prev.name.trim().is_empty() {
                    node.name = prev.name.clone();
                }
                node.sort_order = prev.sort_order;
                node.latency = prev.latency;
                node.group_id = prev.group_id.clone();
            } else {
                max_sort_order += 1;
                node.sort_order = max_sort_order;
            }
            self.upsert_node(&node)?;
            saved.push(node);
        }

        Ok(saved)
    }

    // ==================== Latency History & Updates ====================

    /// Records a new latency measurement and updates the node's latest latency column
    pub fn record_latency(
        &self,
        node_id: &str,
        latency_ms: Option<i64>,
        test_type: &str,
        error_msg: Option<&str>,
    ) -> StorageResult<LatencyRecord> {
        let now = Utc::now().timestamp_millis();
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;

        tx.execute(
            "UPDATE nodes SET latency_ms = ?1, updated_at = ?2 WHERE id = ?3",
            params![latency_ms, now / 1000, node_id],
        )?;

        tx.execute(
            "INSERT INTO latency_history (node_id, tested_at, latency_ms, test_type, error_msg)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![node_id, now, latency_ms, test_type, error_msg],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;

        Ok(LatencyRecord {
            id,
            node_id: node_id.to_string(),
            tested_at: now,
            latency_ms,
            test_type: test_type.to_string(),
            error_msg: error_msg.map(ToString::to_string),
        })
    }

    pub fn get_latency_history(
        &self,
        node_id: &str,
        limit: usize,
    ) -> StorageResult<Vec<LatencyRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, node_id, tested_at, latency_ms, test_type, error_msg
             FROM latency_history
             WHERE node_id = ?1
             ORDER BY tested_at DESC, id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![node_id, limit as i64], |r| {
            Ok(LatencyRecord {
                id: r.get(0)?,
                node_id: r.get(1)?,
                tested_at: r.get(2)?,
                latency_ms: r.get(3)?,
                test_type: r.get(4)?,
                error_msg: r.get(5)?,
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    // ==================== Settings CRUD ====================

    pub fn get_app_settings(&self) -> StorageResult<AppSettings> {
        let conn = self.conn.lock();
        let raw: Option<String> = conn
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                params![APP_SETTINGS_KEY],
                |r| r.get(0),
            )
            .optional()?;

        match raw {
            Some(json_str) => Ok(serde_json::from_str(&json_str)?),
            None => Ok(AppSettings::default()),
        }
    }

    pub fn save_app_settings(&self, settings: &AppSettings) -> StorageResult<()> {
        let json_str = serde_json::to_string(settings)?;
        let now = Utc::now().timestamp();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO settings (key, value_json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
                value_json = excluded.value_json,
                updated_at = excluded.updated_at",
            params![APP_SETTINGS_KEY, json_str, now],
        )?;
        Ok(())
    }

    // ==================== Routing Rules CRUD ====================

    pub fn upsert_routing_rule(&self, rule: &RuleItem) -> StorageResult<()> {
        let (action_str, action_target) = match &rule.action {
            RuleAction::Proxy => ("proxy", None),
            RuleAction::Direct => ("direct", None),
            RuleAction::Block => ("block", None),
            RuleAction::Tag(t) => ("tag", Some(t.clone())),
        };

        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO routing_rules (
                id, enabled, rule_type, pattern, action, action_target, description, sort_order
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                enabled = excluded.enabled,
                rule_type = excluded.rule_type,
                pattern = excluded.pattern,
                action = excluded.action,
                action_target = excluded.action_target,
                description = excluded.description,
                sort_order = excluded.sort_order",
            params![
                rule.id,
                rule.enabled as i32,
                rule.rule_type.as_str(),
                rule.pattern,
                action_str,
                action_target,
                rule.description,
                rule.sort_order,
            ],
        )?;
        Ok(())
    }

    pub fn list_routing_rules(&self) -> StorageResult<Vec<RuleItem>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, enabled, rule_type, pattern, action, action_target, description, sort_order
             FROM routing_rules ORDER BY sort_order ASC, created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let enabled_int: i32 = row.get(1)?;
            let rule_type_str: String = row.get(2)?;
            let action_str: String = row.get(4)?;
            let action_target: Option<String> = row.get(5)?;

            let action = match action_str.as_str() {
                "direct" => RuleAction::Direct,
                "block" | "reject" => RuleAction::Block,
                "tag" => RuleAction::Tag(action_target.unwrap_or_default()),
                _ => RuleAction::Proxy,
            };

            Ok(RuleItem {
                id: row.get(0)?,
                enabled: enabled_int != 0,
                rule_type: RuleType::from_str_lossy(&rule_type_str),
                pattern: row.get(3)?,
                action,
                description: row.get(6)?,
                sort_order: row.get(7)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_routing_rule(&self, id: &str) -> StorageResult<bool> {
        let conn = self.conn.lock();
        let affected = conn.execute("DELETE FROM routing_rules WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    pub fn reorder_routing_rules(&self, ids: &[String]) -> StorageResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        for (idx, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE routing_rules SET sort_order = ?1 WHERE id = ?2",
                params![idx as i32, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    // ==================== Routing & DNS Settings ====================

    pub fn get_routing_settings(&self) -> StorageResult<RoutingSettings> {
        let conn = self.conn.lock();
        let raw: Option<String> = conn
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                params![ROUTING_SETTINGS_KEY],
                |r| r.get(0),
            )
            .optional()?;
        drop(conn);

        let mut settings = match raw {
            Some(json_str) => serde_json::from_str::<RoutingSettings>(&json_str)?,
            None => RoutingSettings::default(),
        };

        // Attach custom rules from dedicated table
        settings.custom_rules = self.list_routing_rules()?;
        Ok(settings)
    }

    pub fn save_routing_settings(&self, settings: &RoutingSettings) -> StorageResult<()> {
        let json_str = serde_json::to_string(settings)?;
        let now = Utc::now().timestamp();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO settings (key, value_json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
                value_json = excluded.value_json,
                updated_at = excluded.updated_at",
            params![ROUTING_SETTINGS_KEY, json_str, now],
        )?;
        Ok(())
    }

    pub fn get_dns_settings(&self) -> StorageResult<DnsSettings> {
        let conn = self.conn.lock();
        let raw: Option<String> = conn
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                params![DNS_SETTINGS_KEY],
                |r| r.get(0),
            )
            .optional()?;

        match raw {
            Some(json_str) => Ok(serde_json::from_str(&json_str)?),
            None => Ok(DnsSettings::default()),
        }
    }

    pub fn save_dns_settings(&self, settings: &DnsSettings) -> StorageResult<()> {
        let json_str = serde_json::to_string(settings)?;
        let now = Utc::now().timestamp();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO settings (key, value_json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
                value_json = excluded.value_json,
                updated_at = excluded.updated_at",
            params![DNS_SETTINGS_KEY, json_str, now],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        CoreType, ProtocolConfig, ProtocolType, RoutingMode, ShadowsocksConfig, TlsConfig,
        TransportConfig, VlessConfig,
    };
    use std::collections::BTreeMap;

    fn sample_node(id: &str, sub_id: Option<&str>) -> ProxyNode {
        ProxyNode {
            id: id.to_string(),
            name: format!("Test Node {}", id),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: ProtocolType::Vless,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("example.invalid".to_string()),
                insecure: false,
                alpn: vec!["h2".to_string()],
                fingerprint: Some("chrome".to_string()),
                reality: None,
                ech_config: None,
                fragment: None,
            }),
            transport: Some(TransportConfig::Ws {
                path: "/ws".to_string(),
                host: Some("example.invalid".to_string()),
                headers: BTreeMap::new(),
                max_early_data: None,
                early_data_header_name: None,
            }),
            settings: ProtocolConfig::Vless(VlessConfig {
                uuid: "00000000-0000-0000-0000-000000000000".to_string(),
                flow: None,
                encryption: "none".to_string(),
                packet_encoding: Some("xudp".to_string()),
            }),
            subscription_id: sub_id.map(ToString::to_string),
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 1,
        }
    }

    #[test]
    fn test_migrations_and_settings_crud() {
        let db = Database::open_in_memory().expect("open in-memory db");
        assert_eq!(db.applied_migrations().unwrap(), vec![1, 2]);

        let default_settings = db.get_app_settings().unwrap();
        assert_eq!(default_settings.mixed_port, 7890);
        assert_eq!(default_settings.routing_mode, RoutingMode::Rule);

        let mut updated = default_settings.clone();
        updated.mixed_port = 10808;
        updated.preferred_core = Some(CoreType::SingBox);
        updated.routing_mode = RoutingMode::Global;
        db.save_app_settings(&updated).unwrap();

        let loaded = db.get_app_settings().unwrap();
        assert_eq!(loaded.mixed_port, 10808);
        assert_eq!(loaded.preferred_core, Some(CoreType::SingBox));
        assert_eq!(loaded.routing_mode, RoutingMode::Global);

        // Test Routing Rules CRUD
        let rule1 = RuleItem {
            id: "r1".to_string(),
            enabled: true,
            rule_type: RuleType::DomainSuffix,
            pattern: "google.com".to_string(),
            action: RuleAction::Proxy,
            description: Some("Google Proxy".to_string()),
            sort_order: 0,
        };
        let rule2 = RuleItem {
            id: "r2".to_string(),
            enabled: true,
            rule_type: RuleType::IpCidr,
            pattern: "192.168.1.0/24".to_string(),
            action: RuleAction::Direct,
            description: None,
            sort_order: 1,
        };
        db.upsert_routing_rule(&rule1).unwrap();
        db.upsert_routing_rule(&rule2).unwrap();

        let rules = db.list_routing_rules().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].pattern, "google.com");
        assert_eq!(rules[1].action, RuleAction::Direct);

        db.reorder_routing_rules(&["r2".to_string(), "r1".to_string()])
            .unwrap();
        let reordered = db.list_routing_rules().unwrap();
        assert_eq!(reordered[0].id, "r2");
        assert_eq!(reordered[1].id, "r1");

        assert!(db.delete_routing_rule("r1").unwrap());
        assert_eq!(db.list_routing_rules().unwrap().len(), 1);

        // Test Routing Settings & DNS Settings
        let mut r_settings = db.get_routing_settings().unwrap();
        assert!(r_settings.cn_direct);
        r_settings.cn_direct = false;
        db.save_routing_settings(&r_settings).unwrap();
        let loaded_r = db.get_routing_settings().unwrap();
        assert!(!loaded_r.cn_direct);
        assert_eq!(loaded_r.custom_rules.len(), 1);

        let mut dns_settings = db.get_dns_settings().unwrap();
        assert!(dns_settings.fake_ip);
        dns_settings.fake_ip = false;
        db.save_dns_settings(&dns_settings).unwrap();
        let loaded_dns = db.get_dns_settings().unwrap();
        assert!(!loaded_dns.fake_ip);
    }

    #[test]
    fn test_subscriptions_nodes_groups_and_latency_cascade() {
        let db = Database::open_in_memory().expect("open in-memory db");

        let sub = Subscription {
            id: "sub-1".to_string(),
            name: "Test Sub".to_string(),
            url: "https://example.invalid/sub".to_string(),
            enabled: true,
            user_agent: Some("clash.meta".to_string()),
            update_interval_mins: Some(60),
            use_proxy: false,
            updated_at: Some(1700000000),
            upload: Some(1024),
            download: Some(2048),
            total: Some(1048576),
            expire: Some(1800000000),
            home_url: None,
            sort_order: 0,
        };
        db.upsert_subscription(&sub).unwrap();
        assert_eq!(db.list_subscriptions().unwrap().len(), 1);

        let node1 = sample_node("node-1", Some("sub-1"));
        let mut node2 = sample_node("node-2", None);
        node2.protocol = ProtocolType::Shadowsocks;
        node2.settings = ProtocolConfig::Shadowsocks(ShadowsocksConfig {
            method: "2022-blake3-aes-128-gcm".to_string(),
            password: "dummy-password-not-real".to_string(),
            plugin: None,
            plugin_opts: None,
            uot: Some(true),
        });

        db.upsert_node(&node1).unwrap();
        db.upsert_node(&node2).unwrap();

        assert_eq!(db.list_nodes().unwrap().len(), 2);
        assert_eq!(db.list_nodes_by_subscription("sub-1").unwrap().len(), 1);

        let group = NodeGroup {
            id: "group-1".to_string(),
            name: "Auto Best".to_string(),
            group_type: GroupType::UrlTest,
            node_ids: vec!["node-1".to_string(), "node-2".to_string()],
            test_url: Some("https://www.gstatic.com/generate_204".to_string()),
            interval_secs: Some(300),
            sort_order: 0,
        };
        db.upsert_group(&group).unwrap();
        let loaded_group = db.get_group("group-1").unwrap().unwrap();
        assert_eq!(loaded_group.group_type, GroupType::UrlTest);
        assert_eq!(loaded_group.node_ids.len(), 2);

        // Record latency measurements
        db.record_latency("node-1", Some(45), "tcp", None).unwrap();
        db.record_latency("node-1", Some(38), "url", None).unwrap();

        let updated_node1 = db.get_node("node-1").unwrap().unwrap();
        assert_eq!(updated_node1.latency, Some(38));

        let history = db.get_latency_history("node-1", 10).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].latency_ms, Some(38));
        assert_eq!(history[1].latency_ms, Some(45));

        // Deleting subscription should cascade-delete node-1 and its latency history
        assert!(db.delete_subscription("sub-1").unwrap());
        assert!(db.get_node("node-1").unwrap().is_none());
        assert!(db.get_latency_history("node-1", 10).unwrap().is_empty());
        assert!(db.get_node("node-2").unwrap().is_some());
    }
}
