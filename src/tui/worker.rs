use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

use crate::{
    TUI_OUTPUT_LIMIT,
    pipeline::{
        ExecutionPolicy, ExecutionReport, ExecutionRequest, ExecutionTarget, TransformStep,
        execute_report,
    },
};

pub(super) struct PreviewJob {
    pub(super) request_id: u64,
    pub(super) input: Vec<u8>,
    pub(super) steps: Vec<TransformStep>,
    pub(super) target: ExecutionTarget,
}

pub(super) struct PreviewResult {
    pub(super) report: ExecutionReport,
}

struct WorkerState {
    pending: Option<PreviewJob>,
    shutdown: bool,
}

struct WorkerShared {
    state: Mutex<WorkerState>,
    pending_changed: Condvar,
    latest_request_id: AtomicU64,
}

pub(super) struct PreviewWorker {
    shared: Arc<WorkerShared>,
    results: mpsc::Receiver<PreviewResult>,
}

impl Default for PreviewWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewWorker {
    pub(super) fn new() -> Self {
        let shared = Arc::new(WorkerShared {
            state: Mutex::new(WorkerState {
                pending: None,
                shutdown: false,
            }),
            pending_changed: Condvar::new(),
            latest_request_id: AtomicU64::new(0),
        });
        let worker_shared = Arc::clone(&shared);
        let (sender, results) = mpsc::channel();
        thread::spawn(move || {
            loop {
                let job = {
                    let mut state = worker_shared
                        .state
                        .lock()
                        .expect("preview worker lock poisoned");
                    while state.pending.is_none() && !state.shutdown {
                        state = worker_shared
                            .pending_changed
                            .wait(state)
                            .expect("preview worker lock poisoned");
                    }
                    if state.shutdown {
                        return;
                    }
                    state.pending.take().expect("pending job checked")
                };
                let request_id = job.request_id;
                let report = execute_report(
                    ExecutionRequest {
                        request_id,
                        input: job.input,
                        steps: &job.steps,
                        output_limit: TUI_OUTPUT_LIMIT,
                        policy: ExecutionPolicy::AllowBinary,
                        target: job.target,
                    },
                    || worker_shared.latest_request_id.load(Ordering::Acquire) != request_id,
                );
                if sender.send(PreviewResult { report }).is_err() {
                    return;
                }
            }
        });
        Self { shared, results }
    }

    pub(super) fn submit(&self, job: PreviewJob) {
        self.shared
            .latest_request_id
            .store(job.request_id, Ordering::Release);
        let mut state = self
            .shared
            .state
            .lock()
            .expect("preview worker lock poisoned");
        state.pending = Some(job);
        self.shared.pending_changed.notify_one();
    }

    pub(super) fn cancel(&self, request_id: u64) {
        self.shared
            .latest_request_id
            .store(request_id, Ordering::Release);
        if let Ok(mut state) = self.shared.state.lock() {
            state.pending = None;
        }
    }

    pub(super) fn try_recv(&self) -> Option<PreviewResult> {
        self.results.try_recv().ok()
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        self.shared.latest_request_id.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut state) = self.shared.state.lock() {
            state.shutdown = true;
            state.pending = None;
            self.shared.pending_changed.notify_one();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        error::TransformError,
        pipeline::ExecutionOutcome,
        transforms::{TransformDefinition, transform_by_id},
    };
    use std::{
        sync::{
            Barrier, OnceLock,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    struct BlockingTransformControl {
        started: mpsc::Sender<Vec<u8>>,
        release: Arc<Barrier>,
    }

    static LATEST_ONLY_CONTROL: OnceLock<BlockingTransformControl> = OnceLock::new();
    static DROP_CONTROL: OnceLock<BlockingTransformControl> = OnceLock::new();
    static BETWEEN_STEPS_CONTROL: OnceLock<BlockingTransformControl> = OnceLock::new();
    static CANCEL_PENDING_CONTROL: OnceLock<BlockingTransformControl> = OnceLock::new();
    static SECOND_STEP_CALLS: AtomicUsize = AtomicUsize::new(0);

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

    fn block_between_steps(input: &[u8], _: usize) -> Result<Vec<u8>, TransformError> {
        block(&BETWEEN_STEPS_CONTROL, input)
    }

    fn block_before_pending_cancel(input: &[u8], _: usize) -> Result<Vec<u8>, TransformError> {
        block(&CANCEL_PENDING_CONTROL, input)
    }

    fn count_second_step(input: &[u8], _: usize) -> Result<Vec<u8>, TransformError> {
        SECOND_STEP_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(input.to_vec())
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

    static BETWEEN_STEPS_TRANSFORM: TransformDefinition = TransformDefinition {
        id: "test-between-steps",
        display_name: "Test between steps",
        description: "Test-only blocking transform",
        behavior: "test-only blocking transform",
        accepts_binary: true,
        apply: block_between_steps,
    };

    static SECOND_STEP_TRANSFORM: TransformDefinition = TransformDefinition {
        id: "test-second-step",
        display_name: "Test second step",
        description: "Test-only counting transform",
        behavior: "test-only counting transform",
        accepts_binary: true,
        apply: count_second_step,
    };

    static CANCEL_PENDING_TRANSFORM: TransformDefinition = TransformDefinition {
        id: "test-cancel-pending",
        display_name: "Test cancel pending",
        description: "Test-only blocking transform",
        behavior: "test-only blocking transform",
        accepts_binary: true,
        apply: block_before_pending_cancel,
    };

    fn blocking_job(
        request_id: u64,
        input: &[u8],
        definition: &'static TransformDefinition,
    ) -> PreviewJob {
        PreviewJob {
            request_id,
            input: input.to_vec(),
            steps: vec![TransformStep {
                definition,
                enabled: true,
            }],
            target: ExecutionTarget::Final,
        }
    }

    #[test]
    fn worker_returns_a_bounded_pipeline_result() {
        let worker = PreviewWorker::new();
        worker.submit(PreviewJob {
            request_id: 7,
            input: b"foo".to_vec(),
            steps: vec![TransformStep {
                definition: transform_by_id("base64-encode").unwrap(),
                enabled: true,
            }],
            target: ExecutionTarget::Final,
        });
        let result = worker.results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(result.report.request_id, 7);
        assert_eq!(
            result.report.outcome,
            crate::pipeline::ExecutionOutcome::Success(b"Zm9v".to_vec())
        );
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
        assert_eq!(first.report.request_id, 1);
        assert_eq!(
            first.report.outcome,
            crate::pipeline::ExecutionOutcome::Cancelled
        );
        assert_eq!(
            started.recv_timeout(Duration::from_secs(1)).unwrap(),
            b"last"
        );
        release.wait();

        let last = worker.results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(last.report.request_id, 3);
        assert_eq!(
            last.report.outcome,
            crate::pipeline::ExecutionOutcome::Success(b"last".to_vec())
        );
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
        worker.submit(PreviewJob {
            request_id: 2,
            input: b"pending".to_vec(),
            steps: Vec::new(),
            target: ExecutionTarget::Final,
        });
        let shared = Arc::clone(&worker.shared);
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
        assert_ne!(shared.latest_request_id.load(Ordering::Acquire), 2);
        let state = shared.state.lock().unwrap();
        assert!(state.shutdown);
        assert!(state.pending.is_none());
        drop(state);

        let result = results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(result.report.request_id, 1);
        assert_eq!(
            result.report.outcome,
            crate::pipeline::ExecutionOutcome::Cancelled
        );
        assert!(matches!(
            results.recv_timeout(Duration::from_secs(1)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn newer_request_seen_between_steps_prevents_the_second_transform() {
        let (started_sender, started) = mpsc::channel();
        let release = Arc::new(Barrier::new(2));
        assert!(
            BETWEEN_STEPS_CONTROL
                .set(BlockingTransformControl {
                    started: started_sender,
                    release: Arc::clone(&release),
                })
                .is_ok()
        );
        SECOND_STEP_CALLS.store(0, Ordering::SeqCst);
        let worker = PreviewWorker::new();
        worker.submit(PreviewJob {
            request_id: 1,
            input: b"secret-input".to_vec(),
            steps: vec![
                TransformStep {
                    definition: &BETWEEN_STEPS_TRANSFORM,
                    enabled: true,
                },
                TransformStep {
                    definition: &SECOND_STEP_TRANSFORM,
                    enabled: true,
                },
            ],
            target: ExecutionTarget::Final,
        });
        started.recv_timeout(Duration::from_secs(1)).unwrap();

        worker.cancel(2);
        release.wait();

        let result = worker.results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(result.report.request_id, 1);
        assert_eq!(result.report.outcome, ExecutionOutcome::Cancelled);
        assert_eq!(SECOND_STEP_CALLS.load(Ordering::SeqCst), 0);
        let diagnostic = format!("{:?}", result.report);
        assert!(!diagnostic.contains("secret-input"));
    }

    #[test]
    fn cancellation_clears_a_pending_job_before_it_can_execute() {
        let (started_sender, started) = mpsc::channel();
        let release = Arc::new(Barrier::new(2));
        assert!(
            CANCEL_PENDING_CONTROL
                .set(BlockingTransformControl {
                    started: started_sender,
                    release: Arc::clone(&release),
                })
                .is_ok()
        );
        let worker = PreviewWorker::new();
        worker.submit(blocking_job(6, b"running", &CANCEL_PENDING_TRANSFORM));
        started.recv_timeout(Duration::from_secs(1)).unwrap();
        worker.submit(PreviewJob {
            request_id: 7,
            input: b"old".to_vec(),
            steps: Vec::new(),
            target: ExecutionTarget::Final,
        });
        worker.cancel(8);
        release.wait();

        let running = worker.results.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(running.report.request_id, 6);
        assert_eq!(running.report.outcome, ExecutionOutcome::Cancelled);
        assert!(matches!(
            worker.results.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
    }
}
