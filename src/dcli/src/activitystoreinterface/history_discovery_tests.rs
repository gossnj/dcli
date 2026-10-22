use super::*;
use std::sync::Condvar;

#[sqlx::test]
async fn history_fetches_next_page_while_required_page_waits() {
    let next_requested = Arc::new((Mutex::new(false), Condvar::new()));
    let observed = next_requested.clone();
    let server = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(large_history(&[]));
        }
        let page = query(&path, "page").unwrap();
        if page == 1 {
            let (lock, changed) = &*observed;
            let (seen, _) = changed
                .wait_timeout_while(
                    lock.lock().unwrap(),
                    Duration::from_secs(4),
                    |seen| !*seen,
                )
                .unwrap();
            if !*seen {
                return Reply::json(json!({"ErrorCode":5,"ThrottleSeconds":0,
                    "ErrorStatus":"SystemDisabled","Message":"page two was never requested"}));
            }
        } else if page == 2 {
            let (lock, changed) = &*observed;
            *lock.lock().unwrap() = true;
            changed.notify_all();
        }
        let ids: Vec<_> = (1..=750)
            .rev()
            .skip(page as usize * 250)
            .take(250)
            .collect();
        Reply::json(large_history(&ids))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let control = SyncControl::new();
    let result = store
        .sync_member_engine(&member(), &control, &|event| {
            if event.outcome.discovery_complete {
                control.cancel();
            }
        })
        .await;
    assert!(
        result.is_ok(),
        "required page must progress while next page is fetched: {:?}",
        result
    );
    assert!(result.unwrap().recent_ready);
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT activity_id FROM sync_history ORDER BY activity_id",
    )
    .fetch_all(&mut store.db)
    .await
    .unwrap();
    assert_eq!(ids, (1..=750).collect::<Vec<_>>());
    store.db.close().await.unwrap();
}

async fn reopen_history_with_shift(
    added: i64,
    removed: std::ops::RangeInclusive<i64>,
    expect_shortcut: bool,
    interrupt_head: bool,
    metadata_change: Option<&str>,
) {
    const COUNT: i64 = 10_000;
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    let first = Server::new(|path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(large_history(&[]));
        }
        let page = query(&path, "page").unwrap() as usize;
        Reply::json(large_history(
            &(1..=COUNT)
                .rev()
                .skip(page * 250)
                .take(250)
                .collect::<Vec<_>>(),
        ))
    });
    first.attach(&mut store);
    let control = SyncControl::new();
    let partial = store
        .sync_member_engine(&member(), &control, &|event| {
            if event.outcome.discovered >= 7_500 {
                control.cancel();
            }
        })
        .await
        .unwrap();
    assert!(
        partial.cancelled
            && !partial.discovery_complete
            && partial.recent_ready
    );
    let saved_prefix: Vec<i64> =
        sqlx::query_scalar("SELECT activity_id FROM sync_history")
            .fetch_all(&mut store.db)
            .await
            .unwrap();
    if let Some(change) = metadata_change {
        sqlx::query(change).execute(&mut store.db).await.unwrap();
    }
    store.db.close().await.unwrap();
    drop(first);

    let all: Vec<i64> = (1..=COUNT + added)
        .rev()
        .filter(|id| !removed.contains(id))
        .collect();
    let expected: std::collections::BTreeSet<i64> =
        all.iter().copied().chain(saved_prefix).collect();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let second = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(large_history(&[]));
        }
        let page = query(&path, "page").unwrap() as usize;
        seen.lock().unwrap().push(page);
        Reply::json(large_history(
            &all.iter()
                .copied()
                .skip(page * 250)
                .take(250)
                .collect::<Vec<_>>(),
        ))
    });
    if interrupt_head {
        let mut head_only = directory.open().await;
        second.attach(&mut head_only);
        let stop = SyncControl::new();
        head_only
            .sync_member_engine(&member(), &stop, &|event| {
                if event.phase == SyncPhase::Discovering {
                    stop.cancel();
                }
            })
            .await
            .unwrap();
        head_only.db.close().await.unwrap();
        requests.lock().unwrap().clear();
    }
    let mut reopened = directory.open().await;
    second.attach(&mut reopened);
    let control = SyncControl::new();
    let outcome = reopened
        .sync_member_engine(&member(), &control, &|event| {
            if event.outcome.discovery_complete {
                control.cancel();
            }
        })
        .await
        .unwrap();
    assert!(outcome.discovery_complete && outcome.recent_ready);
    let actual: Vec<i64> = sqlx::query_scalar(
        "SELECT activity_id FROM sync_history ORDER BY activity_id",
    )
    .fetch_all(&mut reopened.db)
    .await
    .unwrap();
    assert_eq!(
        actual,
        expected.into_iter().collect::<Vec<_>>(),
        "resume must keep every previously saved and currently returned ID"
    );
    let pages = requests.lock().unwrap();
    assert!(
        !expect_shortcut || pages.len() <= 24,
        "resume should skip the saved 30-page prefix; fetched {} pages: {:?}",
        pages.len(),
        *pages
    );
    reopened.db.close().await.unwrap();
}

#[sqlx::test]
async fn history_probe_failure_preserves_deep_checkpoint_for_retry() {
    const COUNT: i64 = 10_000;
    let fail_probe = Arc::new(AtomicBool::new(false));
    let failing = fail_probe.clone();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let server = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(large_history(&[]));
        }
        let page = query(&path, "page").unwrap() as usize;
        recorded.lock().unwrap().push(page);
        if failing.load(Ordering::SeqCst) && page == 29 {
            return Reply::json(json!({"ErrorCode":5,"ThrottleSeconds":0,
                "ErrorStatus":"SystemDisabled","Message":"temporary fixture failure"}));
        }
        Reply::json(large_history(
            &(1..=COUNT)
                .rev()
                .skip(page * 250)
                .take(250)
                .collect::<Vec<_>>(),
        ))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let stop = SyncControl::new();
    let partial = store
        .sync_member_engine(&member(), &stop, &|event| {
            if event.outcome.discovered >= 7_500 {
                stop.cancel();
            }
        })
        .await
        .unwrap();
    assert!(
        partial.cancelled
            && partial.recent_ready
            && !partial.discovery_complete
    );
    let saved: (i64, i64, i64, String, String, i64) = sqlx::query_as(
        "SELECT version,epoch,boundary,head_raw,tail_raw,page_hint FROM sync_history_resume WHERE member=10 AND character=100 AND mode=5")
        .fetch_one(&mut store.db).await.unwrap();
    assert_eq!(saved.5, 29);
    store.db.close().await.unwrap();

    fail_probe.store(true, Ordering::SeqCst);
    let mut failed = directory.open().await;
    server.attach(&mut failed);
    let result = failed
        .sync_member_engine(&member(), &SyncControl::new(), &|_| {})
        .await;
    assert!(
        result.is_err(),
        "a service failure must remain visible to the caller"
    );
    let after_failure: Option<(i64, i64, i64, String, String, i64)> = sqlx::query_as(
        "SELECT version,epoch,boundary,head_raw,tail_raw,page_hint FROM sync_history_resume WHERE member=10 AND character=100 AND mode=5")
        .fetch_optional(&mut failed.db).await.unwrap();
    assert_eq!(
        after_failure,
        Some(saved),
        "failed probing must preserve the deep checkpoint"
    );
    failed.db.close().await.unwrap();

    fail_probe.store(false, Ordering::SeqCst);
    requests.lock().unwrap().clear();
    let mut retried = directory.open().await;
    server.attach(&mut retried);
    let stop = SyncControl::new();
    let result = retried
        .sync_member_engine(&member(), &stop, &|event| {
            if event.outcome.discovery_complete {
                stop.cancel();
            }
        })
        .await
        .unwrap();
    assert!(result.discovery_complete);
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT activity_id FROM sync_history ORDER BY activity_id",
    )
    .fetch_all(&mut retried.db)
    .await
    .unwrap();
    assert_eq!(ids, (1..=COUNT).collect::<Vec<_>>());
    assert!(
        requests.lock().unwrap().len() <= 24,
        "recovery must reuse the saved deep checkpoint"
    );
    retried.db.close().await.unwrap();
}

#[sqlx::test]
async fn history_reopen_resumes_committed_prefix() {
    reopen_history_with_shift(0, 0..=0, true, false, None).await;
}

#[sqlx::test]
async fn history_reopen_resumes_after_multiple_new_head_pages() {
    reopen_history_with_shift(600, 0..=0, true, false, None).await;
}

#[sqlx::test]
async fn history_reopen_resumes_after_removed_middle_pages() {
    reopen_history_with_shift(0, 7_001..=7_600, true, false, None).await;
}

#[sqlx::test]
#[ignore = "observational fixture benchmark; no timing threshold"]
async fn history_discovery_fixture_benchmark() {
    use std::sync::atomic::AtomicUsize;
    for balanced in [false, true] {
        for repetition in 0..3 {
            let requests = Arc::new(AtomicUsize::new(0));
            let inflight = Arc::new(AtomicUsize::new(0));
            let peak = Arc::new(AtomicUsize::new(0));
            let (r, f, p) = (requests.clone(), inflight.clone(), peak.clone());
            let server = Server::new(move |path| {
                if let Some(id) = pgcr_id(&path) {
                    return Reply::json(envelope(report_json(id, id as u32)));
                }
                if path.contains("components=") {
                    let mut value = profile();
                    if balanced {
                        for id in [200, 300] {
                            let mut character = value["Response"]["characters"]
                                ["data"]["100"]
                                .clone();
                            character["characterId"] = json!(id.to_string());
                            value["Response"]["characters"]["data"]
                                [id.to_string()] = character;
                        }
                    }
                    return Reply::json(value);
                }
                r.fetch_add(1, Ordering::SeqCst);
                let active = f.fetch_add(1, Ordering::SeqCst) + 1;
                p.fetch_max(active, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(40));
                let mode = query(&path, "mode").unwrap();
                let character: i64 = path
                    .split("Character/")
                    .nth(1)
                    .unwrap()
                    .split('/')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                let stream =
                    (character / 100 - 1) * 2 + if mode == 32 { 1 } else { 0 };
                let count = if balanced {
                    7_000
                } else if stream == 0 {
                    42_000
                } else {
                    0
                };
                let high = if balanced {
                    42_000 - stream * 7_000
                } else {
                    42_000
                };
                let page = query(&path, "page").unwrap() as usize;
                let ids: Vec<_> = ((high - count + 1)..=high)
                    .rev()
                    .skip(page * 250)
                    .take(250)
                    .collect();
                let reply = Reply::json(large_history(&ids));
                f.fetch_sub(1, Ordering::SeqCst);
                reply
            });
            for interrupted in [false, true] {
                let directory = TestDirectory::new();
                let mut store = directory.open().await;
                server.attach(&mut store);
                requests.store(0, Ordering::SeqCst);
                peak.store(0, Ordering::SeqCst);
                let start = Instant::now();
                let recent = Mutex::new(None);
                let control = SyncControl::new();
                let first = store
                    .sync_member_engine(&member(), &control, &|event| {
                        if event.outcome.recent_ready {
                            let mut time = recent.lock().unwrap();
                            if time.is_none() {
                                *time = Some(start.elapsed().as_secs_f64());
                            }
                        }
                        if event.outcome.discovery_complete
                            || (interrupted
                                && event.outcome.discovered >= 30_000)
                        {
                            control.cancel();
                        }
                    })
                    .await
                    .unwrap();
                let first_seconds = start.elapsed().as_secs_f64();
                let first_requests = requests.load(Ordering::SeqCst);
                store.db.close().await.unwrap();
                let (resume_seconds, resume_requests) = if interrupted {
                    let mut reopened = directory.open().await;
                    server.attach(&mut reopened);
                    requests.store(0, Ordering::SeqCst);
                    let resumed = Instant::now();
                    let control = SyncControl::new();
                    let outcome = reopened
                        .sync_member_engine(&member(), &control, &|event| {
                            if event.outcome.discovery_complete {
                                control.cancel();
                            }
                        })
                        .await
                        .unwrap();
                    assert!(outcome.discovery_complete);
                    assert_eq!(outcome.discovered, 42_000);
                    reopened.db.close().await.unwrap();
                    (
                        resumed.elapsed().as_secs_f64(),
                        requests.load(Ordering::SeqCst),
                    )
                } else {
                    assert_eq!(first.discovered, 42_000);
                    (0.0, 0)
                };
                println!(
                    "BENCH {}",
                    json!({"balanced":balanced,"repetition":repetition,"interrupted":interrupted,
                    "first_seconds":first_seconds,"first_requests":first_requests,"recent_seconds":*recent.lock().unwrap(),
                    "resume_seconds":resume_seconds,"resume_requests":resume_requests,"max_http_inflight":peak.load(Ordering::SeqCst)})
                );
            }
        }
    }
}

#[sqlx::test]
async fn history_reopen_falls_back_when_checkpoint_anchors_disappear() {
    reopen_history_with_shift(0, 2_501..=2_750, false, false, None).await;
}

#[sqlx::test]
async fn history_reopen_falls_back_when_saved_head_disappears() {
    reopen_history_with_shift(0, 9_751..=10_000, false, false, None).await;
}

#[sqlx::test]
async fn history_prefetch_holds_buffered_pages_inside_stream_bound() {
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let observed = gate.clone();
    let first_waiting = Arc::new(AtomicBool::new(false));
    let waiting = first_waiting.clone();
    let overflow = Arc::new(AtomicBool::new(false));
    let overflowed = overflow.clone();
    let server = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(large_history(&[]));
        }
        let page = query(&path, "page").unwrap() as usize;
        if page == 1 {
            waiting.store(true, Ordering::SeqCst);
            let (lock, changed) = &*observed;
            let _seen = changed
                .wait_timeout_while(
                    lock.lock().unwrap(),
                    Duration::from_secs(4),
                    |seen| !*seen,
                )
                .unwrap();
            std::thread::sleep(Duration::from_millis(150));
            waiting.store(false, Ordering::SeqCst);
        } else if page == 2 {
            let (lock, changed) = &*observed;
            *lock.lock().unwrap() = true;
            changed.notify_all();
        } else if page > 2 && waiting.load(Ordering::SeqCst) {
            overflowed.store(true, Ordering::SeqCst);
        }
        Reply::json(large_history(
            &(1..=1_250)
                .rev()
                .skip(page * 250)
                .take(250)
                .collect::<Vec<_>>(),
        ))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let control = SyncControl::new();
    let outcome = store
        .sync_member_engine(&member(), &control, &|event| {
            if event.outcome.discovery_complete {
                control.cancel();
            }
        })
        .await
        .unwrap();
    assert!(outcome.discovery_complete);
    assert!(!overflow.load(Ordering::SeqCst), "a decoded page must still consume its stream reservation until admission");
    store.db.close().await.unwrap();
}

#[sqlx::test]
async fn history_ignores_error_beyond_earlier_empty_page() {
    let server = Server::new(|path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(large_history(&[]));
        }
        match query(&path, "page").unwrap() {
            0 => {
                Reply::json(large_history(&(1..=250).rev().collect::<Vec<_>>()))
            }
            1 => {
                std::thread::sleep(Duration::from_millis(75));
                Reply::json(large_history(&[]))
            }
            _ => Reply::json(
                json!({"ErrorCode":5,"ThrottleSeconds":0,"ErrorStatus":"SystemDisabled","Message":"irrelevant speculative failure"}),
            ),
        }
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let control = SyncControl::new();
    let outcome = store
        .sync_member_engine(&member(), &control, &|event| {
            if event.outcome.discovery_complete {
                control.cancel();
            }
        })
        .await
        .unwrap();
    assert!(outcome.discovery_complete);
    assert_eq!(outcome.discovered, 250);
    store.db.close().await.unwrap();
}

#[sqlx::test]
async fn history_reopen_preserves_deep_checkpoint_when_head_bridge_is_interrupted(
) {
    reopen_history_with_shift(600, 0..=0, true, true, None).await;
}

#[sqlx::test]
async fn history_reopen_uses_surviving_anchors_when_one_activity_disappears() {
    reopen_history_with_shift(0, 2_501..=2_501, true, false, None).await;
}

#[sqlx::test]
async fn history_initial_frontiers_share_global_four_page_bound() {
    use std::sync::atomic::AtomicUsize;
    let started = Arc::new((Mutex::new(0usize), Condvar::new()));
    let gate = started.clone();
    let inflight = Arc::new(AtomicUsize::new(0));
    let active = inflight.clone();
    let peak = Arc::new(AtomicUsize::new(0));
    let max_active = peak.clone();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let server = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            let mut value = profile();
            for id in [200, 300] {
                let mut character =
                    value["Response"]["characters"]["data"]["100"].clone();
                character["characterId"] = json!(id.to_string());
                value["Response"]["characters"]["data"][id.to_string()] =
                    character;
            }
            return Reply::json(value);
        }
        let page = query(&path, "page").unwrap();
        recorded.lock().unwrap().push(page);
        let n = active.fetch_add(1, Ordering::SeqCst) + 1;
        max_active.fetch_max(n, Ordering::SeqCst);
        let (lock, changed) = &*gate;
        let mut count = lock.lock().unwrap();
        *count += 1;
        changed.notify_all();
        let _guard = changed
            .wait_timeout_while(count, Duration::from_secs(4), |count| {
                *count < 4
            })
            .unwrap();
        active.fetch_sub(1, Ordering::SeqCst);
        Reply::json(large_history(&[]))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let outcome = store
        .sync_member_engine(&member(), &SyncControl::new(), &|_| {})
        .await
        .unwrap();
    assert!(outcome.complete);
    assert_eq!(
        peak.load(Ordering::SeqCst),
        4,
        "frontier requests must make progress within the global limit"
    );
    assert_eq!(*requests.lock().unwrap(), vec![0; 6]);
    store.db.close().await.unwrap();
}

async fn failed_history_page_admission(checkpoint_failure: bool) {
    let server = Server::new(|path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(large_history(&[]));
        }
        let page = query(&path, "page").unwrap() as usize;
        Reply::json(large_history(
            &(1..=3_000)
                .rev()
                .skip(page * 250)
                .take(250)
                .collect::<Vec<_>>(),
        ))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    if checkpoint_failure {
        let stop = SyncControl::new();
        store
            .sync_member_engine(&member(), &stop, &|event| {
                if event.outcome.discovered >= 250 {
                    stop.cancel();
                }
            })
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER reject_history_page BEFORE UPDATE ON sync_history_resume WHEN NEW.page_hint=5 BEGIN SELECT RAISE(FAIL,'fixture checkpoint failure'); END")
            .execute(&mut store.db).await.unwrap();
    } else {
        sqlx::query("CREATE TRIGGER reject_history_page BEFORE INSERT ON sync_history WHEN NEW.activity_id=1600 BEGIN SELECT RAISE(FAIL,'fixture page failure'); END")
            .execute(&mut store.db).await.unwrap();
    }
    assert!(store
        .sync_member_engine(&member(), &SyncControl::new(), &|_| {})
        .await
        .is_err());
    let failed_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_history WHERE activity_id BETWEEN 1501 AND 1750")
        .fetch_one(&mut store.db).await.unwrap();
    let failed_queue: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_queue WHERE activity_id BETWEEN 1501 AND 1750")
        .fetch_one(&mut store.db).await.unwrap();
    assert_eq!(
        (failed_rows, failed_queue),
        (0, 0),
        "failed page admission must be atomic"
    );
    sqlx::query("DROP TRIGGER reject_history_page")
        .execute(&mut store.db)
        .await
        .unwrap();
    store.db.close().await.unwrap();
    let mut reopened = directory.open().await;
    server.attach(&mut reopened);
    let stop = SyncControl::new();
    let outcome = reopened
        .sync_member_engine(&member(), &stop, &|event| {
            if event.outcome.discovery_complete {
                stop.cancel();
            }
        })
        .await
        .unwrap();
    assert!(outcome.discovery_complete);
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT activity_id FROM sync_history ORDER BY activity_id",
    )
    .fetch_all(&mut reopened.db)
    .await
    .unwrap();
    assert_eq!(ids, (1..=3_000).collect::<Vec<_>>());
    reopened.db.close().await.unwrap();
}

#[sqlx::test]
async fn history_checks_every_stream_frontier_before_fetching_recent_reports() {
    let frontiers = Arc::new(Mutex::new(std::collections::HashSet::new()));
    let seen = frontiers.clone();
    let premature = Arc::new(AtomicBool::new(false));
    let early = premature.clone();
    let server = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            if seen.lock().unwrap().len() != 6 {
                early.store(true, Ordering::SeqCst);
            }
            let mut report = report_json(id, id as u32);
            report["entries"][0]["characterId"] =
                json!((((id - 1) / 500 + 1) * 100).to_string());
            return Reply::json(envelope(report));
        }
        if path.contains("components=") {
            let mut value = profile();
            for id in [200, 300] {
                let mut character =
                    value["Response"]["characters"]["data"]["100"].clone();
                character["characterId"] = json!(id.to_string());
                value["Response"]["characters"]["data"][id.to_string()] =
                    character;
            }
            return Reply::json(value);
        }
        let page = query(&path, "page").unwrap();
        let mode = query(&path, "mode").unwrap();
        let character: i64 = path
            .split("Character/")
            .nth(1)
            .unwrap()
            .split('/')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        if page == 0 {
            seen.lock().unwrap().insert((character, mode));
        }
        let stream = (character / 100 - 1) * 2 + if mode == 32 { 1 } else { 0 };
        let high = (stream + 1) * 250;
        let ids: Vec<_> = ((high - 249)..=high)
            .rev()
            .skip(page as usize * 250)
            .take(250)
            .collect();
        Reply::json(large_history(&ids))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let stop = SyncControl::new();
    store
        .sync_member_engine(&member(), &stop, &|event| {
            if event.outcome.recent_ready {
                stop.cancel();
            }
        })
        .await
        .unwrap();
    assert!(!premature.load(Ordering::SeqCst), "the recent window must include the first frontier from every stream before report selection");
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT activity_id FROM activity ORDER BY activity_id",
    )
    .fetch_all(&mut store.db)
    .await
    .unwrap();
    assert_eq!(ids, (1451..=1500).collect::<Vec<_>>());
    store.db.close().await.unwrap();
}

#[sqlx::test]
#[ignore = "observational SQLx admission benchmark; no timing threshold"]
async fn history_admission_fixture_benchmark() {
    for repetition in 0..3 {
        for batched in [false, true] {
            let directory = TestDirectory::new();
            let mut store = directory.open().await;
            seed_queue(&mut store, &[]).await;
            for replay in [false, true] {
                let mut writes = Duration::ZERO;
                let mut counts = Duration::ZERO;
                let mut inserted = 0;
                for page in 0..168i64 {
                    let ids: Vec<i64> =
                        ((page * 250 + 1)..=(page * 250 + 250)).collect();
                    let start = Instant::now();
                    store.begin_transaction().await.unwrap();
                    if batched {
                        let mut queue: QueryBuilder<Sqlite> = QueryBuilder::new("INSERT OR IGNORE INTO activity_queue(activity_id,character) ");
                        queue.push_values(&ids, |mut row, id| {
                            row.push_bind(id).push_bind(100i64);
                        });
                        queue.build().execute(&mut store.db).await.unwrap();
                        let mut history: QueryBuilder<Sqlite> = QueryBuilder::new("INSERT OR IGNORE INTO sync_history(member,activity_id,character,period) ");
                        history.push_values(&ids, |mut row, id| {
                            row.push_bind(10i64)
                                .push_bind(id)
                                .push_bind(100i64)
                                .push_bind("2026-01-01T00:00:00Z");
                        });
                        inserted += history
                            .build()
                            .execute(&mut store.db)
                            .await
                            .unwrap()
                            .rows_affected();
                    } else {
                        for id in ids {
                            sqlx::query("INSERT OR IGNORE INTO activity_queue(activity_id,character) VALUES (?,?)").bind(id).bind(100i64).execute(&mut store.db).await.unwrap();
                            inserted += sqlx::query("INSERT OR IGNORE INTO sync_history(member,activity_id,character,period) VALUES (?,?,?,?)").bind(10i64).bind(id).bind(100i64).bind("2026-01-01T00:00:00Z").execute(&mut store.db).await.unwrap().rows_affected();
                        }
                    }
                    store.commit_transaction().await.unwrap();
                    writes += start.elapsed();
                    let start = Instant::now();
                    let count: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM sync_history WHERE member=?",
                    )
                    .bind(10i64)
                    .fetch_one(&mut store.db)
                    .await
                    .unwrap();
                    let coverage: (i64,i64) = sqlx::query_as("SELECT COUNT(DISTINCT CASE WHEN u.activity_id IS NULL THEN q.activity_id END), COUNT(DISTINCT u.activity_id) FROM activity_queue q JOIN character c ON c.character_id=q.character LEFT JOIN sync_unavailable u ON u.activity_id=q.activity_id WHERE c.member=? AND q.synced=0").bind(10i64).fetch_one(&mut store.db).await.unwrap();
                    counts += start.elapsed();
                    assert_eq!(coverage, (count, 0));
                }
                assert_eq!(inserted, if replay { 0 } else { 42_000 });
                println!(
                    "ADMISSION_BENCH {}",
                    json!({"repetition":repetition,"batched":batched,"replay":replay,"write_seconds":writes.as_secs_f64(),"count_seconds":counts.as_secs_f64()})
                );
            }
            store.db.close().await.unwrap();
        }
    }
}

#[sqlx::test]
async fn history_reopen_ignores_corrupt_checkpoint_metadata() {
    reopen_history_with_shift(
        0,
        0..=0,
        false,
        false,
        Some("UPDATE sync_history_resume SET tail_raw='invalid'"),
    )
    .await;
}

#[sqlx::test]
async fn history_reopen_migrates_existing_store_without_checkpoints() {
    reopen_history_with_shift(
        0,
        0..=0,
        false,
        false,
        Some("DROP TABLE sync_history_resume"),
    )
    .await;
}

#[sqlx::test]
async fn history_failed_page_admission_reopens_without_skipping_rows() {
    failed_history_page_admission(false).await;
}

#[sqlx::test]
async fn history_failed_checkpoint_write_rolls_back_page_rows() {
    failed_history_page_admission(true).await;
}
