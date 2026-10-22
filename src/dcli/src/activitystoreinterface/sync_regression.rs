use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Reply {
    status: u16,
    body: Vec<u8>,
    encoding: Option<&'static str>,
    retry_after: Option<&'static str>,
}
impl Reply {
    fn json(value: serde_json::Value) -> Self {
        Self {
            status: 200,
            body: serde_json::to_vec(&value).unwrap(),
            encoding: None,
            retry_after: None,
        }
    }
}
struct Server {
    endpoint: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new(handler: impl Fn(String) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let handler = Arc::new(handler);
        let thread = std::thread::spawn(move || {
            let mut connections = Vec::new();
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        let handler = handler.clone();
                        connections.push(std::thread::spawn(move || {
                            socket.set_nonblocking(false).unwrap();
                            socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                            let mut request = Vec::new();
                            let mut buf = [0; 1024];
                            while !request.windows(4).any(|v| v == b"\r\n\r\n") {
                                let n = socket.read(&mut buf).unwrap_or(0);
                                if n == 0 { return; }
                                request.extend_from_slice(&buf[..n]);
                            }
                            let request = String::from_utf8(request).unwrap();
                            let path = request.split_whitespace().nth(1).unwrap().to_owned();
                            let reply = handler(path);
                            if reply.status == 0 { return; }
                            let encoding = reply.encoding.map(|v| format!("Content-Encoding: {}\r\n", v)).unwrap_or_default();
                            let retry_after = reply.retry_after.map(|v| format!("Retry-After: {}\r\n", v)).unwrap_or_default();
                            let header = format!("HTTP/1.1 {} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}{}Connection: close\r\n\r\n", reply.status, reply.body.len(), encoding, retry_after);
                            let _ = socket.write_all(header.as_bytes());
                            let _ = socket.write_all(&reply.body);
                        }));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("fixture listener: {}", e),
                }
            }
            for connection in connections {
                connection.join().unwrap();
            }
        });
        Self {
            endpoint,
            stop,
            thread: Some(thread),
        }
    }
    fn attach(&self, store: &mut ActivityStoreInterface) {
        store.api_interface.client.test_endpoint = Some(self.endpoint.clone());
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn envelope(response: serde_json::Value) -> serde_json::Value {
    json!({"Response": response, "ErrorCode": 1, "ThrottleSeconds": 0, "ErrorStatus": "Success", "Message": ""})
}
fn profile() -> serde_json::Value {
    let user =
        report_json(1, 1)["entries"][0]["player"]["destinyUserInfo"].clone();
    envelope(json!({"characters":{"data":{"100":{
        "characterId":"100", "dateLastPlayed":"2026-01-01T12:00:00Z", "minutesPlayedTotal":"1",
        "raceType":0,"classType":0,"genderType":0,"emblemHash":1,"baseCharacterLevel":50,
        "stats":{"1935470627":2000,"2996146975":1,"392767087":1,"1943323491":1,"1735777505":1,"144602215":1,"4244567218":1}
    }}}, "profile":{"data":{"userInfo":user}}}))
}
fn history(ids: &[i64]) -> serde_json::Value {
    let activities: Vec<_> = ids.iter().map(|id| {
        let r = report_json(*id, *id as u32);
        json!({"period": format!("2026-01-{:02}T12:00:00Z", (*id / 100).max(1)), "activityDetails":r["activityDetails"],"values":r["entries"][0]["values"]})
    }).collect();
    envelope(json!({"activities":activities}))
}
fn pgcr_id(path: &str) -> Option<i64> {
    path.split("PostGameCarnageReport/")
        .nth(1)?
        .trim_end_matches('/')
        .parse()
        .ok()
}
fn query(path: &str, name: &str) -> Option<i32> {
    path.split('?').nth(1)?.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        if key == name {
            value.parse().ok()
        } else {
            None
        }
    })
}
fn member() -> Member {
    serde_json::from_value::<crate::response::pgcr::UserInfoCard>(
        report_json(1, 1)["entries"][0]["player"]["destinyUserInfo"].clone(),
    )
    .unwrap()
    .to_member()
}

#[sqlx::test]
async fn interrupted_discovery_preserves_recent_commit_and_resumes_shifted_short_pages(
) {
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    let downloaded = Arc::new(AtomicBool::new(false));
    let seen = downloaded.clone();
    let first = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            seen.store(true, Ordering::SeqCst);
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(history(&[]));
        }
        if query(&path, "page") == Some(0) {
            return Reply::json(history(&[300]));
        }
        let until = Instant::now() + Duration::from_secs(2);
        while !seen.load(Ordering::SeqCst) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(100));
        Reply::json(
            json!({"ErrorCode":5,"ThrottleSeconds":0,"ErrorStatus":"SystemDisabled","Message":"fixture interrupted"}),
        )
    });
    first.attach(&mut store);
    let _ = store.sync_member(&member()).await;
    assert!(
        store.has_activity(&300).await,
        "recent report must be durable before a later history-page failure"
    );
    store.db.close().await.unwrap();
    drop(first);
    let second = Server::new(|path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(history(&[]));
        }
        Reply::json(history(match query(&path, "page") {
            Some(0) => &[400, 300],
            Some(1) => &[200],
            Some(2) => &[100],
            _ => &[],
        }))
    });
    let mut store = directory.open().await;
    second.attach(&mut store);
    store.sync_member(&member()).await.unwrap();
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT activity_id FROM activity ORDER BY activity_id",
    )
    .fetch_all(&mut store.db)
    .await
    .unwrap();
    assert_eq!(
        ids,
        vec![100, 200, 300, 400],
        "an early commit must not become the boundary of an interrupted scan"
    );
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM activity_queue WHERE synced=0",
    )
    .fetch_one(&mut store.db)
    .await
    .unwrap();
    assert_eq!(pending, 0);
}

#[sqlx::test]
async fn rolling_downloads_refill_while_an_earlier_report_stalls() {
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    seed_queue(&mut store, &(1..=26).collect::<Vec<_>>()).await;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let refilled = Arc::new(AtomicBool::new(false));
    let observed = refilled.clone();
    let server = Server::new(move |path| {
        let id = pgcr_id(&path).expect("only report requests expected");
        recorded.lock().unwrap().push(id);
        if id == 26 {
            let until = Instant::now() + Duration::from_secs(3);
            while Instant::now() < until {
                if recorded.lock().unwrap().contains(&1) {
                    observed.store(true, Ordering::SeqCst);
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        Reply::json(envelope(report_json(id, id as u32)))
    });
    server.attach(&mut store);
    assert_eq!(store.sync_activities(&100).await.unwrap().total_synced, 26);
    assert!(
        refilled.load(Ordering::SeqCst),
        "fast slots must refill before the slowest report finishes"
    );
    let mut fetched = requests.lock().unwrap().clone();
    fetched.sort();
    assert_eq!(
        fetched,
        (1..=26).collect::<Vec<_>>(),
        "one request per unique report"
    );
}

fn gzip_stored(bytes: &[u8]) -> Vec<u8> {
    assert!(bytes.len() < 65536);
    let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255, 1];
    let len = bytes.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(bytes);
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(crc & 1));
        }
    }
    out.extend_from_slice(&(!crc).to_le_bytes());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out
}

#[sqlx::test]
async fn gzip_and_identity_reports_preserve_identical_canonical_rows() {
    let plain = Server::new(|_| Reply::json(envelope(report_json(1, 1))));
    let zipped = Server::new(|_| Reply {
        status: 200,
        body: gzip_stored(
            &serde_json::to_vec(&envelope(report_json(1, 1))).unwrap(),
        ),
        encoding: Some("gzip"),
        retry_after: None,
    });
    let a = TestDirectory::new();
    let b = TestDirectory::new();
    let mut a = a.open().await;
    let mut b = b.open().await;
    seed_queue(&mut a, &[1]).await;
    seed_queue(&mut b, &[1]).await;
    plain.attach(&mut a);
    zipped.attach(&mut b);
    assert_eq!(a.sync_activities(&100).await.unwrap().total_synced, 1);
    assert_eq!(
        b.sync_activities(&100).await.unwrap().total_synced,
        1,
        "gzip response must be decoded before JSON parsing"
    );
    assert_eq!(snapshot(&mut a.db).await, snapshot(&mut b.db).await);
}

#[sqlx::test]
async fn privacy_denial_is_not_retried_as_a_transient_failure() {
    let calls = Arc::new(AtomicU64::new(0));
    let observed = calls.clone();
    let server = Server::new(move |_| {
        observed.fetch_add(1, Ordering::SeqCst);
        Reply::json(
            json!({"ErrorCode":1665,"ThrottleSeconds":0,"ErrorStatus":"DestinyPrivacyRestriction","Message":"private"}),
        )
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    assert!(store
        .api_interface
        .retrieve_post_game_carnage_report(1)
        .await
        .is_err());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "permission denial must not consume the transient retry budget"
    );
}

#[sqlx::test]
async fn advertised_throttle_delays_the_next_attempt() {
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let observed = attempts.clone();
    let server = Server::new(move |_| {
        let mut attempts = observed.lock().unwrap();
        attempts.push(Instant::now());
        if attempts.len() == 1 {
            Reply::json(
                json!({"ErrorCode":5,"ThrottleSeconds":1,"ErrorStatus":"SystemDisabled","Message":"retry later"}),
            )
        } else {
            Reply::json(envelope(report_json(1, 1)))
        }
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    assert!(store
        .api_interface
        .retrieve_post_game_carnage_report(1)
        .await
        .unwrap()
        .is_some());
    let attempts = attempts.lock().unwrap();
    assert_eq!(attempts.len(), 2);
    assert!(
        attempts[1].duration_since(attempts[0]) >= Duration::from_millis(950),
        "server throttle must be respected before retrying"
    );
}

#[sqlx::test]
async fn oversized_api_body_is_rejected_before_deserializing_report() {
    let server = Server::new(|_| {
        let mut value = envelope(report_json(1, 1));
        value["Message"] = json!("x".repeat(9 * 1024 * 1024));
        Reply::json(value)
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let result = store.api_interface.client.call_and_parse::<crate::response::pgcr::PGCRResponse>("https://stats.bungie.net/Platform/Destiny2/Stats/PostGameCarnageReport/1/").await;
    assert!(result.is_err(),"an API response above the 8 MiB decoded limit must not be admitted into the report queue");
}

#[sqlx::test]
async fn recent_window_is_ready_before_older_reports_are_requested() {
    let older_requested = Arc::new(AtomicBool::new(false));
    let observed = older_requested.clone();
    let server = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            if id == 1 {
                observed.store(true, Ordering::SeqCst);
            }
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(history(&[]));
        }
        let ids: Vec<i64> = match query(&path, "page") {
            Some(0) => (200..250).rev().collect(),
            Some(1) => vec![1],
            _ => vec![],
        };
        Reply::json(history(&ids))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let ready = std::cell::Cell::new(false);
    let outcome = store
        .sync_member_engine(&member(), &SyncControl::default(), &|event| {
            if event.phase == SyncPhase::RecentReady {
                assert!(
                    !older_requested.load(Ordering::SeqCst),
                    "recent readiness was delayed until older history"
                );
                assert!(event.outcome.recent_ready);
                assert!(!event.outcome.complete);
                assert_eq!(event.outcome.committed, 50);
                ready.set(true);
            }
        })
        .await
        .unwrap();
    assert!(ready.get());
    assert!(outcome.complete);
    assert_eq!(outcome.committed, 51);
}

#[sqlx::test]
async fn unresolved_report_is_attempted_once_per_pass_and_never_full_coverage()
{
    let requests = Arc::new(AtomicU64::new(0));
    let observed = requests.clone();
    let server = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            if id == 300 {
                observed.fetch_add(1, Ordering::SeqCst);
                return Reply::json(envelope(serde_json::Value::Null));
            }
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(history(&[]));
        }
        Reply::json(history(match query(&path, "page") {
            Some(0) => &[300],
            Some(1) => &[200],
            Some(2) => &[100],
            _ => &[],
        }))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let outcome = store
        .sync_member_engine(&member(), &SyncControl::default(), &|event| {
            assert_ne!(
                event.phase,
                SyncPhase::Complete,
                "unresolved details were presented as a complete archive"
            );
        })
        .await
        .unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    assert!(outcome.discovery_complete);
    assert!(!outcome.complete);
    assert!(!outcome.recent_ready);
    assert_eq!(outcome.pending, 1);
    assert_eq!(outcome.committed, 2);
}

#[sqlx::test]
async fn cancellation_after_commit_drains_and_resumes_without_losing_discovery()
{
    let requests = Arc::new(AtomicU64::new(0));
    let observed = requests.clone();
    let server = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            observed.fetch_add(1, Ordering::SeqCst);
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(history(&[]));
        }
        Reply::json(history(match query(&path, "page") {
            Some(0) => &[300, 200],
            Some(1) => &[100],
            _ => &[],
        }))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let control = SyncControl::default();
    let outcome = store
        .sync_member_engine(&member(), &control, &|event| {
            if event.phase == SyncPhase::Committed {
                control.cancel();
            }
        })
        .await
        .unwrap();
    assert!(outcome.cancelled);
    assert!(!outcome.complete);
    assert!(outcome.committed > 0);
    let calls_at_return = requests.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        requests.load(Ordering::SeqCst),
        calls_at_return,
        "network work survived cancellation return"
    );
    store.db.close().await.unwrap();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let resumed = store
        .sync_member_engine(&member(), &SyncControl::default(), &|_| {})
        .await
        .unwrap();
    assert!(resumed.complete);
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT activity_id FROM activity ORDER BY activity_id",
    )
    .fetch_all(&mut store.db)
    .await
    .unwrap();
    assert_eq!(ids, vec![100, 200, 300]);
}

#[sqlx::test]
async fn pre_cancelled_sync_makes_no_api_requests() {
    let calls = Arc::new(AtomicU64::new(0));
    let observed = calls.clone();
    let server = Server::new(move |path| {
        observed.fetch_add(1, Ordering::SeqCst);
        Reply::json(if path.contains("components=") {
            profile()
        } else {
            history(&[])
        })
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let control = SyncControl::default();
    control.cancel();
    let outcome = store
        .sync_member_engine(&member(), &control, &|_| {})
        .await
        .unwrap();
    assert!(outcome.cancelled);
    assert!(!outcome.complete);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "already cancelled work started a network request"
    );
}

#[sqlx::test]
async fn cancellation_interrupts_a_stalled_history_request() {
    let control = SyncControl::default();
    let cancel = control.clone();
    let server = Server::new(move |path| {
        if path.contains("components=") {
            return Reply::json(profile());
        }
        cancel.cancel();
        std::thread::sleep(Duration::from_secs(2));
        Reply::json(history(&[]))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let start = Instant::now();
    let outcome = store
        .sync_member_engine(&member(), &control, &|_| {})
        .await
        .unwrap();
    assert!(outcome.cancelled);
    assert!(!outcome.complete);
    assert!(
        start.elapsed() < Duration::from_millis(750),
        "cancellation waited for the blocked history response"
    );
}

#[sqlx::test]
async fn throttle_limit_envelope_is_retried_after_shared_cooldown() {
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let observed = attempts.clone();
    let server = Server::new(move |_| {
        let mut attempts = observed.lock().unwrap();
        attempts.push(Instant::now());
        if attempts.len() == 1 {
            Reply::json(
                json!({"ErrorCode":36,"ThrottleSeconds":1,"ErrorStatus":"ThrottleLimitExceededMomentarily","Message":"retry later"}),
            )
        } else {
            Reply::json(envelope(report_json(1, 1)))
        }
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    assert!(store
        .api_interface
        .retrieve_post_game_carnage_report(1)
        .await
        .expect("throttle is retryable")
        .is_some());
    let attempts = attempts.lock().unwrap();
    assert_eq!(attempts.len(), 2);
    assert!(
        attempts[1].duration_since(attempts[0]) >= Duration::from_millis(950)
    );
}

#[sqlx::test]
async fn http_retry_after_is_respected() {
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let observed = attempts.clone();
    let server = Server::new(move |_| {
        let mut attempts = observed.lock().unwrap();
        attempts.push(Instant::now());
        if attempts.len() == 1 {
            let mut reply = Reply::json(json!({}));
            reply.status = 429;
            reply.retry_after = Some("1");
            reply
        } else {
            Reply::json(envelope(report_json(1, 1)))
        }
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    assert!(store
        .api_interface
        .retrieve_post_game_carnage_report(1)
        .await
        .unwrap()
        .is_some());
    let attempts = attempts.lock().unwrap();
    assert_eq!(attempts.len(), 2);
    assert!(
        attempts[1].duration_since(attempts[0]) >= Duration::from_millis(950)
    );
}

#[sqlx::test]
async fn transient_disconnect_retries_and_commits_in_the_same_pass() {
    let calls = Arc::new(AtomicU64::new(0));
    let observed = calls.clone();
    let server = Server::new(move |_| {
        if observed.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply {
                status: 0,
                body: Vec::new(),
                encoding: None,
                retry_after: None,
            }
        } else {
            Reply::json(envelope(report_json(1, 1)))
        }
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    seed_queue(&mut store, &[1]).await;
    server.attach(&mut store);
    let result = store.sync_activities(&100).await.unwrap();
    assert_eq!(
        result.total_synced, 1,
        "a dropped connection must recover within the current sync pass"
    );
    assert!(store.has_activity(&1).await);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[sqlx::test]
async fn repeated_disconnects_exhaust_a_bounded_retry_budget_and_remain_pending(
) {
    let calls = Arc::new(AtomicU64::new(0));
    let observed = calls.clone();
    let server = Server::new(move |_| {
        observed.fetch_add(1, Ordering::SeqCst);
        Reply {
            status: 0,
            body: Vec::new(),
            encoding: None,
            retry_after: None,
        }
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    seed_queue(&mut store, &[1]).await;
    server.attach(&mut store);
    let result = store.sync_activities(&100).await.unwrap();
    assert_eq!(result.total_synced, 0);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "transport retries must stop after three attempts"
    );
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM activity_queue WHERE synced=0",
    )
    .fetch_one(&mut store.db)
    .await
    .unwrap();
    assert_eq!(pending, 1);
}

fn large_history(ids: &[i64]) -> serde_json::Value {
    let activities: Vec<_> = ids.iter().map(|id| {
        let report = report_json(*id, *id as u32);
        let period = DateTime::<Utc>::from_timestamp(1_767_225_600 + id, 0).unwrap();
        json!({"period": period.format("%Y-%m-%dT%H:%M:%SZ").to_string(), "activityDetails": report["activityDetails"],
            "values": report["entries"][0]["values"]})
    }).collect();
    envelope(json!({"activities": activities}))
}

#[sqlx::test]
async fn large_history_discovery_does_not_wait_for_old_reports() {
    const COUNT: i64 = 10_250;
    let discovered = Arc::new(AtomicBool::new(false));
    let history_finished = discovered.clone();
    let older_requested = Arc::new(AtomicBool::new(false));
    let observed_older_request = older_requested.clone();
    let control = SyncControl::default();
    let server_control = control.clone();
    let server = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            if id <= COUNT - 50 && !history_finished.load(Ordering::SeqCst) {
                observed_older_request.store(true, Ordering::SeqCst);
                server_control.cancel();
            }
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(large_history(&[]));
        }
        let page = query(&path, "page").unwrap() as usize;
        let ids: Vec<_> =
            (1..=COUNT).rev().skip(page * 250).take(250).collect();
        Reply::json(large_history(&ids))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let timed_out = Arc::new(AtomicBool::new(false));
    let observed_timeout = timed_out.clone();
    let timeout_control = control.clone();
    let timeout = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        observed_timeout.store(true, Ordering::SeqCst);
        timeout_control.cancel();
    });
    let result = store
        .sync_member_engine(&member(), &control, &|event| {
            if event.outcome.discovery_complete {
                discovered.store(true, Ordering::SeqCst);
                control.cancel();
            }
        })
        .await;
    timeout.abort();
    let outcome = result.unwrap();
    assert!(
        !timed_out.load(Ordering::SeqCst),
        "large-history fixture exceeded its safety timeout"
    );
    assert!(
        !older_requested.load(Ordering::SeqCst),
        "older report requests must not begin before all history is discovered"
    );
    assert!(
        discovered.load(Ordering::SeqCst),
        "all 10,250 IDs must be discovered without waiting for older report bodies"
    );
    assert!(
        outcome.recent_ready,
        "latest 50 usable reports must remain available"
    );
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT activity_id FROM sync_history ORDER BY activity_id",
    )
    .fetch_all(&mut store.db)
    .await
    .unwrap();
    assert_eq!(ids, (1..=COUNT).collect::<Vec<_>>());
    let saved: Vec<i64> = sqlx::query_scalar(
        "SELECT activity_id FROM activity ORDER BY activity_id",
    )
    .fetch_all(&mut store.db)
    .await
    .unwrap();
    assert_eq!(saved, ((COUNT - 49)..=COUNT).collect::<Vec<_>>());
    assert!(!outcome.complete);
}

#[sqlx::test]
async fn short_refresh_opportunity_saves_pending_reports_before_slow_discovery()
{
    let server = Server::new(|path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        std::thread::sleep(Duration::from_secs(1));
        Reply::json(history(&[]))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    seed_queue(&mut store, &(1..=32).collect::<Vec<_>>()).await;
    sqlx::query("DELETE FROM sync")
        .execute(&mut store.db)
        .await
        .unwrap();
    server.attach(&mut store);
    let control = SyncControl::default();
    let result = store
        .sync_member_slice_with_control(
            member().id,
            member().platform,
            Duration::from_millis(500),
            &control,
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(result.disposition, SyncSliceDisposition::Checkpointed);
    let outcome = result.outcome;
    assert!(
        !outcome.cancelled,
        "a planned checkpoint is distinct from OS cancellation"
    );
    assert!(outcome.committed > 0, "a short refresh must advance durable pending work before slow enumeration");
    let saved: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity")
        .fetch_one(&mut store.db)
        .await
        .unwrap();
    assert_eq!(saved, outcome.committed as i64);
    let completed_syncs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync")
        .fetch_one(&mut store.db)
        .await
        .unwrap();
    assert_eq!(
        completed_syncs, 0,
        "a partial refresh must not set the full-sync timestamp"
    );
    assert!(!outcome.complete);
}

#[sqlx::test]
async fn equal_timestamp_frontier_does_not_publish_premature_recent_readiness()
{
    let second_page = Arc::new(AtomicBool::new(false));
    let observed = second_page.clone();
    let server = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(history(&[]));
        }
        let ids: Vec<i64> = match query(&path, "page") {
            Some(0) => (1..=50).rev().collect(),
            Some(1) => {
                observed.store(true, Ordering::SeqCst);
                (51..=100).rev().collect()
            }
            _ => vec![],
        };
        let mut value = history(&ids);
        for item in value["Response"]["activities"].as_array_mut().unwrap() {
            item["period"] = json!("2026-01-01T12:00:00Z");
        }
        Reply::json(value)
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    server.attach(&mut store);
    let premature = std::cell::Cell::new(false);
    let outcome = store
        .sync_member_engine(&member(), &SyncControl::default(), &|event| {
            if event.phase == SyncPhase::RecentReady
                && !second_page.load(Ordering::SeqCst)
            {
                premature.set(true);
            }
        })
        .await
        .unwrap();
    assert!(!premature.get(), "equal timestamps do not establish that every newest eligible ID is known");
    assert!(outcome.recent_ready && outcome.complete);
}

#[sqlx::test]
async fn zero_budget_and_precancelled_slices_admit_no_requests() {
    let requests = Arc::new(AtomicU64::new(0));
    let seen = requests.clone();
    let server = Server::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        Reply::json(profile())
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    seed_queue(&mut store, &[1]).await;
    server.attach(&mut store);
    let control = SyncControl::default();
    let zero = store
        .sync_member_slice_with_control(
            10,
            member().platform,
            Duration::ZERO,
            &control,
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(zero.disposition, SyncSliceDisposition::Deferred);
    control.cancel();
    let cancelled = store
        .sync_member_slice_with_control(
            10,
            member().platform,
            Duration::from_secs(2),
            &control,
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(cancelled.disposition, SyncSliceDisposition::Cancelled);
    assert!(cancelled.outcome.cancelled);
    assert_eq!(requests.load(Ordering::SeqCst), 0);
}

#[sqlx::test]
async fn slice_new_discovery_is_useful_but_replayed_ids_are_not() {
    for replay in [false, true] {
        let server = Server::new(|path| {
            if let Some(id) = pgcr_id(&path) {
                std::thread::sleep(Duration::from_millis(700));
                return Reply::json(envelope(report_json(id, id as u32)));
            }
            if path.contains("components=") {
                return Reply::json(profile());
            }
            if query(&path, "mode") == Some(32) {
                return Reply::json(history(&[]));
            }
            Reply::json(history(if query(&path, "page") == Some(0) {
                &[300, 200, 100]
            } else {
                &[]
            }))
        });
        let directory = TestDirectory::new();
        let mut store = directory.open().await;
        seed_queue(&mut store, &[]).await;
        sqlx::query("DELETE FROM sync")
            .execute(&mut store.db)
            .await
            .unwrap();
        if replay {
            for id in [300_i64, 200, 100] {
                sqlx::query("INSERT INTO sync_history(member,activity_id,character,period) VALUES (10,?,100,'2026-01-01T12:00:00Z')")
                    .bind(id).execute(&mut store.db).await.unwrap();
            }
        }
        server.attach(&mut store);
        let result = store
            .sync_member_slice_with_control(
                10,
                member().platform,
                Duration::from_millis(250),
                &SyncControl::default(),
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(result.outcome.committed, 0);
        assert_eq!(result.outcome.newly_discovered, if replay { 0 } else { 3 });
        assert_eq!(
            result.disposition,
            if replay {
                SyncSliceDisposition::Deferred
            } else {
                SyncSliceDisposition::Checkpointed
            }
        );
        assert!(!result.outcome.complete && !result.outcome.cancelled);
    }
}

async fn canonical_report_rows(
    store: &mut ActivityStoreInterface,
) -> Vec<(String, Vec<Vec<String>>)> {
    let tables = [
        "activity",
        "character",
        "member",
        "modes",
        "team_result",
        "weapon_result",
        "medal_result",
        "character_activity_stats",
        "scoreboard_result",
    ];
    let generated_id_tables = [
        "modes",
        "team_result",
        "weapon_result",
        "medal_result",
        "character_activity_stats",
        "scoreboard_result",
    ];
    let child_tables = ["weapon_result", "medal_result", "scoreboard_result"];
    let mut canonical = Vec::with_capacity(tables.len());

    for table in tables {
        let columns = sqlx::query(&format!("PRAGMA table_info(\"{}\")", table))
            .fetch_all(&mut store.db)
            .await
            .unwrap();
        let mut expressions = Vec::new();
        for column in columns {
            let name: String = column.get("name");
            if name == "id" && generated_id_tables.contains(&table) {
                continue;
            }
            if child_tables.contains(&table)
                && name == "character_activity_stats"
            {
                expressions.push("quote(parent.\"activity\")".to_owned());
                expressions.push("quote(parent.\"character\")".to_owned());
            } else {
                let quoted_name = name.replace('"', "\"\"");
                expressions.push(format!("quote(child.\"{}\")", quoted_name));
            }
        }

        let from = if child_tables.contains(&table) {
            let orphans: i64 = sqlx::query_scalar(&format!(
                "SELECT count(*) FROM \"{}\" AS child \
                 LEFT JOIN character_activity_stats AS parent \
                 ON child.character_activity_stats = parent.id \
                 WHERE parent.id IS NULL",
                table
            ))
            .fetch_one(&mut store.db)
            .await
            .unwrap();
            assert_eq!(orphans, 0, "{} contains orphan stats references", table);
            format!(
                "\"{}\" AS child LEFT JOIN character_activity_stats AS parent \
                 ON child.character_activity_stats = parent.id",
                table
            )
        } else {
            format!("\"{}\" AS child", table)
        };
        let query = format!("SELECT {} FROM {}", expressions.join(", "), from);
        let rows = sqlx::query(&query)
            .fetch_all(&mut store.db)
            .await
            .unwrap();
        let mut values: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                (0..expressions.len())
                    .map(|index| row.get::<String, _>(index))
                    .collect()
            })
            .collect();
        values.sort();
        canonical.push((table.to_owned(), values));
    }

    canonical
}

#[sqlx::test]
async fn canonical_reports_compare_values_and_relationships_not_insertion_order() {
    let left_directory = TestDirectory::new();
    let right_directory = TestDirectory::new();
    let mut left = left_directory.open().await;
    let mut right = right_directory.open().await;
    seed_queue(&mut left, &[1, 2]).await;
    seed_queue(&mut right, &[1, 2]).await;
    left.insert_activities_batch(&mut [report(1, 1001), report(2, 1002)], &100)
        .await.unwrap();
    right.insert_activities_batch(&mut [report(2, 1002), report(1, 1001)], &100)
        .await.unwrap();
    let expected = canonical_report_rows(&mut left).await;
    assert_eq!(expected, canonical_report_rows(&mut right).await);
    sqlx::query("UPDATE weapon_result SET kills=99 WHERE reference_id=1001")
        .execute(&mut right.db).await.unwrap();
    assert_ne!(expected, canonical_report_rows(&mut right).await);
    sqlx::query("UPDATE weapon_result SET kills=3, character_activity_stats=(SELECT id FROM character_activity_stats WHERE activity=2) WHERE reference_id=1001")
        .execute(&mut right.db).await.unwrap();
    assert_ne!(expected, canonical_report_rows(&mut right).await);
}

#[sqlx::test]
async fn repeated_refresh_reopens_advance_queue_and_match_uninterrupted_rows() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let slow_listing = Arc::new(AtomicBool::new(true));
    let slow = slow_listing.clone();
    let server = Server::new(move |path| {
        if let Some(id) = pgcr_id(&path) {
            recorded.lock().unwrap().push(id);
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if slow.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(500));
        }
        let ids: Vec<i64> = if query(&path, "mode") != Some(32)
            && query(&path, "page") == Some(0)
        {
            (1..=80).rev().collect()
        } else {
            vec![]
        };
        Reply::json(history(&ids))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    seed_queue(&mut store, &(1..=80).collect::<Vec<_>>()).await;
    sqlx::query("DELETE FROM sync")
        .execute(&mut store.db)
        .await
        .unwrap();
    store.db.close().await.unwrap();
    let mut saved = Vec::new();
    for _ in 0..20 {
        let mut store = directory.open().await;
        server.attach(&mut store);
        requests.lock().unwrap().clear();
        let result = store
            .sync_member_slice_with_control(
                10,
                member().platform,
                Duration::from_millis(150),
                &SyncControl::default(),
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(result.disposition, SyncSliceDisposition::Checkpointed);
        assert!(!result.outcome.cancelled && !result.outcome.complete);
        let fetched = requests.lock().unwrap().clone();
        assert!(
            !fetched.iter().any(|id| saved.contains(id)),
            "a reopened slice re-fetched an already committed report"
        );
        let next: Vec<i64> = sqlx::query_scalar(
            "SELECT activity_id FROM activity ORDER BY activity_id",
        )
        .fetch_all(&mut store.db)
        .await
        .unwrap();
        assert!(
            next.len() > saved.len(),
            "each bounded opportunity must advance this pending queue"
        );
        saved = next;
        store.db.close().await.unwrap();
        if saved.len() == 80 {
            break;
        }
    }
    assert_eq!(saved, (1..=80).collect::<Vec<_>>());
    slow_listing.store(false, Ordering::SeqCst);
    let mut store = directory.open().await;
    server.attach(&mut store);
    let final_slice = store
        .sync_member_slice_with_control(
            10,
            member().platform,
            Duration::from_secs(3),
            &SyncControl::default(),
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(final_slice.disposition, SyncSliceDisposition::FullComplete);
    assert!(
        final_slice.outcome.discovery_complete && final_slice.outcome.complete
    );
    assert_eq!(final_slice.outcome.pending, 0);
    let oracle_directory = TestDirectory::new();
    let mut oracle = oracle_directory.open().await;
    server.attach(&mut oracle);
    let full = oracle
        .sync_member_engine(&member(), &SyncControl::default(), &|_| {})
        .await
        .unwrap();
    assert!(full.complete);
    assert_eq!(
        canonical_report_rows(&mut store).await,
        canonical_report_rows(&mut oracle).await
    );
    let unchanged = store
        .sync_member_slice_with_control(
            10,
            member().platform,
            Duration::from_secs(3),
            &SyncControl::default(),
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(unchanged.disposition, SyncSliceDisposition::FullComplete);
    assert_eq!(unchanged.outcome.committed, 0);
    assert_eq!(unchanged.outcome.newly_discovered, 0);
}

#[sqlx::test]
async fn draining_legacy_queue_does_not_hide_undiscovered_older_history() {
    let server = Server::new(|path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        if query(&path, "mode") == Some(32) {
            return Reply::json(history(&[]));
        }
        Reply::json(history(match query(&path, "page") {
            Some(0) => &[300],
            Some(1) => &[200],
            Some(2) => &[100],
            _ => &[],
        }))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    seed_queue(&mut store, &[300]).await;
    sqlx::query("DELETE FROM sync")
        .execute(&mut store.db)
        .await
        .unwrap();
    server.attach(&mut store);
    let result = store
        .sync_member_slice_with_control(
            10,
            member().platform,
            Duration::from_secs(3),
            &SyncControl::default(),
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(result.disposition, SyncSliceDisposition::FullComplete);
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT activity_id FROM activity ORDER BY activity_id",
    )
    .fetch_all(&mut store.db)
    .await
    .unwrap();
    assert_eq!(
        ids,
        vec![100, 200, 300],
        "new queued commits must not move the frozen history boundary"
    );
}

#[sqlx::test]
async fn external_expiration_after_slice_commit_is_not_a_clean_checkpoint() {
    let server = Server::new(|path| {
        if let Some(id) = pgcr_id(&path) {
            return Reply::json(envelope(report_json(id, id as u32)));
        }
        if path.contains("components=") {
            return Reply::json(profile());
        }
        Reply::json(history(&[]))
    });
    let directory = TestDirectory::new();
    let mut store = directory.open().await;
    seed_queue(&mut store, &(1..=40).collect::<Vec<_>>()).await;
    server.attach(&mut store);
    let control = SyncControl::default();
    let result = store
        .sync_member_slice_with_control(
            10,
            member().platform,
            Duration::from_secs(3),
            &control,
            |event| {
                if event.outcome.committed > 0 {
                    control.cancel();
                }
            },
        )
        .await
        .unwrap();
    assert_eq!(result.disposition, SyncSliceDisposition::Cancelled);
    assert!(result.outcome.cancelled && !result.outcome.complete);
    assert!(result.outcome.committed > 0 && result.outcome.pending > 0);
}

#[path = "history_discovery_tests.rs"]
mod history_discovery_tests;
