use super::*;
use serde_json::json;
use sqlx::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "dcli-store-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    async fn open(&self) -> ActivityStoreInterface {
        let mut store = ActivityStoreInterface::init_with_path(
            &self.0,
            Some("offline-test".to_owned()),
        )
        .await
        .unwrap();
        store.fix_corrupt_data = false;
        store
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn report(id: i64, weapon: u32) -> DestinyPostGameCarnageReportData {
    let mut values = serde_json::Map::new();
    for name in [
        "assists",
        "score",
        "kills",
        "deaths",
        "completed",
        "opponentsDefeated",
        "efficiency",
        "killsDeathsRatio",
        "killsDeathsAssists",
        "activityDurationSeconds",
        "standing",
        "team",
        "completionReason",
        "startSeconds",
        "timePlayedSeconds",
        "playerCount",
        "teamScore",
        "fireteamId",
    ] {
        values.insert(name.to_owned(), json!({"basic": {"value": 1}}));
    }
    serde_json::from_value(json!({
        "period": "2026-01-01T12:00:00Z",
        "activityDetails": {"referenceId": 1, "directorActivityHash": 1,
            "instanceId": id.to_string(), "mode": 71, "modes": [5, 71], "isPrivate": false, "membershipType": 3},
        "teams": [{"teamId": 1, "teamName": "Alpha", "score": {"basic": {"value": 100}}, "standing": {"basic": {"value": 0}}}],
        "entries": [{"characterId": "100", "score": {"basic": {"value": 10}}, "standing": 0, "values": values,
            "player": {
                "destinyUserInfo": {"crossSaveOverride": 3, "isPublic": true, "membershipType": 3, "membershipId": "10",
                    "displayName": "Fixture", "bungieGlobalDisplayName": "Fixture", "bungieGlobalDisplayNameCode": 1234},
                "classHash": 3655393761u32, "raceHash": 0, "genderHash": 0, "characterLevel": 50, "lightLevel": 2000, "emblemHash": 1},
            "extended": {"values": {"precisionKills": {"basic": {"value": 2}}},
                "weapons": [{"referenceId": weapon, "values": {"uniqueWeaponKills": {"basic": {"value": 3}}}}]}
        }]
    })).unwrap()
}

async fn seed_queue(store: &mut ActivityStoreInterface, ids: &[i64]) {
    sqlx::raw_sql(
        "INSERT INTO member(member_id, platform_id) VALUES (10,3);
        INSERT INTO character(character_id,member,class) VALUES (100,10,0);
        INSERT INTO sync(member,last_sync) VALUES (10,'2026-01-01T00:00:00Z');",
    )
    .execute(&mut store.db)
    .await
    .unwrap();
    for id in ids {
        sqlx::query(
            "INSERT INTO activity_queue(activity_id,character) VALUES (?,100)",
        )
        .bind(id)
        .execute(&mut store.db)
        .await
        .unwrap();
    }
}

async fn snapshot(
    db: &mut SqliteConnection,
) -> Vec<(String, Vec<Vec<String>>)> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
    )
    .fetch_all(&mut *db)
    .await
    .unwrap();
    let mut out = Vec::new();
    for table in tables {
        let columns = sqlx::query(&format!("PRAGMA table_info(\"{}\")", table))
            .fetch_all(&mut *db)
            .await
            .unwrap();
        let quoted: Vec<String> = columns
            .iter()
            .map(|row| format!("quote(\"{}\")", row.get::<String, _>("name")))
            .collect();
        let rows = sqlx::query(&format!(
            "SELECT {} FROM \"{}\" ORDER BY rowid",
            quoted.join(","),
            table
        ))
        .fetch_all(&mut *db)
        .await
        .unwrap();
        out.push((
            table,
            rows.iter()
                .map(|row| {
                    (0..quoted.len()).map(|i| row.get::<String, _>(i)).collect()
                })
                .collect(),
        ));
    }
    out
}

async fn assert_indexed_reads(store: &mut ActivityStoreInterface) {
    for query in [
        "SELECT activity_id FROM activity_queue WHERE character=100 AND synced=0 ORDER BY activity_id DESC",
        "SELECT team_id,score,standing FROM team_result WHERE activity=1 ORDER BY standing ASC",
    ] {
        let plan = sqlx::query(&format!("EXPLAIN QUERY PLAN {}", query)).fetch_all(&mut store.db).await.unwrap();
        let details: Vec<String> = plan.iter().map(|row| row.get("detail")).collect();
        assert!(details.iter().any(|line| line.contains("SEARCH") && line.contains("INDEX")), "Expected indexed lookup: {:?}", details);
        assert!(!details.iter().any(|line| line.starts_with("SCAN ")), "{:?}", details);
    }
}

#[sqlx::test]
async fn additive_indexes_preserve_existing_history_and_reopen_idempotently() {
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    seed_queue(&mut store, &[1, 2, 3]).await;
    assert_eq!(
        store
            .insert_activities_batch(&mut [report(1, 9000)], &100)
            .await
            .unwrap(),
        1
    );
    sqlx::raw_sql(
        "DROP INDEX IF EXISTS activity_queue_pending_character_index;
        DROP INDEX IF EXISTS team_result_activity_index;
        INSERT INTO character(character_id,member,class) VALUES (101,10,1);
        INSERT INTO activity_queue(activity_id,character) VALUES (1,101);",
    )
    .execute(&mut store.db)
    .await
    .unwrap();
    let before = snapshot(&mut store.db).await;
    store.db.close().await.unwrap();
    for _ in 0..2 {
        let mut reopened = directory.open().await;
        assert_eq!(snapshot(&mut reopened.db).await, before);
        let ids: Vec<i64> = sqlx::query_scalar("SELECT activity_id FROM activity_queue WHERE character=100 AND synced=0 ORDER BY activity_id DESC")
            .fetch_all(&mut reopened.db).await.unwrap();
        assert_eq!(ids, vec![3, 2]);
        assert_indexed_reads(&mut reopened).await;
        reopened.db.close().await.unwrap();
    }
}

#[sqlx::test]
async fn fresh_store_has_indexed_reads_and_empty_sync_does_not_call_api() {
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    assert_indexed_reads(&mut store).await;
    assert_eq!(store.sync_activities(&100).await.unwrap().total_synced, 0);
    assert_eq!(
        store
            .sync_activities_with_progress(&100, &|_| {})
            .await
            .unwrap()
            .total_synced,
        0
    );
    store.db.close().await.unwrap();
}

#[sqlx::test]
async fn failed_activity_rolls_back_all_rows_and_can_retry_after_reopen() {
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    seed_queue(&mut store, &[1, 2]).await;
    sqlx::raw_sql("CREATE TRIGGER reject_weapon BEFORE INSERT ON weapon_result
        WHEN NEW.reference_id=9001 BEGIN SELECT RAISE(ABORT, 'fixture write rejected'); END;")
        .execute(&mut store.db).await.unwrap();
    assert_eq!(
        store
            .insert_activities_batch(
                &mut [report(1, 9001), report(2, 9002)],
                &100
            )
            .await
            .unwrap(),
        1
    );
    store.db.close().await.unwrap();
    let mut store = directory.open().await;
    assert!(!store.has_activity(&1).await, "Failed report left a parent row that sync would mistake for completion");
    assert!(store.has_activity(&2).await);
    for table in ["modes", "team_result", "character_activity_stats"] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {} WHERE activity=1",
            table
        ))
        .fetch_one(&mut store.db)
        .await
        .unwrap();
        assert_eq!(count, 0, "Partial rows remain in {}", table);
    }
    let flags: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT activity_id,synced FROM activity_queue ORDER BY activity_id",
    )
    .fetch_all(&mut store.db)
    .await
    .unwrap();
    assert_eq!(flags, vec![(1, 0), (2, 1)]);
    sqlx::query("DROP TRIGGER reject_weapon")
        .execute(&mut store.db)
        .await
        .unwrap();
    assert_eq!(
        store
            .insert_activities_batch(&mut [report(1, 9001)], &100)
            .await
            .unwrap(),
        1
    );
    for table in [
        "activity",
        "team_result",
        "character_activity_stats",
        "weapon_result",
        "medal_result",
    ] {
        let count: i64 =
            sqlx::query_scalar(&format!("SELECT count(*) FROM {}", table))
                .fetch_one(&mut store.db)
                .await
                .unwrap();
        assert_eq!(count, 2, "Retry did not complete {} exactly once", table);
    }
    assert_eq!(store.sync_activities(&100).await.unwrap().total_synced, 0);
    assert_eq!(
        store
            .sync_activities_with_progress(&100, &|_| {})
            .await
            .unwrap()
            .total_synced,
        0
    );
    store.db.close().await.unwrap();
}

#[sqlx::test]
async fn failed_commit_rolls_back_batch_and_connection_remains_reusable() {
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    seed_queue(&mut store, &[1]).await;
    sqlx::raw_sql("CREATE TABLE commit_guard(activity INTEGER REFERENCES activity(activity_id) DEFERRABLE INITIALLY DEFERRED);
        CREATE TRIGGER reject_commit AFTER INSERT ON team_result BEGIN INSERT INTO commit_guard VALUES (-1); END;")
        .execute(&mut store.db).await.unwrap();
    assert!(store
        .insert_activities_batch(&mut [report(1, 9001)], &100)
        .await
        .is_err());
    assert!(
        !store.has_activity(&1).await,
        "Failed commit left the connection inside a partial transaction"
    );
    let synced: i64 = sqlx::query_scalar(
        "SELECT synced FROM activity_queue WHERE activity_id=1",
    )
    .fetch_one(&mut store.db)
    .await
    .unwrap();
    assert_eq!(synced, 0);
    sqlx::query("DROP TRIGGER reject_commit")
        .execute(&mut store.db)
        .await
        .unwrap();
    assert_eq!(
        store
            .insert_activities_batch(&mut [report(1, 9001)], &100)
            .await
            .unwrap(),
        1
    );
    store.db.close().await.unwrap();
}

#[sqlx::test]
async fn interrupted_index_installation_preserves_rows_and_retries() {
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    seed_queue(&mut store, &[1]).await;
    sqlx::raw_sql(
        "DROP INDEX IF EXISTS activity_queue_pending_character_index;
        DROP INDEX IF EXISTS team_result_activity_index;
        CREATE TABLE team_result_activity_index(value TEXT);
        INSERT INTO team_result_activity_index VALUES ('preserve');",
    )
    .execute(&mut store.db)
    .await
    .unwrap();
    let before = snapshot(&mut store.db).await;
    let result = ActivityStoreInterface::init_with_path(
        &directory.0,
        Some("offline-test".to_owned()),
    )
    .await;
    assert!(
        result.is_err(),
        "Index installation failure must be surfaced"
    );
    assert_eq!(snapshot(&mut store.db).await, before);
    sqlx::query("DROP TABLE team_result_activity_index")
        .execute(&mut store.db)
        .await
        .unwrap();
    store.db.close().await.unwrap();
    let mut store = directory.open().await;
    assert_indexed_reads(&mut store).await;
    store.db.close().await.unwrap();
}
