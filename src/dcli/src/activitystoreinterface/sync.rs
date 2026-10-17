use super::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::task::JoinSet;

pub(super) const SYNC_SCAN_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sync_scan (
    member INTEGER NOT NULL,
    character INTEGER NOT NULL,
    mode INTEGER NOT NULL,
    boundary INTEGER NOT NULL,
    complete INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (member, character, mode)
);
CREATE TABLE IF NOT EXISTS sync_unavailable (
    activity_id INTEGER PRIMARY KEY,
    reason TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sync_history (
    member INTEGER NOT NULL,
    activity_id INTEGER NOT NULL,
    character INTEGER NOT NULL,
    period TEXT NOT NULL,
    PRIMARY KEY(member, activity_id)
);
CREATE INDEX IF NOT EXISTS sync_history_recent ON sync_history(member, period DESC);
"#;

const HISTORY_PAGE_SIZE: i32 = 250;
const NETWORK_WINDOW: usize = 25;
const WRITE_BATCH_SIZE: usize = 4;
const QUEUE_PAGE_SIZE: i64 = 256;
const RECENT_COUNT: usize = 50;

/// A cancellation request that can be sent without borrowing the store.
#[derive(Clone, Default)]
pub struct SyncControl(Arc<AtomicBool>);

impl SyncControl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub async fn cancelled(&self) {
        while !self.is_cancelled() {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }
}

pub(super) async fn await_or_cancel<F: Future>(
    control: &SyncControl,
    request: F,
) -> Option<F::Output> {
    if control.is_cancelled() {
        return None;
    }
    let cancelled = control.cancelled();
    futures::pin_mut!(cancelled);
    futures::pin_mut!(request);
    match futures::future::select(cancelled, request).await {
        futures::future::Either::Left(_) => None,
        futures::future::Either::Right((_value, _))
            if control.is_cancelled() =>
        {
            None
        }
        futures::future::Either::Right((value, _)) => Some(value),
    }
}

/// Coverage and durable-write state at one point in a sync.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SyncOutcome {
    pub discovered: u32,
    pub committed: u32,
    pub pending: u32,
    pub unavailable: u32,
    pub discovery_complete: bool,
    pub recent_ready: bool,
    pub complete: bool,
    pub cancelled: bool,
}

/// Events expose durable write progress and coverage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncPhase {
    Starting,
    Discovering,
    Committed,
    RecentReady,
    Complete,
    Cancelled,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncEvent {
    pub phase: SyncPhase,
    pub outcome: SyncOutcome,
}

struct Stream {
    character: i64,
    mode: Mode,
    boundary: i64,
    page: i32,
    complete: bool,
    frontier: Option<DateTime<Utc>>,
}

fn trim_recent(recent: &mut HashMap<i64, (DateTime<Utc>, i64)>) {
    if recent.len() <= RECENT_COUNT {
        return;
    }
    let mut ranked: Vec<_> = recent
        .iter()
        .map(|(id, (period, _))| (*id, *period))
        .collect();
    ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
    let keep: HashSet<_> = ranked
        .into_iter()
        .take(RECENT_COUNT)
        .map(|(id, _)| id)
        .collect();
    recent.retain(|id, _| keep.contains(id));
}

impl ActivityStoreInterface {
    async fn reset_attempted(&mut self) -> Result<(), Error> {
        sqlx::query("CREATE TEMP TABLE IF NOT EXISTS sync_attempted(activity_id INTEGER PRIMARY KEY)")
            .execute(&mut self.db).await?;
        sqlx::query("DELETE FROM sync_attempted")
            .execute(&mut self.db)
            .await?;
        Ok(())
    }

    async fn discovered_count(&mut self, member: i64) -> Result<u32, Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sync_history WHERE member=?",
        )
        .bind(member)
        .fetch_one(&mut self.db)
        .await?;
        Ok(count as u32)
    }

    async fn load_existing_recent(
        &mut self,
        member: i64,
        recent: &mut HashMap<i64, (DateTime<Utc>, i64)>,
    ) -> Result<(), Error> {
        let rows: Vec<(i64, String, i64)> = sqlx::query_as(
            "SELECT a.activity_id, a.period, MIN(s.character) FROM activity a JOIN character_activity_stats s ON s.activity=a.activity_id JOIN character c ON c.character_id=s.character WHERE c.member=? GROUP BY a.activity_id ORDER BY a.period DESC LIMIT 50")
            .bind(member).fetch_all(&mut self.db).await?;
        for (id, period, character) in rows {
            let parsed = DateTime::parse_from_rfc3339(&period)
                .map_err(|error| Error::ChronoParse {
                    description: error.to_string(),
                })?
                .with_timezone(&Utc);
            recent.insert(id, (parsed, character));
        }
        Ok(())
    }

    async fn maybe_recent_ready<F>(
        &mut self,
        streams: &[Stream],
        recent: &HashMap<i64, (DateTime<Utc>, i64)>,
        control: &SyncControl,
        outcome: &mut SyncOutcome,
        on_event: &F,
    ) -> Result<(), Error>
    where
        F: Fn(SyncEvent),
    {
        if outcome.recent_ready || outcome.cancelled || control.is_cancelled() {
            return Ok(());
        }
        let enough = recent.len() >= RECENT_COUNT;
        if !enough && streams.iter().any(|stream| !stream.complete) {
            return Ok(());
        }
        if enough {
            let cutoff =
                recent.values().map(|(period, _)| *period).min().unwrap();
            if streams.iter().any(|stream| {
                !stream.complete
                    && stream.frontier.map_or(true, |period| period > cutoff)
            }) {
                return Ok(());
            }
        }
        if !recent.is_empty() {
            let mut query: QueryBuilder<Sqlite> = QueryBuilder::new(
                "SELECT COUNT(*) FROM activity WHERE activity_id IN (",
            );
            let mut separated = query.separated(",");
            for id in recent.keys() {
                separated.push_bind(*id);
            }
            separated.push_unseparated(")");
            let stored: i64 =
                query.build_query_scalar().fetch_one(&mut self.db).await?;
            if stored != recent.len() as i64 {
                return Ok(());
            }
        }
        outcome.recent_ready = true;
        on_event(SyncEvent {
            phase: SyncPhase::RecentReady,
            outcome: *outcome,
        });
        Ok(())
    }

    pub(super) async fn sync_member_engine<F>(
        &mut self,
        member: &Member,
        control: &SyncControl,
        on_event: &F,
    ) -> Result<SyncOutcome, Error>
    where
        F: Fn(SyncEvent),
    {
        if control.is_cancelled() {
            let outcome = SyncOutcome {
                cancelled: true,
                ..SyncOutcome::default()
            };
            on_event(SyncEvent {
                phase: SyncPhase::Cancelled,
                outcome,
            });
            return Ok(outcome);
        }
        let profile = match await_or_cancel(
            control,
            self.api_interface
                .get_player_info(&member.id, &member.platform),
        )
        .await
        {
            Some(result) => result?,
            None => {
                let outcome = SyncOutcome {
                    cancelled: true,
                    ..SyncOutcome::default()
                };
                on_event(SyncEvent {
                    phase: SyncPhase::Cancelled,
                    outcome,
                });
                return Ok(outcome);
            }
        };
        if control.is_cancelled() {
            let outcome = SyncOutcome {
                cancelled: true,
                ..SyncOutcome::default()
            };
            on_event(SyncEvent {
                phase: SyncPhase::Cancelled,
                outcome,
            });
            return Ok(outcome);
        }
        self.insert_member(&profile.user_info.to_member()).await?;
        let mut streams = Vec::new();
        for character in &profile.characters.characters {
            self.insert_character(&character.id, &character.class_type, member)
                .await?;
            for mode in [Mode::AllPvP, Mode::PrivateMatchesAll] {
                let previous: Option<(i64, i64)> = sqlx::query_as(
                    "SELECT boundary, complete FROM sync_scan WHERE member=? AND character=? AND mode=?"
                ).bind(member.id).bind(character.id).bind(mode.as_id() as i64)
                 .fetch_optional(&mut self.db).await?;
                let boundary = match previous {
                    Some((boundary, 0)) => boundary,
                    _ => self.get_max_activity_id(&character.id, &mode).await?,
                };
                streams.push(Stream {
                    character: character.id,
                    mode,
                    boundary,
                    page: 0,
                    complete: false,
                    frontier: None,
                });
            }
        }

        self.begin_transaction().await?;
        let initialized = async {
            for stream in &streams {
                sqlx::query("INSERT INTO sync_scan (member,character,mode,boundary,complete) VALUES (?,?,?,?,0) ON CONFLICT(member,character,mode) DO UPDATE SET boundary=excluded.boundary, complete=0")
                    .bind(member.id).bind(stream.character).bind(stream.mode.as_id() as i64)
                    .bind(stream.boundary).execute(&mut self.db).await?;
            }
            self.commit_transaction().await
        }.await;
        if let Err(error) = initialized {
            self.rollback_transaction().await?;
            return Err(error);
        }

        let mut outcome = SyncOutcome::default();
        let mut recent: HashMap<i64, (DateTime<Utc>, i64)> = HashMap::new();
        self.reset_attempted().await?;
        self.load_existing_recent(member.id, &mut recent).await?;
        let mut first_pages = Vec::new();

        // Admit one page from every stream before fetching details. This makes the
        // newest period across characters available ahead of old queued work.
        for stream in &mut streams {
            if control.is_cancelled() {
                break;
            }
            first_pages.extend(
                self.scan_page(member, stream, control, &mut recent).await?,
            );
            if control.is_cancelled() {
                break;
            }
            outcome.discovered = self.discovered_count(member.id).await?;
            self.update_outcome_coverage(member.id, &mut outcome)
                .await?;
            on_event(SyncEvent {
                phase: SyncPhase::Discovering,
                outcome,
            });
        }
        outcome.discovery_complete =
            streams.iter().all(|stream| stream.complete);

        if !control.is_cancelled() {
            let mut selected: Vec<_> = recent
                .iter()
                .map(|(id, (period, character))| (*id, *period, *character))
                .collect();
            selected.sort_unstable_by(|a, b| {
                b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0))
            });
            let recent_ids: Vec<_> = selected
                .into_iter()
                .take(RECENT_COUNT)
                .map(|(id, _, character)| (id, character))
                .collect();
            self.download_ids(recent_ids, control, &mut outcome, on_event)
                .await?;
            self.maybe_recent_ready(
                &streams,
                &recent,
                control,
                &mut outcome,
                on_event,
            )
            .await?;
            for page in first_pages.chunks(QUEUE_PAGE_SIZE as usize) {
                if control.is_cancelled() {
                    break;
                }
                self.download_ids(
                    page.to_vec(),
                    control,
                    &mut outcome,
                    on_event,
                )
                .await?;
            }
            outcome.discovery_complete =
                streams.iter().all(|stream| stream.complete);
            self.maybe_recent_ready(
                &streams,
                &recent,
                control,
                &mut outcome,
                on_event,
            )
            .await?;
        }

        while !control.is_cancelled() && streams.iter().any(|s| !s.complete) {
            for stream in &mut streams {
                if control.is_cancelled() {
                    break;
                }
                if stream.complete {
                    continue;
                }
                let page_ids = self
                    .scan_page(member, stream, control, &mut recent)
                    .await?;
                if control.is_cancelled() {
                    break;
                }
                outcome.discovered = self.discovered_count(member.id).await?;
                self.update_outcome_coverage(member.id, &mut outcome)
                    .await?;
                on_event(SyncEvent {
                    phase: SyncPhase::Discovering,
                    outcome,
                });
                self.download_ids(page_ids, control, &mut outcome, on_event)
                    .await?;
            }
            outcome.discovery_complete =
                streams.iter().all(|stream| stream.complete);
            self.maybe_recent_ready(
                &streams,
                &recent,
                control,
                &mut outcome,
                on_event,
            )
            .await?;
        }

        outcome.discovery_complete = streams.iter().all(|s| s.complete);
        if !control.is_cancelled() {
            self.download_member_pending(
                member.id,
                control,
                &mut outcome,
                on_event,
            )
            .await?;
        }
        self.update_outcome_coverage(member.id, &mut outcome)
            .await?;
        outcome.cancelled = control.is_cancelled();
        self.maybe_recent_ready(
            &streams,
            &recent,
            control,
            &mut outcome,
            on_event,
        )
        .await?;
        outcome.complete = outcome.discovery_complete
            && !outcome.cancelled
            && outcome.pending == 0
            && outcome.unavailable == 0;
        if outcome.complete {
            self.update_sync_entry(&member.id).await?;
        }
        on_event(SyncEvent {
            phase: if outcome.cancelled {
                SyncPhase::Cancelled
            } else if outcome.complete {
                SyncPhase::Complete
            } else {
                SyncPhase::Incomplete
            },
            outcome,
        });
        Ok(outcome)
    }

    async fn scan_page(
        &mut self,
        member: &Member,
        stream: &mut Stream,
        control: &SyncControl,
        recent: &mut HashMap<i64, (DateTime<Utc>, i64)>,
    ) -> Result<Vec<(i64, i64)>, Error> {
        let page = match await_or_cancel(
            control,
            self.api_interface.retrieve_activities(
                &member.id,
                &stream.character,
                &member.platform,
                &stream.mode,
                HISTORY_PAGE_SIZE,
                stream.page,
            ),
        )
        .await
        {
            Some(result) => result?,
            None => return Ok(Vec::new()),
        };
        if control.is_cancelled() {
            return Ok(Vec::new());
        }
        let Some(activities) = page else {
            self.finish_scan(member.id, stream).await?;
            return Ok(Vec::new());
        };
        if activities.is_empty() {
            self.finish_scan(member.id, stream).await?;
            return Ok(Vec::new());
        }
        let mut reached_boundary = false;
        let mut admitted_ids = Vec::new();
        self.begin_transaction().await?;
        let admitted = async {
            for activity in &activities {
                let id = activity.details.instance_id;
                if stream.boundary != 0 && id == stream.boundary {
                    reached_boundary = true;
                    break;
                }
                if [2526740498, 248695599].contains(&activity.details.director_activity_hash) {
                    continue;
                }
                sqlx::query("INSERT OR IGNORE INTO activity_queue(activity_id,character) VALUES (?,?)")
                    .bind(id).bind(stream.character).execute(&mut self.db).await?;
                sqlx::query("INSERT OR IGNORE INTO sync_history(member,activity_id,character,period) VALUES (?,?,?,?)")
                    .bind(member.id).bind(id).bind(stream.character).bind(activity.period.to_rfc3339())
                    .execute(&mut self.db).await?;
                admitted_ids.push((id, stream.character));
                recent.entry(id).and_modify(|entry| {
                    if activity.period > entry.0 { *entry = (activity.period, stream.character); }
                }).or_insert((activity.period, stream.character));
            }
            self.commit_transaction().await
        }.await;
        if let Err(error) = admitted {
            self.rollback_transaction().await?;
            return Err(error);
        }
        trim_recent(recent);
        stream.frontier = activities.last().map(|a| a.period);
        stream.page += 1;
        if reached_boundary {
            self.finish_scan(member.id, stream).await?;
        }
        Ok(admitted_ids)
    }

    async fn finish_scan(
        &mut self,
        member: i64,
        stream: &mut Stream,
    ) -> Result<(), Error> {
        sqlx::query("UPDATE sync_scan SET complete=1 WHERE member=? AND character=? AND mode=?")
            .bind(member).bind(stream.character).bind(stream.mode.as_id() as i64)
            .execute(&mut self.db).await?;
        stream.complete = true;
        Ok(())
    }

    async fn reconcile_queue(&mut self) -> Result<(), Error> {
        sqlx::query("UPDATE activity_queue SET synced=1 WHERE synced=0 AND EXISTS (SELECT 1 FROM activity WHERE activity.activity_id=activity_queue.activity_id)")
            .execute(&mut self.db).await?;
        Ok(())
    }

    #[cfg(test)]
    pub(super) async fn download_pending_for_character<F>(
        &mut self,
        character: i64,
        control: &SyncControl,
        on_event: &F,
    ) -> Result<SyncResult, Error>
    where
        F: Fn(SyncEvent),
    {
        self.reset_attempted().await?;
        self.reconcile_queue().await?;
        let mut outcome = SyncOutcome::default();
        let mut cursor = i64::MAX;
        while !control.is_cancelled() {
            let ids: Vec<i64> = sqlx::query_scalar(
                "SELECT activity_id FROM activity_queue WHERE character=? AND synced=0 AND activity_id<? ORDER BY activity_id DESC LIMIT ?")
                .bind(character).bind(cursor).bind(QUEUE_PAGE_SIZE)
                .fetch_all(&mut self.db).await?;
            if ids.is_empty() {
                break;
            }
            cursor = *ids.last().unwrap();
            outcome.discovered += ids.len() as u32;
            self.download_ids(
                ids.into_iter().map(|id| (id, character)).collect(),
                control,
                &mut outcome,
                on_event,
            )
            .await?;
        }
        Ok(SyncResult {
            total_available: outcome.discovered,
            total_synced: outcome.committed,
        })
    }

    async fn download_member_pending<F>(
        &mut self,
        member: i64,
        control: &SyncControl,
        outcome: &mut SyncOutcome,
        on_event: &F,
    ) -> Result<(), Error>
    where
        F: Fn(SyncEvent),
    {
        self.reconcile_queue().await?;
        let mut cursor = i64::MAX;
        while !control.is_cancelled() {
            let rows: Vec<(i64, i64)> = sqlx::query_as(
                "SELECT q.activity_id, MIN(q.character) FROM activity_queue q JOIN character c ON c.character_id=q.character WHERE c.member=? AND q.synced=0 AND q.activity_id<? GROUP BY q.activity_id ORDER BY q.activity_id DESC LIMIT ?")
                .bind(member).bind(cursor).bind(QUEUE_PAGE_SIZE)
                .fetch_all(&mut self.db).await?;
            if rows.is_empty() {
                break;
            }
            cursor = rows.last().unwrap().0;
            self.download_ids(rows, control, outcome, on_event).await?;
        }
        Ok(())
    }

    async fn download_ids<F>(
        &mut self,
        ids: Vec<(i64, i64)>,
        control: &SyncControl,
        outcome: &mut SyncOutcome,
        on_event: &F,
    ) -> Result<(), Error>
    where
        F: Fn(SyncEvent),
    {
        if ids.is_empty() {
            return Ok(());
        }
        let mut query: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT DISTINCT q.activity_id FROM activity_queue q WHERE q.synced=0 AND NOT EXISTS (SELECT 1 FROM activity a WHERE a.activity_id=q.activity_id) AND NOT EXISTS (SELECT 1 FROM sync_attempted t WHERE t.activity_id=q.activity_id) AND q.activity_id IN (");
        let mut separated = query.separated(",");
        for (id, _) in &ids {
            separated.push_bind(*id);
        }
        separated.push_unseparated(")");
        let selected: Vec<i64> =
            query.build_query_scalar().fetch_all(&mut self.db).await?;
        if selected.is_empty() {
            return Ok(());
        }
        let selected: HashSet<_> = selected.into_iter().collect();
        let mut seen = HashSet::new();
        let pending: Vec<_> = ids
            .into_iter()
            .filter(|(id, _)| selected.contains(id) && seen.insert(*id))
            .collect();
        let mut attempts: QueryBuilder<Sqlite> = QueryBuilder::new(
            "INSERT OR IGNORE INTO sync_attempted(activity_id) ",
        );
        attempts.push_values(&pending, |mut builder, (id, _)| {
            builder.push_bind(*id);
        });
        attempts.build().execute(&mut self.db).await?;
        let mut jobs = JoinSet::new();
        let mut next = 0;
        while next < pending.len()
            && jobs.len() < NETWORK_WINDOW
            && !control.is_cancelled()
        {
            let (id, character) = pending[next];
            let api = self.api_interface.clone();
            jobs.spawn(async move {
                (
                    id,
                    character,
                    api.retrieve_post_game_carnage_report_bounded(id).await,
                )
            });
            next += 1;
        }
        while !jobs.is_empty() {
            if control.is_cancelled() {
                jobs.abort_all();
            }
            let result = match tokio::time::timeout(
                std::time::Duration::from_millis(100),
                jobs.join_next(),
            )
            .await
            {
                Ok(Some(result)) => result,
                Ok(None) => break,
                Err(_) => continue,
            };
            if next < pending.len() && !control.is_cancelled() {
                let (id, character) = pending[next];
                let api = self.api_interface.clone();
                jobs.spawn(async move {
                    (
                        id,
                        character,
                        api.retrieve_post_game_carnage_report_bounded(id).await,
                    )
                });
                next += 1;
            }
            let mut fetched = vec![result];
            while fetched.len() < WRITE_BATCH_SIZE
                && !jobs.is_empty()
                && !control.is_cancelled()
            {
                match jobs.try_join_next() {
                    Some(result) => {
                        if next < pending.len() {
                            let (id, character) = pending[next];
                            let api = self.api_interface.clone();
                            jobs.spawn(async move { (id, character, api.retrieve_post_game_carnage_report_bounded(id).await) });
                            next += 1;
                        }
                        fetched.push(result);
                    }
                    None => break,
                }
            }
            if control.is_cancelled() {
                continue;
            }
            let written = async {
                let mut reports: BTreeMap<i64, Vec<_>> = BTreeMap::new();
                for result in fetched {
                    let (id, character, report) = match result {
                        Ok(value) => value,
                        Err(error) if error.is_cancelled() => continue,
                        Err(error) => return Err(Error::Unknown { description: error.to_string() }),
                    };
                    match report {
                        Ok((Some(report), permit)) => reports.entry(character).or_default().push((report, permit)),
                        Err(error) if error.is_unavailable() => {
                            sqlx::query("INSERT OR REPLACE INTO sync_unavailable(activity_id,reason) VALUES (?,'privacy')")
                                .bind(id).execute(&mut self.db).await?;
                            outcome.pending = outcome.pending.saturating_sub(1);
                            outcome.unavailable += 1;
                        }
                        Ok((None, _permit)) => tell::error!("PGCR {} returned no report; leaving queued", id),
                        Err(error) => tell::error!("PGCR {} remains queued: {}", id, error),
                    }
                }
                for (character, group) in reports {
                    let (mut items, permits): (Vec<_>, Vec<_>) = group.into_iter().unzip();
                    let count = self.insert_activities_batch(&mut items, &character).await?;
                    drop(permits);
                    if count > 0 {
                        outcome.committed += count;
                        outcome.pending = outcome.pending.saturating_sub(count);
                        on_event(SyncEvent { phase: SyncPhase::Committed, outcome: *outcome });
                    }
                }
                Ok::<(), Error>(())
            }.await;
            if let Err(error) = written {
                jobs.abort_all();
                while jobs.join_next().await.is_some() {}
                return Err(error);
            }
        }
        Ok(())
    }

    async fn update_outcome_coverage(
        &mut self,
        member: i64,
        outcome: &mut SyncOutcome,
    ) -> Result<(), Error> {
        let (pending, unavailable): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(DISTINCT CASE WHEN u.activity_id IS NULL THEN q.activity_id END), COUNT(DISTINCT u.activity_id) FROM activity_queue q JOIN character c ON c.character_id=q.character LEFT JOIN sync_unavailable u ON u.activity_id=q.activity_id WHERE c.member=? AND q.synced=0")
            .bind(member).fetch_one(&mut self.db).await?;
        outcome.pending = pending as u32;
        outcome.unavailable = unavailable as u32;
        Ok(())
    }
}
