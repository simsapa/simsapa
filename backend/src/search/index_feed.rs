//! Feeding one index build step to tantivy so that it can report how far
//! tantivy has got and can stop at once on a cancel.
//!
//! `IndexWriter::add_document()` only queues a document: tantivy's workers
//! index it later, and the queue holds up to 10,000 documents. For the sutta
//! steps that is over a minute of work, so neither a count of the documents
//! added nor a cancel check in the adding loop says anything about the work
//! still to do. `commit()` waits for the whole queue, and so does dropping the
//! writer.
//!
//! Each document is therefore wrapped in a [`TrackedDoc`]:
//!
//! - tantivy drops a document when it has indexed it, and the drop counts it
//!   as done, so the progress follows tantivy's work, also during `commit()`;
//! - after a cancel, a queued document shows tantivy no fields, so the queue
//!   is worked off in moments. The step then drops its writer uncommitted.
//!
//! See `docs/fulltext-index-storage-and-file-locking.md` §10.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use anyhow::{anyhow, Result};
use tantivy::schema::document::Document;
use tantivy::{Directory, Index, IndexWriter, TantivyDocument};

/// How often the step's calling thread looks at the cancel flag and reports
/// progress while the step runs.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// What one build step did: the documents it added, or that it stopped on the
/// cancel flag. A cancelled step's documents are not committed, unless the
/// cancel arrived while its `commit()` was already running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StepResult {
    Done(u64),
    Cancelled,
}

/// Shared by the documents of one step, the thread adding them and the thread
/// reporting progress.
#[derive(Default)]
struct StepTracker {
    /// Rows finished with: skipped by the step, or indexed by tantivy.
    done: AtomicU64,
    /// Set on cancel. Documents not yet indexed are indexed as empty ones.
    stopping: AtomicBool,
}

const UNDECIDED: u8 = 0;
const INDEX_FIELDS: u8 = 1;
const EMPTY: u8 = 2;

/// A document of an index build, counted as done when tantivy drops it.
pub(crate) struct TrackedDoc {
    doc: TantivyDocument,
    tracker: Arc<StepTracker>,
    /// Tantivy reads a document's fields three times (fast fields, postings,
    /// doc store). The first read decides for all three, so a cancel between
    /// them cannot leave a document half indexed.
    state: AtomicU8,
}

impl TrackedDoc {
    fn is_empty_for_tantivy(&self) -> bool {
        let decided = if self.tracker.stopping.load(Ordering::Relaxed) { EMPTY } else { INDEX_FIELDS };
        match self.state.compare_exchange(UNDECIDED, decided, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => decided == EMPTY,
            Err(earlier) => earlier == EMPTY,
        }
    }
}

impl Document for TrackedDoc {
    type Value<'a> = <TantivyDocument as Document>::Value<'a>;
    type FieldsValuesIter<'a> = std::iter::Take<<TantivyDocument as Document>::FieldsValuesIter<'a>>;

    fn iter_fields_and_values(&self) -> Self::FieldsValuesIter<'_> {
        let n = if self.is_empty_for_tantivy() { 0 } else { usize::MAX };
        self.doc.iter_fields_and_values().take(n)
    }
}

impl Drop for TrackedDoc {
    fn drop(&mut self) {
        self.tracker.done.fetch_add(1, Ordering::Relaxed);
    }
}

/// What a step's row loop uses to hand its documents to tantivy.
pub(crate) struct IndexFeed<'w> {
    writer: &'w IndexWriter<TrackedDoc>,
    tracker: Arc<StepTracker>,
}

impl IndexFeed<'_> {
    /// True once the build was cancelled: the row loop returns.
    pub(crate) fn is_stopping(&self) -> bool {
        self.tracker.stopping.load(Ordering::Relaxed)
    }

    /// Queue a document. It counts as done when tantivy has indexed it.
    pub(crate) fn add(&self, doc: TantivyDocument) -> Result<()> {
        self.writer.add_document(TrackedDoc {
            doc,
            tracker: self.tracker.clone(),
            state: AtomicU8::new(UNDECIDED),
        })?;
        Ok(())
    }

    /// A row the step does not index (empty text, another language). It
    /// counts as done at once.
    pub(crate) fn skip_row(&self) {
        self.tracker.done.fetch_add(1, Ordering::Relaxed);
    }
}

/// Run one step's row loop and its commit on a helper thread, while this
/// thread watches `cancel` and reports `on_docs(done, total)` every
/// [`POLL_INTERVAL`] when the count has moved. `total` is the number of rows
/// the loop will go through, each of which ends as an [`IndexFeed::add`] or an
/// [`IndexFeed::skip_row`]. `done == total` while tantivy writes the step's
/// files.
///
/// `produce` returns the number of documents it added. When it sees
/// [`IndexFeed::is_stopping`] it returns early, and the writer is dropped
/// without a commit.
pub(crate) fn run_tracked_step<F>(
    writer: IndexWriter<TrackedDoc>,
    index: &Index,
    total: u64,
    cancel: &AtomicBool,
    on_docs: &mut dyn FnMut(u64, u64),
    produce: F,
) -> Result<StepResult>
where
    F: FnOnce(&IndexFeed) -> Result<u64> + Send,
{
    let tracker = Arc::new(StepTracker::default());
    on_docs(0, total);

    std::thread::scope(|s| {
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let worker_tracker = tracker.clone();
        let handle = s.spawn(move || -> Result<StepResult> {
            // Tells the watching thread to stop waiting on every exit, error included.
            let _done = DoneOnDrop(done_tx);
            let mut writer = writer;
            let added = {
                let feed = IndexFeed { writer: &writer, tracker: worker_tracker.clone() };
                produce(&feed)?
            };
            if worker_tracker.stopping.load(Ordering::Relaxed) {
                // The queued documents are empty by now, so this is quick.
                drop(writer);
                return Ok(StepResult::Cancelled);
            }

            // 1. commit() persists documents and makes them searchable.
            // 2. wait_merging_threads() blocks until background merges finish. Since it takes 'self',
            //    it consumes the writer and explicitly releases the INDEX_WRITER_LOCK.
            // 3. sync_directory() ensures the OS flushes directory metadata, preventing "file changed"
            //    errors during external archival (tar).
            writer.commit()?;
            writer.wait_merging_threads()?;
            index.directory().sync_directory()?;

            if worker_tracker.stopping.load(Ordering::Relaxed) {
                // Cancelled during the commit: what it committed is partial.
                return Ok(StepResult::Cancelled);
            }
            Ok(StepResult::Done(added))
        });

        let mut reported = 0u64;
        loop {
            let finished = !matches!(done_rx.recv_timeout(POLL_INTERVAL), Err(mpsc::RecvTimeoutError::Timeout));
            if cancel.load(Ordering::Relaxed) {
                tracker.stopping.store(true, Ordering::Relaxed);
            }
            if finished {
                break;
            }
            let done = tracker.done.load(Ordering::Relaxed).min(total);
            if done != reported && !tracker.stopping.load(Ordering::Relaxed) {
                reported = done;
                on_docs(done, total);
            }
        }

        handle.join().unwrap_or_else(|_| Err(anyhow!("the index build step stopped unexpectedly")))
    })
}

struct DoneOnDrop(mpsc::Sender<()>);

impl Drop for DoneOnDrop {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}
