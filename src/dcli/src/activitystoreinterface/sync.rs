use super::*;
use futures::future::{AbortHandle, Abortable};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::future::Future;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::task::JoinSet;

use crate::response::activities::Activity;

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
const HISTORY_WINDOW: usize = 4;
const HISTORY_STREAM_WINDOW: usize = 2;
const PAGE_INSERT_BATCH: usize = 200;
const RESUME_ANCHORS: usize = 8;
const RESUME_HEAD_PAGES: i32 = 8;
const RESUME_SEARCH_RADIUS: i32 = 8;

const HISTORY_RESUME_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sync_history_resume (
    member INTEGER NOT NULL,
    character INTEGER NOT NULL,
    mode INTEGER NOT NULL,
    version INTEGER NOT NULL,
    epoch INTEGER NOT NULL,
    boundary INTEGER NOT NULL,
    head_raw TEXT NOT NULL,
    tail_raw TEXT NOT NULL,
    page_hint INTEGER NOT NULL,
    PRIMARY KEY(member, character, mode)
);
"#;

/// A cancellation request that can be sent without borrowing the store.
#[derive(Clone)]
pub struct SyncControl {
    cancelled: Arc<AtomicBool>,
    deadline: Option<Instant>,
}

impl Default for SyncControl {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: None,
        }
    }
}

impl SyncControl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(super) fn with_deadline(&self, deadline: Instant) -> Self {
        Self {
            cancelled: self.cancelled.clone(),
            deadline: Some(deadline),
        }
    }

    fn should_stop(&self) -> bool {
        self.is_cancelled()
            || self
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
    }

    pub async fn cancelled(&self) {
        while !self.should_stop() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

pub(super) async fn await_or_cancel<F: Future>(
    control: &SyncControl,
    request: F,
) -> Option<F::Output> {
    if control.should_stop() {
        return None;
    }
    let cancelled = control.cancelled();
    futures::pin_mut!(cancelled);
    futures::pin_mut!(request);
    match futures::future::select(cancelled, request).await {
        futures::future::Either::Left(_) => None,
        futures::future::Either::Right((_value, _))
            if control.should_stop() =>
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
    pub newly_discovered: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncSliceDisposition {
    Deferred,
    Checkpointed,
    FullComplete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncSliceResult {
    pub disposition: SyncSliceDisposition,
    pub outcome: SyncOutcome,
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
    epoch: i64,
    checkpoint: Option<ResumeCheckpoint>,
    resume_pending: bool,
    head_raw: Vec<i64>,
    bridge_tail: Vec<i64>,
    head_overlap_page: Option<i32>,
}

struct ResumeCheckpoint {
    epoch: i64,
    head_raw: Vec<i64>,
    tail_raw: Vec<i64>,
    page_hint: i32,
}

fn ordered_overlap(anchors: &[i64], raw: &[i64]) -> Option<usize> {
    let mut locations = HashMap::new();
    for (index, id) in raw.iter().enumerate() {
        if locations.insert(*id, index).is_some() {
            return None;
        }
    }
    let positions: Vec<_> = anchors
        .iter()
        .enumerate()
        .filter_map(|(anchor, id)| {
            locations.get(id).map(|&position| (anchor, position))
        })
        .collect();
    (positions.len() >= 3
        && positions.windows(2).all(|pair| pair[0].1 < pair[1].1)
        && positions.windows(2).any(|pair| pair[0].0 + 1 == pair[1].0))
    .then(|| positions.last().unwrap().1)
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
    async fn ensure_resume_schema(&mut self) -> Result<(), Error> {
        sqlx::query(HISTORY_RESUME_SCHEMA)
            .execute(&mut self.db)
            .await?;
        Ok(())
    }

    async fn discard_resume(
        &mut self,
        member: i64,
        character: i64,
        mode: Mode,
    ) -> Result<(), Error> {
        sqlx::query("DELETE FROM sync_history_resume WHERE member=? AND character=? AND mode=?")
            .bind(member).bind(character).bind(mode.as_id() as i64)
            .execute(&mut self.db).await?;
        Ok(())
    }

    async fn load_resume(
        &mut self,
        member: i64,
        character: i64,
        mode: Mode,
        boundary: i64,
    ) -> Result<Option<ResumeCheckpoint>, Error> {
        let saved: Option<(i64, i64, i64, String, String, i64)> = sqlx::query_as(
            "SELECT version,epoch,boundary,head_raw,tail_raw,page_hint FROM sync_history_resume WHERE member=? AND character=? AND mode=?"
        ).bind(member).bind(character).bind(mode.as_id() as i64)
            .fetch_optional(&mut self.db).await?;
        let Some((version, epoch, saved_boundary, head, tail, hint)) = saved
        else {
            return Ok(None);
        };
        let parsed = serde_json::from_str::<Vec<i64>>(&head)
            .ok()
            .zip(serde_json::from_str::<Vec<i64>>(&tail).ok());
        if let Some((head_raw, tail_raw)) = parsed {
            let valid_ids = |ids: &[i64]| {
                ids.len() >= 3
                    && ids.len() <= RESUME_ANCHORS
                    && ids.iter().copied().collect::<HashSet<_>>().len()
                        == ids.len()
            };
            if version == 1
                && saved_boundary == boundary
                && hint >= 0
                && hint <= i32::MAX as i64
                && valid_ids(&head_raw)
                && valid_ids(&tail_raw)
            {
                return Ok(Some(ResumeCheckpoint {
                    epoch,
                    head_raw,
                    tail_raw,
                    page_hint: hint as i32,
                }));
            }
        }
        self.discard_resume(member, character, mode).await?;
        Ok(None)
    }

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
        if outcome.recent_ready || outcome.cancelled || control.should_stop() {
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
                    && stream.frontier.map_or(true, |period| period >= cutoff)
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

    async fn download_recent<F>(
        &mut self,
        recent: &HashMap<i64, (DateTime<Utc>, i64)>,
        control: &SyncControl,
        outcome: &mut SyncOutcome,
        on_event: &F,
    ) -> Result<(), Error>
    where
        F: Fn(SyncEvent),
    {
        let mut selected: Vec<_> = recent
            .iter()
            .map(|(id, (period, character))| (*id, *period, *character))
            .collect();
        selected
            .sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
        self.download_ids(
            selected
                .into_iter()
                .take(RECENT_COUNT)
                .map(|(id, _, character)| (id, character))
                .collect(),
            control,
            outcome,
            on_event,
        )
        .await
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
        self.sync_member_engine_after_pending(
            member,
            control,
            on_event,
            SyncOutcome::default(),
            true,
        )
        .await
    }

    pub(super) async fn sync_member_slice_engine<F>(
        &mut self,
        member: &Member,
        control: &SyncControl,
        budget: Duration,
        on_event: &F,
    ) -> Result<SyncSliceResult, Error>
    where
        F: Fn(SyncEvent),
    {
        let mut outcome = SyncOutcome::default();
        if control.is_cancelled() {
            outcome.cancelled = true;
            return Ok(SyncSliceResult {
                disposition: SyncSliceDisposition::Cancelled,
                outcome,
            });
        }
        if budget.is_zero() {
            return Ok(SyncSliceResult {
                disposition: SyncSliceDisposition::Deferred,
                outcome,
            });
        }
        let scoped = control.with_deadline(Instant::now() + budget);
        self.reset_attempted().await?;
        self.freeze_known_boundaries(member.id).await?;
        outcome.discovered = self.discovered_count(member.id).await?;
        self.update_outcome_coverage(member.id, &mut outcome)
            .await?;
        self.download_member_pending(
            member.id,
            &scoped,
            &mut outcome,
            on_event,
        )
        .await?;
        if !scoped.should_stop() {
            outcome = self
                .sync_member_engine_after_pending(
                    member, &scoped, on_event, outcome, false,
                )
                .await?;
        } else {
            self.update_outcome_coverage(member.id, &mut outcome)
                .await?;
            outcome.cancelled = control.is_cancelled();
        }
        let disposition = if outcome.cancelled {
            SyncSliceDisposition::Cancelled
        } else if outcome.complete {
            SyncSliceDisposition::FullComplete
        } else if outcome.committed > 0 || outcome.newly_discovered > 0 {
            SyncSliceDisposition::Checkpointed
        } else {
            SyncSliceDisposition::Deferred
        };
        Ok(SyncSliceResult {
            disposition,
            outcome,
        })
    }

    async fn freeze_known_boundaries(
        &mut self,
        member: i64,
    ) -> Result<(), Error> {
        self.ensure_resume_schema().await?;
        let characters: Vec<i64> = sqlx::query_scalar(
            "SELECT character_id FROM character WHERE member=?",
        )
        .bind(member)
        .fetch_all(&mut self.db)
        .await?;
        for character in characters {
            for mode in [Mode::AllPvP, Mode::PrivateMatchesAll] {
                let previous: Option<(i64, i64)> = sqlx::query_as(
                    "SELECT boundary,complete FROM sync_scan WHERE member=? AND character=? AND mode=?",
                ).bind(member).bind(character).bind(mode.as_id() as i64)
                    .fetch_optional(&mut self.db).await?;
                let boundary = match previous {
                    Some((boundary, 0)) => boundary,
                    _ => {
                        self.discard_resume(member, character, mode).await?;
                        self.get_max_activity_id(&character, &mode).await?
                    }
                };
                sqlx::query("INSERT INTO sync_scan(member,character,mode,boundary,complete) VALUES (?,?,?,?,0) ON CONFLICT(member,character,mode) DO UPDATE SET boundary=excluded.boundary,complete=0")
                    .bind(member).bind(character).bind(mode.as_id() as i64).bind(boundary)
                    .execute(&mut self.db).await?;
            }
        }
        Ok(())
    }

    async fn sync_member_engine_after_pending<F>(
        &mut self,
        member: &Member,
        control: &SyncControl,
        on_event: &F,
        mut outcome: SyncOutcome,
        reset_attempted: bool,
    ) -> Result<SyncOutcome, Error>
    where
        F: Fn(SyncEvent),
    {
        if control.is_cancelled() {
            outcome.cancelled = true;
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
                outcome.cancelled = control.is_cancelled();
                on_event(SyncEvent {
                    phase: if outcome.cancelled {
                        SyncPhase::Cancelled
                    } else {
                        SyncPhase::Incomplete
                    },
                    outcome,
                });
                return Ok(outcome);
            }
        };
        if control.should_stop() {
            outcome.cancelled = control.is_cancelled();
            on_event(SyncEvent {
                phase: if outcome.cancelled {
                    SyncPhase::Cancelled
                } else {
                    SyncPhase::Incomplete
                },
                outcome,
            });
            return Ok(outcome);
        }
        let profile_member = profile.user_info.to_member();
        if !reset_attempted
            && (profile_member.id != member.id
                || profile_member.platform != member.platform)
        {
            return Err(Error::InvalidParameters);
        }
        self.insert_member(&profile_member).await?;
        self.ensure_resume_schema().await?;
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
                    _ => {
                        self.discard_resume(member.id, character.id, mode)
                            .await?;
                        self.get_max_activity_id(&character.id, &mode).await?
                    }
                };
                let checkpoint = self
                    .load_resume(member.id, character.id, mode, boundary)
                    .await?;
                let epoch = match &checkpoint {
                    Some(checkpoint) => checkpoint.epoch,
                    None => {
                        sqlx::query_scalar("SELECT random()")
                            .fetch_one(&mut self.db)
                            .await?
                    }
                };
                streams.push(Stream {
                    character: character.id,
                    mode,
                    boundary,
                    page: 0,
                    complete: false,
                    frontier: None,
                    epoch,
                    resume_pending: checkpoint.is_some(),
                    checkpoint,
                    head_raw: Vec::new(),
                    bridge_tail: Vec::new(),
                    head_overlap_page: None,
                });
            }
        }
        streams.sort_by_key(|stream| (stream.character, stream.mode.as_id()));

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

        let mut recent: HashMap<i64, (DateTime<Utc>, i64)> = HashMap::new();
        if reset_attempted {
            self.reset_attempted().await?;
        }
        self.load_existing_recent(member.id, &mut recent).await?;
        outcome.discovered = self.discovered_count(member.id).await?;
        self.update_outcome_coverage(member.id, &mut outcome)
            .await?;
        let mut next_wave = 0;
        while !control.should_stop() && !outcome.recent_ready {
            let initial_frontiers = streams
                .iter()
                .any(|stream| !stream.complete && stream.page == 0);
            let cutoff = if recent.len() >= RECENT_COUNT {
                recent.values().map(|(period, _)| *period).min()
            } else {
                None
            };
            let eligible: Vec<_> = (0..streams.len())
                .map(|offset| (next_wave + offset) % streams.len())
                .filter(|&index| {
                    let stream = &streams[index];
                    !stream.complete
                        && (initial_frontiers && stream.page == 0
                            || !initial_frontiers
                                && !cutoff.is_some_and(|cutoff| {
                                    stream
                                        .frontier
                                        .is_some_and(|period| period < cutoff)
                                }))
                })
                .take(HISTORY_WINDOW)
                .collect();
            if !eligible.is_empty() {
                self.scan_pages(
                    member,
                    &mut streams,
                    &eligible,
                    1,
                    control,
                    &mut recent,
                    &mut outcome,
                    on_event,
                )
                .await?;
                next_wave = (eligible[eligible.len() - 1] + 1) % streams.len();
            }
            outcome.discovery_complete =
                streams.iter().all(|stream| stream.complete);
            if control.should_stop() {
                break;
            }
            if streams
                .iter()
                .any(|stream| !stream.complete && stream.page == 0)
            {
                continue;
            }
            self.download_recent(&recent, control, &mut outcome, on_event)
                .await?;
            self.maybe_recent_ready(
                &streams,
                &recent,
                control,
                &mut outcome,
                on_event,
            )
            .await?;
            if outcome.recent_ready
                || control.should_stop()
                || eligible.is_empty()
            {
                break;
            }
        }

        if !control.should_stop()
            && streams.iter().any(|stream| !stream.complete)
        {
            for index in 0..streams.len() {
                if control.should_stop() {
                    break;
                }
                self.prepare_resume(
                    member,
                    &mut streams,
                    index,
                    control,
                    &mut recent,
                    &mut outcome,
                    on_event,
                )
                .await?;
            }
        }
        if !control.should_stop()
            && streams.iter().any(|stream| !stream.complete)
        {
            let eligible: Vec<_> = (0..streams.len()).collect();
            self.scan_pages(
                member,
                &mut streams,
                &eligible,
                HISTORY_STREAM_WINDOW,
                control,
                &mut recent,
                &mut outcome,
                on_event,
            )
            .await?;
        }

        outcome.discovery_complete = streams.iter().all(|s| s.complete);
        outcome.discovered = self.discovered_count(member.id).await?;
        self.update_outcome_coverage(member.id, &mut outcome)
            .await?;
        if outcome.discovery_complete && !control.should_stop() {
            on_event(SyncEvent {
                phase: SyncPhase::Discovering,
                outcome,
            });
        }
        if !control.should_stop() {
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

    async fn prepare_resume<F>(
        &mut self,
        member: &Member,
        streams: &mut [Stream],
        index: usize,
        control: &SyncControl,
        recent: &mut HashMap<i64, (DateTime<Utc>, i64)>,
        outcome: &mut SyncOutcome,
        on_event: &F,
    ) -> Result<(), Error>
    where
        F: Fn(SyncEvent),
    {
        if !streams[index].resume_pending || streams[index].complete {
            return Ok(());
        }
        while !control.should_stop()
            && !streams[index].complete
            && streams[index].head_overlap_page.is_none()
            && streams[index].page < RESUME_HEAD_PAGES
        {
            self.scan_pages(
                member,
                streams,
                &[index],
                1,
                control,
                recent,
                outcome,
                on_event,
            )
            .await?;
        }
        if control.should_stop() || streams[index].complete {
            return Ok(());
        }
        let checkpoint = streams[index].checkpoint.as_ref().unwrap();
        let guess = streams[index]
            .head_overlap_page
            .and_then(|head_page| checkpoint.page_hint.checked_add(head_page));
        if let Some(guess) = guess {
            let mut probes: BTreeMap<i32, Vec<Activity>> = BTreeMap::new();
            let mut probe_order = VecDeque::new();
            for step in 0..=(RESUME_SEARCH_RADIUS * 2) {
                if control.should_stop() {
                    return Ok(());
                }
                let offset = if step == 0 {
                    0
                } else if step % 2 == 1 {
                    -(step + 1) / 2
                } else {
                    step / 2
                };
                let Some(page) = guess.checked_add(offset) else {
                    continue;
                };
                if page <= streams[index].page + 1 {
                    continue;
                }
                let api = self.api_interface.clone();
                let fetched = await_or_cancel(
                    control,
                    api.retrieve_activities(
                        &member.id,
                        &streams[index].character,
                        &member.platform,
                        &streams[index].mode,
                        HISTORY_PAGE_SIZE,
                        page,
                    ),
                )
                .await;
                let Some(fetched) = fetched else {
                    return Ok(());
                };
                match fetched {
                    Ok(Some(activities)) if !activities.is_empty() => {
                        probes.insert(page, activities);
                        probe_order.push_back(page);
                        while probe_order.len() > 3 {
                            probes.remove(&probe_order.pop_front().unwrap());
                        }
                    }
                    Ok(_) => continue,
                    Err(_) => break,
                }
                let anchors =
                    &streams[index].checkpoint.as_ref().unwrap().tail_raw;
                let mut landings = Vec::new();
                for (&probe_page, activities) in &probes {
                    let raw: Vec<_> = activities
                        .iter()
                        .map(|activity| activity.details.instance_id)
                        .collect();
                    if ordered_overlap(anchors, &raw).is_some() {
                        landings.push(probe_page);
                    }
                    if let Some(next) = probes.get(&(probe_page + 1)) {
                        let mut combined = raw;
                        combined.extend(
                            next.iter()
                                .map(|activity| activity.details.instance_id),
                        );
                        if ordered_overlap(anchors, &combined).is_some() {
                            landings.push(probe_page);
                        }
                    }
                }
                landings.sort_unstable();
                landings.dedup();
                if let Some(&landing) = landings.first() {
                    if landings.iter().any(|&page| page > landing + 1) {
                        break;
                    }
                    let activities = probes.remove(&landing).unwrap();
                    streams[index].page = landing;
                    streams[index].resume_pending = false;
                    streams[index].checkpoint = None;
                    let queue_changed = self
                        .admit_page(
                            member,
                            &mut streams[index],
                            Some(activities),
                            recent,
                            outcome,
                        )
                        .await?;
                    if !control.should_stop() {
                        if queue_changed {
                            self.update_outcome_coverage(member.id, outcome)
                                .await?;
                        }
                        on_event(SyncEvent {
                            phase: SyncPhase::Discovering,
                            outcome: *outcome,
                        });
                    }
                    return Ok(());
                }
            }
        }
        if !control.should_stop() {
            self.discard_resume(
                member.id,
                streams[index].character,
                streams[index].mode,
            )
            .await?;
            streams[index].checkpoint = None;
            streams[index].resume_pending = false;
        }
        Ok(())
    }

    async fn scan_pages<F>(
        &mut self,
        member: &Member,
        streams: &mut [Stream],
        eligible: &[usize],
        per_stream: usize,
        control: &SyncControl,
        recent: &mut HashMap<i64, (DateTime<Utc>, i64)>,
        outcome: &mut SyncOutcome,
        on_event: &F,
    ) -> Result<(), Error>
    where
        F: Fn(SyncEvent),
    {
        let mut jobs = JoinSet::new();
        let mut aborts: HashMap<(usize, i32), AbortHandle> = HashMap::new();
        let mut buffered: BTreeMap<
            (i32, usize),
            Result<Option<Vec<Activity>>, Error>,
        > = BTreeMap::new();
        let mut next_fetch: Vec<_> =
            streams.iter().map(|stream| stream.page).collect();
        let limit: Vec<_> = streams
            .iter()
            .map(|stream| {
                if per_stream == 1 {
                    Some(stream.page + 1)
                } else {
                    None
                }
            })
            .collect();
        let mut failure = None;

        while !control.should_stop() {
            while jobs.len() + buffered.len() < HISTORY_WINDOW {
                let candidate = eligible
                    .iter()
                    .copied()
                    .filter(|&index| {
                        !streams[index].complete
                            && limit[index]
                                .is_none_or(|limit| next_fetch[index] < limit)
                            && (next_fetch[index] - streams[index].page)
                                < per_stream as i32
                    })
                    .min_by_key(|&index| (next_fetch[index], index));
                let Some(index) = candidate else {
                    break;
                };
                let page = next_fetch[index];
                next_fetch[index] += 1;
                let api = self.api_interface.clone();
                let member_id = member.id;
                let platform = member.platform;
                let character = streams[index].character;
                let mode = streams[index].mode;
                let (abort, registration) = AbortHandle::new_pair();
                aborts.insert((index, page), abort);
                jobs.spawn(async move {
                    let fetched = Abortable::new(
                        api.retrieve_activities(
                            &member_id,
                            &character,
                            &platform,
                            &mode,
                            HISTORY_PAGE_SIZE,
                            page,
                        ),
                        registration,
                    )
                    .await;
                    (index, page, fetched)
                });
            }

            let next = eligible
                .iter()
                .copied()
                .filter(|&index| {
                    !streams[index].complete
                        && limit[index]
                            .is_none_or(|limit| streams[index].page < limit)
                })
                .min_by_key(|&index| (streams[index].page, index));
            let Some(index) = next else {
                break;
            };
            let page = streams[index].page;
            if let Some(fetched) = buffered.remove(&(page, index)) {
                match fetched {
                    Ok(activities) => {
                        let queue_changed = match self
                            .admit_page(
                                member,
                                &mut streams[index],
                                activities,
                                recent,
                                outcome,
                            )
                            .await
                        {
                            Ok(changed) => changed,
                            Err(error) => {
                                failure = Some(error);
                                break;
                            }
                        };
                        if streams[index].complete {
                            buffered.retain(|&(_, stream_index), _| {
                                stream_index != index
                            });
                            for (&(stream_index, _), abort) in &aborts {
                                if stream_index == index {
                                    abort.abort();
                                }
                            }
                        }
                        if control.should_stop() {
                            break;
                        }
                        if queue_changed {
                            if let Err(error) = self
                                .update_outcome_coverage(member.id, outcome)
                                .await
                            {
                                failure = Some(error);
                                break;
                            }
                        }
                        on_event(SyncEvent {
                            phase: SyncPhase::Discovering,
                            outcome: *outcome,
                        });
                    }
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                }
                continue;
            }
            if jobs.is_empty() {
                failure = Some(Error::Unknown {
                    description: "history page was not scheduled".into(),
                });
                break;
            }
            match tokio::time::timeout(
                Duration::from_millis(25),
                jobs.join_next(),
            )
            .await
            {
                Ok(Some(Ok((index, page, fetched)))) => {
                    aborts.remove(&(index, page));
                    if !streams[index].complete {
                        if let Ok(result) = fetched {
                            buffered.insert((page, index), result);
                        }
                    }
                }
                Ok(Some(Err(error))) => {
                    failure = Some(Error::Unknown {
                        description: error.to_string(),
                    });
                    break;
                }
                Ok(None) | Err(_) => {}
            }
        }
        jobs.abort_all();
        while jobs.join_next().await.is_some() {}
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(())
    }

    async fn admit_page(
        &mut self,
        member: &Member,
        stream: &mut Stream,
        page: Option<Vec<Activity>>,
        recent: &mut HashMap<i64, (DateTime<Utc>, i64)>,
        outcome: &mut SyncOutcome,
    ) -> Result<bool, Error> {
        let Some(activities) = page else {
            self.finish_scan(member.id, stream).await?;
            return Ok(false);
        };
        if activities.is_empty() {
            self.finish_scan(member.id, stream).await?;
            return Ok(false);
        }
        let raw: Vec<_> = activities
            .iter()
            .map(|activity| activity.details.instance_id)
            .collect();
        let head_raw = if stream.page == 0 {
            raw.iter().take(RESUME_ANCHORS).copied().collect::<Vec<_>>()
        } else {
            stream.head_raw.clone()
        };
        let tail_raw: Vec<_> = raw
            .iter()
            .rev()
            .take(RESUME_ANCHORS)
            .copied()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let mut reached_boundary = false;
        let mut accepted = Vec::new();
        for activity in &activities {
            if stream.boundary != 0
                && activity.details.instance_id == stream.boundary
            {
                reached_boundary = true;
                break;
            }
            if [2526740498, 248695599]
                .contains(&activity.details.director_activity_hash)
            {
                continue;
            }
            accepted.push(activity);
        }
        let mut new_ids = 0;
        let mut new_queue_rows = 0;
        self.begin_transaction().await?;
        let admitted = async {
            for batch in accepted.chunks(PAGE_INSERT_BATCH) {
                let mut queue = QueryBuilder::<Sqlite>::new(
                    "INSERT OR IGNORE INTO activity_queue(activity_id,character) ",
                );
                queue.push_values(batch, |mut row, activity| {
                    row.push_bind(activity.details.instance_id)
                        .push_bind(stream.character);
                });
                new_queue_rows += queue.build().execute(&mut self.db).await?.rows_affected();

                let mut history = QueryBuilder::<Sqlite>::new(
                    "INSERT OR IGNORE INTO sync_history(member,activity_id,character,period) ",
                );
                history.push_values(batch, |mut row, activity| {
                    row.push_bind(member.id)
                        .push_bind(activity.details.instance_id)
                        .push_bind(stream.character)
                        .push_bind(activity.period.to_rfc3339());
                });
                new_ids += history.build().execute(&mut self.db).await?.rows_affected() as u32;
            }
            if !stream.resume_pending && head_raw.len() >= 3 && tail_raw.len() >= 3
                && !reached_boundary
            {
                let head = serde_json::to_string(&head_raw).map_err(|error| Error::Unknown {
                    description: error.to_string(),
                })?;
                let tail = serde_json::to_string(&tail_raw).map_err(|error| Error::Unknown {
                    description: error.to_string(),
                })?;
                sqlx::query("INSERT INTO sync_history_resume(member,character,mode,version,epoch,boundary,head_raw,tail_raw,page_hint) VALUES (?,?,?,1,?,?,?,?,?) ON CONFLICT(member,character,mode) DO UPDATE SET version=excluded.version,epoch=excluded.epoch,boundary=excluded.boundary,head_raw=excluded.head_raw,tail_raw=excluded.tail_raw,page_hint=excluded.page_hint")
                    .bind(member.id).bind(stream.character).bind(stream.mode.as_id() as i64)
                    .bind(stream.epoch).bind(stream.boundary).bind(&head).bind(&tail).bind(stream.page)
                    .execute(&mut self.db).await?;
            }
            self.commit_transaction().await
        }.await;
        if let Err(error) = admitted {
            self.rollback_transaction().await?;
            return Err(error);
        }
        outcome.newly_discovered += new_ids;
        outcome.discovered += new_ids;
        for activity in accepted {
            let id = activity.details.instance_id;
            let period = activity.period;
            recent
                .entry(id)
                .and_modify(|entry| {
                    if period > entry.0 {
                        *entry = (period, stream.character);
                    }
                })
                .or_insert((period, stream.character));
        }
        trim_recent(recent);
        if stream.page == 0 {
            stream.head_raw = head_raw;
        }
        if stream.resume_pending {
            let mut bridge = std::mem::take(&mut stream.bridge_tail);
            bridge.extend(&raw);
            if stream.head_overlap_page.is_none()
                && ordered_overlap(
                    &stream.checkpoint.as_ref().unwrap().head_raw,
                    &bridge,
                )
                .is_some()
            {
                stream.head_overlap_page = Some(stream.page);
            }
            stream.bridge_tail = bridge
                .into_iter()
                .rev()
                .take(RESUME_ANCHORS)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
        }
        stream.frontier = activities.last().map(|a| a.period);
        stream.page += 1;
        if reached_boundary {
            self.finish_scan(member.id, stream).await?;
        }
        Ok(new_queue_rows > 0)
    }

    async fn finish_scan(
        &mut self,
        member: i64,
        stream: &mut Stream,
    ) -> Result<(), Error> {
        self.begin_transaction().await?;
        let finished = async {
            sqlx::query("UPDATE sync_scan SET complete=1 WHERE member=? AND character=? AND mode=?")
                .bind(member).bind(stream.character).bind(stream.mode.as_id() as i64)
                .execute(&mut self.db).await?;
            self.discard_resume(member, stream.character, stream.mode).await?;
            self.commit_transaction().await
        }.await;
        if let Err(error) = finished {
            self.rollback_transaction().await?;
            return Err(error);
        }
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
        while !control.should_stop() {
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
        while !control.should_stop() {
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
            && !control.should_stop()
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
            if control.should_stop() {
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
            if next < pending.len() && !control.should_stop() {
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
                && !control.should_stop()
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
            if control.should_stop() {
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
