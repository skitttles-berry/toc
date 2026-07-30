use std::{
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
};

use crate::{
    TUI_OUTPUT_LIMIT,
    error::PipelineError,
    pipeline::{TransformStep, execute},
};

pub(super) struct PreviewJob {
    pub(super) generation: u64,
    pub(super) input: Vec<u8>,
    pub(super) steps: Vec<TransformStep>,
}

pub(super) struct PreviewResult {
    pub(super) generation: u64,
    pub(super) result: Result<Vec<u8>, PipelineError>,
}

struct WorkerState {
    pending: Option<PreviewJob>,
    shutdown: bool,
}

pub(super) struct PreviewWorker {
    shared: Arc<(Mutex<WorkerState>, Condvar)>,
    results: mpsc::Receiver<PreviewResult>,
}

impl Default for PreviewWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewWorker {
    pub(super) fn new() -> Self {
        let shared = Arc::new((
            Mutex::new(WorkerState {
                pending: None,
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let worker_shared = Arc::clone(&shared);
        let (sender, results) = mpsc::channel();
        thread::spawn(move || {
            loop {
                let job = {
                    let (lock, condition) = &*worker_shared;
                    let mut state = lock.lock().expect("preview worker lock poisoned");
                    while state.pending.is_none() && !state.shutdown {
                        state = condition.wait(state).expect("preview worker lock poisoned");
                    }
                    if state.shutdown {
                        return;
                    }
                    state.pending.take().expect("pending job checked")
                };
                let result = execute(job.input, &job.steps, TUI_OUTPUT_LIMIT);
                if sender
                    .send(PreviewResult {
                        generation: job.generation,
                        result,
                    })
                    .is_err()
                {
                    return;
                }
            }
        });
        Self { shared, results }
    }

    pub(super) fn submit(&self, job: PreviewJob) {
        let (lock, condition) = &*self.shared;
        let mut state = lock.lock().expect("preview worker lock poisoned");
        state.pending = Some(job);
        condition.notify_one();
    }

    pub(super) fn try_recv(&self) -> Option<PreviewResult> {
        self.results.try_recv().ok()
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        let (lock, condition) = &*self.shared;
        if let Ok(mut state) = lock.lock() {
            state.shutdown = true;
            state.pending = None;
            condition.notify_one();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        error::TransformError,
        transforms::{TransformDefinition, transform_by_id},
    };
    use std::{
        sync::{Barrier, OnceLock},
        time::Duration,
    };

    struct BlockingTransformControl {
        started: mpsc::Sender<Vec<u8>>,
        release: Arc<Barrier>,
    }

    static LATEST_ONLY_CONTROL: OnceLock<BlockingTransformControl> = OnceLock::new();
    static DROP_CONTROL: OnceLock<BlockingTransformControl> = OnceLock::new();

    fn block(
        control: &OnceLock<BlockingTransformControl>,
        input: &[u8],
    ) -> Result<Vec<u8>, TransformError> {
        let control = control.get().expect("blocking transform configured");
        control
            .started
            .send(input.to_vec())
            .expect("blocking transform observer available");
        control.release.wait();
        Ok(input.to_vec())
    }

    fn block_latest_only(input: &[u8], _: usize) -> Result<Vec<u8>, TransformError> {
        block(&LATEST_ONLY_CONTROL, input)
    }

    fn block_during_drop(input: &[u8], _: usize) -> Result<Vec<u8>, TransformError> {
        block(&DROP_CONTROL, input)
    }

    static LATEST_ONLY_TRANSFORM: TransformDefinition = TransformDefinition {
        id: "test-latest-only",
        display_name: "Test latest only",
        description: "Test-only blocking transform",
        behavior: "test-only blocking transform",
        accepts_binary: true,
        apply: block_latest_only,
    };

    static DROP_TRANSFORM: TransformDefinition = TransformDefinition {
        id: "test-drop",
        display_name: "Test drop",
        description: "Test-only blocking transform",
        behavior: "test-only blocking transform",
        accepts_binary: true,
        apply: block_during_drop,
    };

    fn blocking_job(
        generation: u64,
        input: &[u8],
        definition: &'static TransformDefinition,
    ) -> PreviewJob {
        PreviewJob {
            generation,
            input: input.to_vec(),
            steps: vec![TransformStep {
                definition,
                enabled: true,
            }],
        }
    }

    #[test]
    fn worker_returns_a_bounded_pipeline_result() {
        let worker = PreviewWorker::new();
        worker.submit(PreviewJob {
            generation: 7,
            input: b"foo".to_vec(),
            steps: vec![TransformStep {
                definition: transform_by_id("base64-encode").unwrap(),
                enabled: true,
            }],
        });
        let result = worker.results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(result.generation, 7);
        assert_eq!(result.result.unwrap(), b"Zm9v");
    }
    #[test]
    fn worker_runs_current_job_and_only_the_latest_pending_job() {
        let (started_sender, started) = mpsc::channel();
        let release = Arc::new(Barrier::new(2));
        assert!(
            LATEST_ONLY_CONTROL
                .set(BlockingTransformControl {
                    started: started_sender,
                    release: Arc::clone(&release),
                })
                .is_ok()
        );
        let worker = PreviewWorker::new();

        worker.submit(blocking_job(1, b"first", &LATEST_ONLY_TRANSFORM));
        assert_eq!(
            started.recv_timeout(Duration::from_secs(1)).unwrap(),
            b"first"
        );
        worker.submit(blocking_job(2, b"middle", &LATEST_ONLY_TRANSFORM));
        worker.submit(blocking_job(3, b"last", &LATEST_ONLY_TRANSFORM));
        release.wait();

        let first = worker.results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(first.generation, 1);
        assert_eq!(first.result.unwrap(), b"first");
        assert_eq!(
            started.recv_timeout(Duration::from_secs(1)).unwrap(),
            b"last"
        );
        release.wait();

        let last = worker.results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(last.generation, 3);
        assert_eq!(last.result.unwrap(), b"last");
        assert!(matches!(
            worker.results.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        assert!(matches!(started.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }
    #[test]
    fn dropping_running_worker_does_not_wait_and_worker_exits_after_release() {
        let (started_sender, started) = mpsc::channel();
        let release = Arc::new(Barrier::new(2));
        assert!(
            DROP_CONTROL
                .set(BlockingTransformControl {
                    started: started_sender,
                    release: Arc::clone(&release),
                })
                .is_ok()
        );
        let mut worker = PreviewWorker::new();
        let (dummy_sender, dummy_results) = mpsc::channel();
        let results = std::mem::replace(&mut worker.results, dummy_results);
        drop(dummy_sender);

        worker.submit(blocking_job(1, b"running", &DROP_TRANSFORM));
        assert_eq!(
            started.recv_timeout(Duration::from_secs(1)).unwrap(),
            b"running"
        );
        let (drop_sender, drop_returned) = mpsc::channel();
        thread::spawn(move || {
            drop(worker);
            drop_sender.send(()).expect("drop observer available");
        });

        let returned_before_release = drop_returned.recv_timeout(Duration::from_secs(1)).is_ok();
        release.wait();
        if !returned_before_release {
            drop_returned
                .recv_timeout(Duration::from_secs(1))
                .expect("drop returns after transform release");
        }
        assert!(returned_before_release);

        let result = results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(result.generation, 1);
        assert_eq!(result.result.unwrap(), b"running");
        assert!(matches!(
            results.recv_timeout(Duration::from_secs(1)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }
}
