//! Per-notebook live-sync glue.
//!
//! Owns process-lifetime state about which notebooks the user has
//! enabled "Live sync" on. When enabled, every local stroke commit
//! fans out to a `RemoteStroke.create` mutation.
//!
//! Sync model is row-by-row mirroring of the local SQLite tables to
//! AppSync/DynamoDB — no S3 binary blobs. See
//! `journal_storage::remote_notebook_store` for the data shapes.
//!
//! Persistence: process-local for now. Re-toggle after relaunch.

#![cfg(feature = "remote")]

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use journal_core::{NotebookId, PageId, Section, Stroke};
pub use journal_storage::remote_notebook_store::{NotebookVisibility, SyncProgress};
use journal_storage::remote_notebook_store::{
    NotebookSyncError, RemoteNotebookStore, SyncReport,
};

/// Result of [`pull_and_merge_notebook`]. Reports how many remote
/// rows were absent locally and got inserted.
#[derive(Debug, Default, Clone, Copy)]
pub struct PullReport {
    pub strokes_inserted: usize,
    pub strokes_skipped_duplicate: usize,
}

use crate::state::SharedState;

thread_local! {
    /// Set of notebook IDs the user has enabled live sync on for this
    /// process. Membership = "fire RemoteStroke.create on every local
    /// insert".
    static ENABLED: RefCell<HashSet<NotebookId>> = RefCell::new(HashSet::new());
    /// Pending debounced metadata-resync timer per notebook. New
    /// requests cancel the previous timer, so a flurry of sidebar
    /// refreshes (rename → flag → delete) collapses into one resync.
    static PENDING_RESYNC: RefCell<std::collections::HashMap<NotebookId, gtk4::glib::SourceId>> =
        RefCell::new(std::collections::HashMap::new());
    /// Cache: page_id → notebook_id. Without this every stroke
    /// commit triggers two SQLite lookups (get_page + get_section)
    /// just to find which notebook owns the page; for hundreds of
    /// strokes-per-second during a draw burst that adds up.
    static PAGE_TO_NOTEBOOK: RefCell<std::collections::HashMap<PageId, NotebookId>> =
        RefCell::new(std::collections::HashMap::new());
}

/// Drop the page→notebook cache. Called on notebook open so a moved
/// page doesn't keep pointing at its prior owner. Pages don't move
/// across notebooks today, but the cache lifetime is per-process so
/// belt-and-suspenders. Currently unused; kept for the page-move
/// follow-up.
#[allow(dead_code)]
pub fn clear_page_cache() {
    PAGE_TO_NOTEBOOK.with(|m| m.borrow_mut().clear());
}

pub fn is_enabled(id: NotebookId) -> bool {
    ENABLED.with(|set| set.borrow().contains(&id))
}

fn mark_enabled(id: NotebookId) {
    ENABLED.with(|set| {
        set.borrow_mut().insert(id);
    });
}

/// Public wrapper for [`mark_enabled`] so callers outside this module
/// (autosync-on-open in `window.rs`) can flip the flag without
/// triggering an immediate sync.
pub fn mark_enabled_external(id: NotebookId) {
    mark_enabled(id);
}

pub fn disable(id: NotebookId) {
    ENABLED.with(|set| {
        set.borrow_mut().remove(&id);
    });
}

/// Update only the remote Notebook row's visibility. No section /
/// page / stroke writes. Used by the "Notebook visibility…" menu
/// entry. Errors propagate to the caller for surfacing in a dialog.
pub fn set_remote_visibility(
    notebook_id: NotebookId,
    visibility: NotebookVisibility,
) -> Result<(), NotebookSyncError> {
    let mut store = RemoteNotebookStore::connect()?;
    if !store.is_signed_in() {
        return Err(NotebookSyncError::NotSignedIn);
    }
    store.set_visibility(notebook_id, visibility)
}

/// Look up a notebook's existing remote visibility. `Ok(None)` means
/// the notebook has never been pushed — caller should prompt for an
/// initial visibility. Errors indicate auth / network problems.
pub fn fetch_remote_visibility(
    notebook_id: NotebookId,
) -> Result<Option<NotebookVisibility>, NotebookSyncError> {
    let mut store = RemoteNotebookStore::connect()?;
    if !store.is_signed_in() {
        return Err(NotebookSyncError::NotSignedIn);
    }
    store.get_notebook_visibility(notebook_id)
}

/// Sync + flip the in-process live-sync flag. Caller has already
/// resolved the visibility (either from a previous remote row or a
/// first-time prompt). The GUI uses [`spawn_sync`] with
/// `also_enable_live = true` instead; this synchronous wrapper stays
/// for tests / one-shot CLIs.
#[allow(dead_code)]
pub fn enable_for_notebook(
    state: &SharedState,
    notebook_id: NotebookId,
    visibility: NotebookVisibility,
) -> Result<SyncReport, NotebookSyncError> {
    let report = sync_notebook_now(state, notebook_id, visibility)?;
    mark_enabled(notebook_id);
    Ok(report)
}

/// Pull all remote strokes for a notebook and insert any the local
/// SQLite doesn't have yet. Synchronous version retained for tests;
/// the GUI uses [`spawn_pull`] + [`apply_pulled_strokes`] so the
/// fetch doesn't block the main loop.
#[allow(dead_code)]
pub fn pull_and_merge_notebook(
    state: &SharedState,
    notebook_id: NotebookId,
) -> Result<PullReport, NotebookSyncError> {
    let mut store = RemoteNotebookStore::connect()?;
    if !store.is_signed_in() {
        return Ok(PullReport::default());
    }
    let Some(pulled) = store.pull_notebook(notebook_id)? else {
        return Ok(PullReport::default());
    };

    // Collect existing local stroke ids for every page that the
    // notebook owns. We deduplicate on the desktop's stable Stroke
    // uuid — the same id round-trips through `RemoteStroke.id`.
    use std::collections::HashSet;
    let mut local_ids: HashSet<uuid::Uuid> = HashSet::new();
    let mut report = PullReport::default();
    let st = state.borrow();
    let mut backend = st.backend.borrow_mut();
    let sections = backend
        .list_sections(notebook_id)
        .map_err(|e| NotebookSyncError::Encode(format!("list_sections: {e}")))?;
    for s in &sections {
        let pages = backend
            .list_pages(s.id)
            .map_err(|e| NotebookSyncError::Encode(format!("list_pages: {e}")))?;
        for p in &pages {
            let strokes = backend
                .list_strokes_for_page(p.id)
                .map_err(|e| NotebookSyncError::Encode(format!("list_strokes: {e}")))?;
            for s in &strokes {
                local_ids.insert(s.id);
            }
        }
    }
    for ps in &pulled.strokes {
        if local_ids.contains(&ps.id) {
            report.strokes_skipped_duplicate += 1;
            continue;
        }
        if let Err(e) = backend.insert_stroke(&ps.stroke, ps.page_id) {
            tracing::warn!(
                "notebook_sync: pull insert_stroke failed for {}: {}",
                ps.id,
                e
            );
            continue;
        }
        report.strokes_inserted += 1;
    }
    Ok(report)
}

/// One-shot sync of the local notebook to DDB. Idempotent. Blocks
/// the caller — fine for tests / one-shot CLIs but the GUI must use
/// [`spawn_sync`] so the main loop stays responsive.
#[allow(dead_code)]
pub fn sync_notebook_now(
    state: &SharedState,
    notebook_id: NotebookId,
    visibility: NotebookVisibility,
) -> Result<SyncReport, NotebookSyncError> {
    let mut store = RemoteNotebookStore::connect()?;
    if !store.is_signed_in() {
        return Err(NotebookSyncError::NotSignedIn);
    }
    let (notebook, sections, pages, strokes_per_page, deleted) =
        sync_inputs(state, notebook_id).map_err(NotebookSyncError::Encode)?;
    let report = store.sync_notebook(
        &notebook,
        &sections,
        &pages,
        &strokes_per_page,
        &deleted,
        visibility,
        &mut |_| {},
    )?;
    // Purge locally any tombstones the cloud confirmed gone.
    // Don't hard-purge `report.strokes_purged` from local — those
    // soft-deleted rows ARE our tombstone record. Removing them
    // would let `apply_pulled_strokes` re-merge the same strokes
    // on the next pull. The strokes table grows but each row is
    // small.
    let _ = report.strokes_purged.len();
    Ok(report)
}

/// Lifecycle of a background sync operation. The GUI clones the inner
/// `Arc<Mutex<SyncJobState>>` and polls it on the main thread to
/// drive a progress dialog.
pub struct SyncJob {
    pub state: Arc<Mutex<SyncJobState>>,
}

#[derive(Debug, Default, Clone)]
pub struct SyncJobState {
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub finished: Option<Result<SyncReport, String>>,
}

/// Run [`sync_notebook_now`] on a background thread. Returns a handle
/// the caller polls; thread terminates when sync completes (success or
/// failure stored in `finished`). `also_enable_live` flips the live-
/// sync flag once sync succeeds — wires the "Live sync" toggle path.
pub fn spawn_sync(
    state: &SharedState,
    notebook_id: NotebookId,
    visibility: NotebookVisibility,
    also_enable_live: bool,
) -> Result<SyncJob, NotebookSyncError> {
    // Snapshot all the inputs on the main thread first so the worker
    // owns the data and never touches the SQLite backend (which lives
    // behind a non-Send `Rc<RefCell<…>>`).
    let (notebook, sections, pages, strokes_per_page, deleted) =
        sync_inputs(state, notebook_id).map_err(NotebookSyncError::Encode)?;
    let job_state = Arc::new(Mutex::new(SyncJobState::default()));
    let worker_state = job_state.clone();

    std::thread::spawn(move || {
        let mut store = match RemoteNotebookStore::connect() {
            Ok(s) => s,
            Err(e) => {
                let mut g = worker_state.lock().unwrap();
                g.finished = Some(Err(format!("connect: {e}")));
                return;
            }
        };
        if !store.is_signed_in() {
            let mut g = worker_state.lock().unwrap();
            g.finished = Some(Err("not signed in".into()));
            return;
        }
        let progress_cell = worker_state.clone();
        let mut on_progress = move |p: SyncProgress| {
            let mut g = progress_cell.lock().unwrap();
            match p {
                SyncProgress::Phase { label, total } => {
                    g.phase = label;
                    g.done = 0;
                    g.total = total;
                }
                SyncProgress::Step { done, total } => {
                    g.done = done;
                    g.total = total;
                }
            }
        };
        let res = store.sync_notebook(
            &notebook,
            &sections,
            &pages,
            &strokes_per_page,
            &deleted,
            visibility,
            &mut on_progress,
        );
        let mut g = worker_state.lock().unwrap();
        g.finished = Some(res.map_err(|e| format!("{e:#}")));
    });

    if also_enable_live {
        // Stamp the in-process flag synchronously so on_local_stroke_created
        // starts publishing as soon as the worker is in flight; if the
        // worker fails we'll roll the flag back from the caller side.
        mark_enabled(notebook_id);
    }

    Ok(SyncJob { state: job_state })
}

/// Background variant of [`pull_and_merge_notebook`]. Same
/// poll-via-`Arc<Mutex<…>>` shape as [`spawn_sync`]. The merge step
/// (which touches the SQLite backend) runs back on the main thread
/// when the caller drains the resulting strokes; the worker only owns
/// the network fetch.
pub struct PullJob {
    pub state: Arc<Mutex<PullJobState>>,
}

#[derive(Debug, Default)]
pub struct PullJobState {
    pub phase: String,
    pub strokes_pulled: usize,
    pub finished: Option<
        Result<Vec<journal_storage::remote_notebook_store::PulledStroke>, String>,
    >,
}

pub fn spawn_pull(notebook_id: NotebookId) -> PullJob {
    let job = Arc::new(Mutex::new(PullJobState {
        phase: "Checking cloud".into(),
        ..Default::default()
    }));
    let worker = job.clone();
    std::thread::spawn(move || {
        let mut store = match RemoteNotebookStore::connect() {
            Ok(s) => s,
            Err(e) => {
                worker.lock().unwrap().finished = Some(Err(format!("connect: {e}")));
                return;
            }
        };
        if !store.is_signed_in() {
            // Treat as nothing to pull — not signed in.
            worker.lock().unwrap().finished = Some(Ok(Vec::new()));
            return;
        }
        worker.lock().unwrap().phase = "Fetching strokes from cloud".into();
        let res = match store.pull_notebook(notebook_id) {
            Ok(Some(p)) => Ok(p.strokes),
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(format!("{e:#}")),
        };
        if let Ok(ref v) = res {
            worker.lock().unwrap().strokes_pulled = v.len();
        }
        worker.lock().unwrap().finished = Some(res);
    });
    PullJob { state: job }
}

/// Apply a finished [`PullJob`]'s strokes to the local SQLite. Runs
/// on the main thread — touches the non-Send `dyn JournalBackend`.
pub fn apply_pulled_strokes(
    state: &SharedState,
    notebook_id: NotebookId,
    pulled: Vec<journal_storage::remote_notebook_store::PulledStroke>,
) -> PullReport {
    let mut report = PullReport::default();
    let st = state.borrow();
    let mut backend = st.backend.borrow_mut();
    let sections = match backend.list_sections(notebook_id) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("apply_pulled_strokes: list_sections: {e}");
            return report;
        }
    };
    let mut local_ids: HashSet<uuid::Uuid> = HashSet::new();
    for s in &sections {
        if let Ok(pages) = backend.list_pages(s.id) {
            for p in pages {
                if let Ok(strokes) = backend.list_strokes_for_page(p.id) {
                    for st in strokes {
                        local_ids.insert(st.id);
                    }
                }
            }
        }
    }
    for ps in pulled {
        if local_ids.contains(&ps.id) {
            report.strokes_skipped_duplicate += 1;
            continue;
        }
        // Skip strokes the user already deleted locally — re-merging
        // would resurrect a stroke they erased before the cloud
        // sync caught up. The next `sync_notebook` cleans up the
        // remote orphan via the soft-delete-push pass.
        if matches!(backend.is_stroke_deleted(ps.id), Ok(true)) {
            report.strokes_skipped_duplicate += 1;
            continue;
        }
        if let Err(e) = backend.insert_stroke(&ps.stroke, ps.page_id) {
            tracing::warn!("apply_pulled_strokes: insert {} failed: {}", ps.id, e);
            continue;
        }
        report.strokes_inserted += 1;
    }
    report
}

/// Schedule a silent re-sync of the notebook's section / page
/// metadata 800 ms in the future. Coalesces back-to-back requests
/// (rename → flag → delete in quick succession all collapse to one
/// run). No-op when live sync is off for the notebook.
///
/// Strokes are pushed inline via [`on_local_stroke_created`] for low
/// latency; pages / sections / renames / reorders / deletes flow
/// through this debounced path. Reuses the pre-pull diff inside
/// [`sync_notebook_now`] so an empty-delta resync is cheap.
pub fn request_metadata_resync(state: &SharedState, notebook_id: NotebookId) {
    if !is_enabled(notebook_id) {
        return;
    }
    PENDING_RESYNC.with(|map| {
        if let Some(prev) = map.borrow_mut().remove(&notebook_id) {
            prev.remove();
        }
    });
    let state = state.clone();
    let id = gtk4::glib::timeout_add_local(
        std::time::Duration::from_millis(800),
        move || {
            PENDING_RESYNC.with(|map| {
                map.borrow_mut().remove(&notebook_id);
            });
            // Look up visibility silently — defaults to PRIVATE if
            // somehow gone. Visibility shouldn't change here, just
            // mirrors the existing remote.
            let visibility = match fetch_remote_visibility(notebook_id) {
                Ok(Some(v)) => v,
                Ok(None) => NotebookVisibility::Private,
                Err(e) => {
                    tracing::warn!(
                        "notebook_sync: visibility lookup failed during resync of {:?}: {}",
                        notebook_id,
                        e
                    );
                    return gtk4::glib::ControlFlow::Break;
                }
            };
            match spawn_sync(&state, notebook_id, visibility, false) {
                Ok(_) => {
                    tracing::debug!(
                        "notebook_sync: debounced metadata resync started for {:?}",
                        notebook_id
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "notebook_sync: debounced resync spawn failed for {:?}: {}",
                        notebook_id,
                        e
                    );
                }
            }
            gtk4::glib::ControlFlow::Break
        },
    );
    PENDING_RESYNC.with(|map| {
        map.borrow_mut().insert(notebook_id, id);
    });
}

// ── Background publish queue ────────────────────────────────────────
//
// Eraser / delete-selection fires N stroke deletes back-to-back; each
// `publish_stroke_*` call is a blocking HTTPS round-trip. Doing it on
// the GTK main thread freezes the canvas. The queue below punts the
// network work to a single worker thread the first time it's needed
// — main thread just `Sender::send`s and returns.

#[derive(Debug, serde::Serialize, serde::Deserialize)]
enum PublishEvent {
    Created {
        notebook_id: NotebookId,
        page_id: PageId,
        stroke: Stroke,
        /// Wall-clock at the moment the local SQLite write committed.
        /// This is the LWW truth clock the cloud uses to discriminate
        /// concurrent ops. Stamped at enqueue time, NOT at worker
        /// batch-encode time, so two workers racing on the same id
        /// agree on which write is newer.
        enqueued_at: String,
    },
    Updated {
        stroke: Stroke,
        enqueued_at: String,
    },
    /// Carries `notebook_id` so the worker thread can find the
    /// owning `.journal` file and hard-purge the soft-deleted row
    /// once the cloud confirms the delete.
    Deleted {
        notebook_id: NotebookId,
        stroke_id: uuid::Uuid,
        enqueued_at: String,
    },
    Replaced {
        notebook_id: NotebookId,
        page_id: PageId,
        old_id: uuid::Uuid,
        new_strokes: Vec<Stroke>,
        enqueued_at: String,
    },
}

/// Persistent on-disk queue. Survives crashes / abrupt closes. Any
/// op the user triggered locally but the workers didn't yet
/// acknowledge sits here until a future session drains it.
mod queue {
    use rusqlite::{params, Connection, OptionalExtension};
    use std::path::PathBuf;
    use std::sync::Mutex;

    use super::PublishEvent;

    fn db_path() -> PathBuf {
        let base = dirs::data_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("journal").join("sync_queue.db")
    }

    /// Bumped whenever the on-disk payload format changes. We drop +
    /// recreate the `pending` table when the stored version doesn't
    /// match — better than silently reading garbage from the prior
    /// format and losing every queued op.
    /// v3: PublishEvent::Deleted gained `notebook_id` so the
    /// worker can hard-purge the soft-deleted local row immediately
    /// after the cloud confirms.
    /// v4: every PublishEvent variant gained `enqueued_at` so the
    /// LWW timestamp seen by the cloud is the actual op time, not
    /// the worker's later batch-encode time.
    /// v5: PublishEvent variants renamed (StrokeCreated→Created etc.).
    /// JSON serialization writes the variant name verbatim, so rows
    /// from older sessions deserialize as garbage; drop them.
    const SCHEMA_VERSION: i32 = 5;

    fn open() -> rusqlite::Result<Connection> {
        let path = db_path();
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let c = Connection::open(&path)?;
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_meta(
                key TEXT PRIMARY KEY,
                value INTEGER NOT NULL
            );",
        )?;
        let stored: i32 = c
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'version'",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        if stored != SCHEMA_VERSION {
            tracing::info!(
                "notebook_sync queue: schema {} -> {} — dropping legacy rows",
                stored,
                SCHEMA_VERSION
            );
            c.execute_batch("DROP TABLE IF EXISTS pending;")?;
            c.execute(
                "INSERT OR REPLACE INTO schema_meta(key, value) VALUES ('version', ?1)",
                params![SCHEMA_VERSION],
            )?;
        }
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS pending(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                payload BLOB NOT NULL,
                created_at TEXT NOT NULL
            );",
        )?;
        Ok(c)
    }

    /// Single global connection guarded by a mutex. SQLite handles
    /// the actual write serialization; the mutex just keeps the
    /// `Connection` from being shared across threads (it isn't
    /// `Sync`).
    fn conn() -> &'static Mutex<Connection> {
        use std::sync::OnceLock;
        static CONN: OnceLock<Mutex<Connection>> = OnceLock::new();
        CONN.get_or_init(|| {
            let c = open().expect("open sync_queue.db");
            Mutex::new(c)
        })
    }

    pub fn push(ev: &PublishEvent) -> rusqlite::Result<i64> {
        // Use serde_json instead of bincode — PublishEvent transitively
        // contains types whose serde impls call `deserialize_any`
        // (bincode 1.x doesn't support that). JSON is ~2× larger than
        // bincode but the queue is tiny relative to the strokes
        // themselves, and JSON is debuggable from the sqlite shell.
        let payload = serde_json::to_vec(ev).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(e.to_string())))
        })?;
        let now = chrono::Utc::now().to_rfc3339();
        let c = conn().lock().unwrap();
        c.execute(
            "INSERT INTO pending(payload, created_at) VALUES (?1, ?2)",
            params![payload, now],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// Re-insert a row that failed to publish so it gets retried on
    /// the next worker wake. Used when a worker's HTTP call returns
    /// a transient error; persistent errors (e.g. ConditionalCheck —
    /// row already gone) skip the requeue and just drop the op.
    pub fn requeue(ev: &PublishEvent) -> rusqlite::Result<()> {
        push(ev).map(|_| ())
    }

    /// Atomically claim up to `limit` oldest rows in one DELETE...
    /// RETURNING. Used by the batch worker to grab a chunk and send
    /// them as a single AppSync `syncStrokesBatch` mutation.
    pub fn claim_batch(limit: usize) -> rusqlite::Result<Vec<(i64, PublishEvent)>> {
        let c = conn().lock().unwrap();
        let mut stmt = c.prepare(
            "DELETE FROM pending WHERE id IN (SELECT id FROM pending ORDER BY id LIMIT ?1) RETURNING id, payload",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, payload) = row?;
            match serde_json::from_slice::<PublishEvent>(&payload) {
                Ok(ev) => out.push((id, ev)),
                Err(e) => {
                    tracing::warn!(
                        "notebook_sync queue: dropped unparseable row {} ({}) in batch",
                        id,
                        e
                    );
                }
            }
        }
        Ok(out)
    }

    pub fn count() -> rusqlite::Result<usize> {
        let c = conn().lock().unwrap();
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM pending", [], |r| r.get(0))
            .optional()?
            .unwrap_or(0);
        Ok(n as usize)
    }
}

/// Wakeup primitive. Workers park on the Condvar; producers call
/// `notify_one()` after each `queue::push`. Using a Condvar instead of
/// `mpsc::Receiver` lets all N workers wait simultaneously — the
/// previous design (Arc<Mutex<Receiver>>) serialized them on the
/// mutex so only one could `recv` at a time and the worker pool
/// effectively had concurrency 1.
static WAKE: OnceLock<(Mutex<()>, Condvar)> = OnceLock::new();

/// Per-stroke lock registry. Workers hold the lock(s) for the
/// stroke ids touched by their op while the HTTP call is in flight,
/// so two ops on the SAME id (e.g. create-then-delete) serialize
/// even when many workers run in parallel. Different ids always run
/// concurrently — we get parallelism + ordering.
static STROKE_LOCKS: OnceLock<Mutex<std::collections::HashMap<uuid::Uuid, Arc<Mutex<()>>>>> =
    OnceLock::new();

fn lock_for(id: uuid::Uuid) -> Arc<Mutex<()>> {
    let map = STROKE_LOCKS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut g = map.lock().unwrap();
    g.entry(id)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn ids_touched(ev: &PublishEvent) -> Vec<uuid::Uuid> {
    let mut out = match ev {
        PublishEvent::Created { stroke, .. } => vec![stroke.id],
        PublishEvent::Updated { stroke, .. } => vec![stroke.id],
        PublishEvent::Deleted { stroke_id, .. } => vec![*stroke_id],
        PublishEvent::Replaced {
            old_id,
            new_strokes,
            ..
        } => {
            let mut v = vec![*old_id];
            v.extend(new_strokes.iter().map(|s| s.id));
            v
        }
    };
    // Sort to give a global lock order — prevents deadlock when two
    // workers each hold one of {A, B} and want the other.
    out.sort();
    out.dedup();
    out
}

fn wake_pair() -> &'static (Mutex<()>, Condvar) {
    WAKE.get_or_init(|| (Mutex::new(()), Condvar::new()))
}

/// Counter of events that have been enqueued but not yet completed.
/// Producer side increments on `enqueue`, worker decrements after the
/// mutation returns (success or error). Drives the shutdown drain.
static IN_FLIGHT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Lazy spawn of the worker pool on first need. Idempotent — the
/// `OnceLock` inside ensures it runs once.
static POOL_INIT: OnceLock<()> = OnceLock::new();

fn ensure_pool() {
    POOL_INIT.get_or_init(|| {
        let count = crate::config::load().sync_worker_count.clamp(1, 16);
        tracing::info!("notebook_sync: spawning {count} worker thread(s)");
        for n in 0..count {
            std::thread::Builder::new()
                .name(format!("notebook_sync_worker_{n}"))
                .spawn(worker_loop)
                .expect("spawn notebook_sync_worker");
        }
        // Bump in_flight to match what's already on disk so the
        // close-time drain knows there's pre-existing work.
        if let Ok(n) = queue::count() {
            if n > 0 {
                IN_FLIGHT.fetch_add(n, std::sync::atomic::Ordering::SeqCst);
                tracing::info!(
                    "notebook_sync: resuming with {} pending op(s) from disk queue",
                    n
                );
            }
        }
        // Wake any worker that might be parked.
        wake_pair().1.notify_all();
    });
}

/// Open the per-notebook `.journal` SQLite file directly from a
/// worker thread (which doesn't have access to `SharedState` /
/// `MultiFileSqliteBackend`) and hard-DELETE the named strokes.
/// CURRENTLY UNUSED — keeping the helper for an eventual periodic
/// "compact tombstones older than N days" path. Hard-purging
/// immediately after cloud-delete removes the tombstone that
/// `apply_pulled_strokes` relies on to block a future pull from
/// re-merging the same stroke.
#[allow(dead_code)]
fn purge_local_strokes(notebook_id: NotebookId, ids: &[uuid::Uuid]) {
    if ids.is_empty() {
        return;
    }
    let base = match dirs::data_dir().or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
    {
        Some(b) => b,
        None => return,
    };
    let path = base
        .join("journal")
        .join("journals")
        .join(format!("{}.journal", notebook_id.0));
    if !path.exists() {
        return;
    }
    let conn = match rusqlite::Connection::open(&path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("notebook_sync purge: open {:?}: {}", path, e);
            return;
        }
    };
    for id in ids {
        let blob = id.as_bytes().to_vec();
        if let Err(e) = conn.execute(
            "DELETE FROM strokes WHERE id = ?1 AND deleted_at IS NOT NULL",
            rusqlite::params![blob],
        ) {
            tracing::debug!("notebook_sync purge {}: {}", id, e);
        }
    }
    tracing::debug!(
        "notebook_sync: hard-purged {} stroke(s) from {:?}",
        ids.len(),
        notebook_id
    );
}

/// DDB BatchWriteItem cap is 25; the Lambda chunks accordingly so
/// we send up to this per AppSync round-trip. Pulling a bigger
/// chunk per worker mostly wastes memory + ack latency.
const WORKER_BATCH_SIZE: usize = 25;

fn worker_loop() {
    let mut store: Option<RemoteNotebookStore> = None;
    loop {
        {
            let (lock, cvar) = wake_pair();
            let mut guard = lock.lock().unwrap();
            while IN_FLIGHT.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                guard = cvar.wait(guard).unwrap();
            }
        }
        loop {
            let batch = match queue::claim_batch(WORKER_BATCH_SIZE) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("STROKE_MOD worker claim_batch FAILED: {}", e);
                    break;
                }
            };
            if batch.is_empty() {
                tracing::trace!("STROKE_MOD worker: queue drained, parking");
                break;
            }
            tracing::debug!(
                "STROKE_MOD worker claimed {} ops from queue (in_flight={})",
                batch.len(),
                IN_FLIGHT.load(std::sync::atomic::Ordering::SeqCst)
            );
            if store.is_none() {
                match RemoteNotebookStore::connect() {
                    Ok(s) => store = Some(s),
                    Err(e) => {
                        tracing::warn!("notebook_sync worker: connect failed: {}", e);
                        for (_, ev) in &batch {
                            let _ = queue::requeue(ev);
                        }
                        break;
                    }
                }
            }
            let s = store.as_mut().expect("set above");
            if !s.is_signed_in() {
                tracing::debug!("notebook_sync worker: not signed in, requeueing batch");
                for (_, ev) in &batch {
                    let _ = queue::requeue(ev);
                }
                store = None;
                break;
            }
            // Acquire per-stroke locks for every id in the batch
            // (sorted to avoid deadlock between concurrent workers).
            let mut all_ids: Vec<uuid::Uuid> = batch
                .iter()
                .flat_map(|(_, ev)| ids_touched(ev))
                .collect();
            all_ids.sort();
            all_ids.dedup();
            let locks: Vec<_> = all_ids.into_iter().map(lock_for).collect();
            let _guards: Vec<_> = locks.iter().map(|l| l.lock().unwrap()).collect();

            // Group the batch into the Lambda's create/delete
            // arrays. Updates are sent as creates (PutItem upserts).
            // Replaces split into delete + creates. Group by
            // notebook_id so each call targets a single notebook
            // (matches Lambda signature).
            //
            // Each tuple carries the per-event `enqueued_at` so the
            // cloud LWW comparison uses the actual op time, not the
            // worker's batch-encode time.
            type CreateRow = (uuid::Uuid, PageId, String, String);
            type DeleteRow = (uuid::Uuid, String);
            type Group = (Vec<CreateRow>, Vec<DeleteRow>);
            let mut by_nb: std::collections::HashMap<NotebookId, Group> =
                std::collections::HashMap::new();
            for (_, ev) in &batch {
                match ev {
                    PublishEvent::Created {
                        notebook_id,
                        page_id,
                        stroke,
                        enqueued_at,
                    } => {
                        let entry = by_nb.entry(*notebook_id).or_default();
                        let body = match serde_json::to_string(stroke) {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::warn!("encode stroke {}: {}", stroke.id, e);
                                continue;
                            }
                        };
                        entry.0.push((stroke.id, *page_id, body, enqueued_at.clone()));
                    }
                    PublishEvent::Updated { stroke, .. } => {
                        // Updates need a notebook_id + page_id, which
                        // StrokeUpdated doesn't carry. Fall back to
                        // the per-row update path for these — they're
                        // rare (selection move/scale).
                        if let Err(e) = s.publish_stroke_update(stroke) {
                            tracing::warn!("update {}: {}", stroke.id, e);
                        }
                    }
                    PublishEvent::Deleted {
                        notebook_id,
                        stroke_id,
                        enqueued_at,
                    } => {
                        let entry = by_nb.entry(*notebook_id).or_default();
                        entry.1.push((*stroke_id, enqueued_at.clone()));
                    }
                    PublishEvent::Replaced {
                        notebook_id,
                        page_id,
                        old_id,
                        new_strokes,
                        enqueued_at,
                    } => {
                        let entry = by_nb.entry(*notebook_id).or_default();
                        entry.1.push((*old_id, enqueued_at.clone()));
                        for st in new_strokes {
                            let body = match serde_json::to_string(st) {
                                Ok(s) => s,
                                Err(e) => {
                                    tracing::warn!("encode stroke {}: {}", st.id, e);
                                    continue;
                                }
                            };
                            entry.0.push((st.id, *page_id, body, enqueued_at.clone()));
                        }
                    }
                }
            }

            // Fire one upsert batch per notebook group. Creates +
            // deletes both flow as upsert items; cloud LWW resolves
            // on each item's own `updated_at`.
            use journal_storage::remote_notebook_store::UpsertStrokeItem;
            let mut failed_ids_total: std::collections::HashSet<uuid::Uuid> =
                std::collections::HashSet::new();
            let mut http_error = false;
            for (nb_id, (creates, deletes)) in &by_nb {
                let mut items: Vec<UpsertStrokeItem> =
                    Vec::with_capacity(creates.len() + deletes.len());
                for (id, page_id, body, ts) in creates {
                    items.push(UpsertStrokeItem {
                        id: *id,
                        page_id: *page_id,
                        payload: body.clone(),
                        created_at: ts.clone(),
                        updated_at: ts.clone(),
                        deleted_at: None,
                    });
                }
                for (id, ts) in deletes {
                    items.push(UpsertStrokeItem {
                        id: *id,
                        page_id: PageId(uuid::Uuid::nil()),
                        payload: String::new(),
                        created_at: ts.clone(),
                        updated_at: ts.clone(),
                        deleted_at: Some(ts.clone()),
                    });
                }
                tracing::info!(
                    "notebook_sync STROKE_MOD upsert: {} items (creates={} deletes={}) on {:?}",
                    items.len(),
                    creates.len(),
                    deletes.len(),
                    nb_id
                );
                for it in &items {
                    tracing::debug!(
                        "  STROKE_MOD item id={} page={:?} payload_len={} updated_at={} deleted={}",
                        it.id,
                        it.page_id,
                        it.payload.len(),
                        it.updated_at,
                        it.deleted_at.is_some(),
                    );
                }
                match s.upsert_strokes_batch(*nb_id, &items) {
                    Ok((upserted, unprocessed, failed_ids)) => {
                        tracing::info!(
                            "notebook_sync STROKE_MOD upsert OK: {} upserted, {} unprocessed, {} failedIds",
                            upserted,
                            unprocessed,
                            failed_ids.len()
                        );
                        for fid in failed_ids {
                            failed_ids_total.insert(fid);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("notebook_sync STROKE_MOD upsert FAILED: {}", e);
                        http_error = true;
                        // Whole call failed — every id in this notebook
                        // group needs requeue.
                        for it in &items {
                            failed_ids_total.insert(it.id);
                        }
                    }
                }
            }

            let n = batch.len();
            // Partition the original events: those whose ids the cloud
            // marked failed get re-queued for retry; the rest count as
            // done and decrement in_flight.
            let mut requeued = 0usize;
            let mut completed = 0usize;
            for (_row, ev) in &batch {
                let touched: Vec<uuid::Uuid> = ids_touched(ev);
                let needs_retry = touched.iter().any(|id| failed_ids_total.contains(id));
                if needs_retry {
                    if let Err(e) = queue::requeue(ev) {
                        tracing::warn!("STROKE_MOD worker requeue FAILED: {}", e);
                        completed += 1; // give up — don't leak in_flight
                    } else {
                        requeued += 1;
                    }
                } else {
                    completed += 1;
                }
            }
            if completed > 0 {
                let prior = IN_FLIGHT.fetch_sub(completed, std::sync::atomic::Ordering::SeqCst);
                tracing::debug!(
                    "STROKE_MOD worker batch DONE: -{} (in_flight {} -> {}), requeued {}",
                    completed,
                    prior,
                    prior.saturating_sub(completed),
                    requeued
                );
            } else if requeued > 0 {
                tracing::warn!(
                    "STROKE_MOD worker batch ALL_REQUEUED: {} ops back to queue",
                    requeued
                );
            }
            // Back-off only when the failure was a hard HTTP error
            // (network / auth / 5xx). Per-item LWW losses are normal.
            if http_error && requeued > 0 {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            let _ = n;
        }
    }
}

fn enqueue(ev: PublishEvent) {
    ensure_pool();
    let kind = match &ev {
        PublishEvent::Created { stroke, .. } => format!("create id={}", stroke.id),
        PublishEvent::Updated { stroke, .. } => format!("update id={}", stroke.id),
        PublishEvent::Deleted { stroke_id, .. } => format!("delete id={}", stroke_id),
        PublishEvent::Replaced { old_id, new_strokes, .. } => {
            format!("replace old={} children={}", old_id, new_strokes.len())
        }
    };
    // Persist BEFORE signalling so a crash between insert + notify
    // still leaves the op recoverable on next launch.
    match queue::push(&ev) {
        Ok(row_id) => {
            let prior = IN_FLIGHT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tracing::debug!(
                "STROKE_MOD enqueue: {} -> queue row {} (in_flight {} -> {})",
                kind,
                row_id,
                prior,
                prior + 1
            );
        }
        Err(e) => {
            tracing::warn!("STROKE_MOD enqueue FAILED ({}): queue push: {}", kind, e);
            return;
        }
    }
    // Wake one worker. Workers loop until queue is empty so a
    // single notify per enqueue is enough; the next enqueue notifies
    // again to catch a worker that just finished a previous batch.
    wake_pair().1.notify_one();
}

/// Number of events queued or being processed. Returns 0 once the
/// publish queue has drained.
pub fn in_flight() -> usize {
    IN_FLIGHT.load(std::sync::atomic::Ordering::SeqCst)
}

/// Hook called from each `insert_stroke` site after the local SQLite
/// write succeeds. No-op when sync is off for the page's notebook.
/// Failures log but never bubble — drawing latency must not block on
/// the network.
pub fn on_local_stroke_created(state: &SharedState, page_id: PageId, stroke: &Stroke) {
    let Some(notebook_id) = resolve_notebook(state, page_id) else {
        tracing::debug!("notebook_sync: create hook skipped — no notebook for {:?}", page_id);
        return;
    };
    if !is_enabled(notebook_id) {
        tracing::debug!(
            "notebook_sync: create hook skipped — sync OFF for {:?} (stroke {})",
            notebook_id,
            stroke.id
        );
        return;
    }
    tracing::debug!("notebook_sync: enqueue create {} for {:?}", stroke.id, notebook_id);
    enqueue(PublishEvent::Created {
        notebook_id,
        page_id,
        stroke: stroke.clone(),
        enqueued_at: chrono::Utc::now().to_rfc3339(),
    });
}

/// Hook for stroke deletions (eraser, lasso delete, undo of an add).
pub fn on_local_stroke_deleted(state: &SharedState, page_id: PageId, stroke_id: uuid::Uuid) {
    let Some(notebook_id) = resolve_notebook(state, page_id) else {
        tracing::debug!("notebook_sync: delete hook skipped — no notebook for {:?}", page_id);
        return;
    };
    if !is_enabled(notebook_id) {
        tracing::debug!(
            "notebook_sync: delete hook skipped — sync OFF for {:?} (stroke {})",
            notebook_id,
            stroke_id
        );
        return;
    }
    tracing::debug!("notebook_sync: enqueue delete {} for {:?}", stroke_id, notebook_id);
    enqueue(PublishEvent::Deleted {
        notebook_id,
        stroke_id,
        enqueued_at: chrono::Utc::now().to_rfc3339(),
    });
}

/// Hook for stroke geometry changes (move, scale, lasso split,
/// partial-erase replacement). Updates the remote row's
/// `strokeJson` so subscribers see the new shape.
pub fn on_local_stroke_updated(state: &SharedState, page_id: PageId, stroke: &Stroke) {
    let Some(notebook_id) = resolve_notebook(state, page_id) else {
        return;
    };
    if !is_enabled(notebook_id) {
        return;
    }
    tracing::debug!("notebook_sync: enqueue update {} for {:?}", stroke.id, notebook_id);
    enqueue(PublishEvent::Updated {
        stroke: stroke.clone(),
        enqueued_at: chrono::Utc::now().to_rfc3339(),
    });
}

/// Hook for "replace one stroke with N children" (partial erase, lasso
/// split). Deletes the original row remotely, then creates each child.
pub fn on_local_stroke_replaced(
    state: &SharedState,
    page_id: PageId,
    old_id: uuid::Uuid,
    new_strokes: &[Stroke],
) {
    let Some(notebook_id) = resolve_notebook(state, page_id) else {
        return;
    };
    if !is_enabled(notebook_id) {
        return;
    }
    tracing::debug!(
        "notebook_sync: enqueue replace {} -> {} children for {:?}",
        old_id,
        new_strokes.len(),
        notebook_id
    );
    enqueue(PublishEvent::Replaced {
        notebook_id,
        page_id,
        old_id,
        new_strokes: new_strokes.to_vec(),
        enqueued_at: chrono::Utc::now().to_rfc3339(),
    });
}

fn resolve_notebook(state: &SharedState, page_id: PageId) -> Option<NotebookId> {
    if let Some(hit) = PAGE_TO_NOTEBOOK.with(|m| m.borrow().get(&page_id).copied()) {
        return Some(hit);
    }
    let nb = {
        let st = state.borrow();
        let mut backend = st.backend.borrow_mut();
        let page = backend.get_page(page_id).ok()?;
        let section = backend.get_section(page.section_id).ok()?;
        section.notebook_id
    };
    PAGE_TO_NOTEBOOK.with(|m| m.borrow_mut().insert(page_id, nb));
    Some(nb)
}

fn sync_inputs(
    state: &SharedState,
    notebook_id: NotebookId,
) -> Result<
    (
        journal_core::Notebook,
        Vec<Section>,
        Vec<journal_core::Page>,
        Vec<(PageId, Vec<Stroke>)>,
        Vec<uuid::Uuid>,
    ),
    String,
> {
    let st = state.borrow();
    let mut backend = st.backend.borrow_mut();
    let notebook = backend
        .get_notebook(notebook_id)
        .map_err(|e| format!("get_notebook: {e}"))?;
    let sections = backend
        .list_sections(notebook_id)
        .map_err(|e| format!("list_sections: {e}"))?;
    let mut pages = Vec::new();
    for s in &sections {
        let mut p = backend
            .list_pages(s.id)
            .map_err(|e| format!("list_pages: {e}"))?;
        pages.append(&mut p);
    }
    let mut strokes_per_page = Vec::new();
    for p in &pages {
        let s = backend
            .list_strokes_for_page(p.id)
            .map_err(|e| format!("list_strokes_for_page: {e}"))?;
        strokes_per_page.push((p.id, s));
    }
    // Locally soft-deleted strokes still pending a cloud-delete push.
    let deleted = backend
        .list_deleted_strokes(notebook_id)
        .map_err(|e| format!("list_deleted_strokes: {e}"))?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    Ok((notebook, sections, pages, strokes_per_page, deleted))
}
