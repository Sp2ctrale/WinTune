//! Minimal background-job runner for the UI: runs a closure on its own
//! thread, streams progress back, and hands over the result when done.
//! The UI polls [`BackgroundJob::poll`] once per frame; nothing blocks.

use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub label: String,
}

impl Progress {
    /// 0.0 ..= 1.0 (0 when the total is unknown).
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.done as f32 / self.total as f32).clamp(0.0, 1.0)
        }
    }
}

/// Handed to the job closure to report progress. Every report also wakes
/// the UI (via the `notify` callback given to [`BackgroundJob::spawn`]).
pub struct ProgressSink {
    tx: Sender<Progress>,
    notify: Box<dyn Fn() + Send + Sync>,
}

impl ProgressSink {
    pub fn report(&self, done: usize, total: usize, label: impl Into<String>) {
        let progress = Progress {
            done,
            total,
            label: label.into(),
        };
        if self.tx.send(progress).is_ok() {
            (self.notify)();
        }
    }
}

pub enum JobPoll<T> {
    Running,
    Finished(T),
    /// The job panicked; the message is meant for the status bar.
    Panicked(String),
}

pub struct BackgroundJob<T> {
    title: String,
    progress: Progress,
    progress_rx: Receiver<Progress>,
    result_rx: Receiver<Result<T, String>>,
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

impl<T: Send + 'static> BackgroundJob<T> {
    pub fn spawn(
        title: impl Into<String>,
        notify: impl Fn() + Send + Sync + 'static,
        work: impl FnOnce(&ProgressSink) -> T + Send + 'static,
    ) -> Self {
        let title = title.into();
        let (progress_tx, progress_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let sink = ProgressSink {
            tx: progress_tx,
            notify: Box::new(notify),
        };

        std::thread::Builder::new()
            .name(format!("job: {}", title))
            .spawn(move || {
                let outcome =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&sink)))
                        .map_err(|payload| panic_message(payload.as_ref()));
                let _ = result_tx.send(outcome);
                (sink.notify)();
            })
            .expect("cannot spawn background job thread");

        Self {
            title,
            progress: Progress::default(),
            progress_rx,
            result_rx,
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Latest progress received (updated by [`Self::poll`]).
    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    /// Non-blocking. Drains pending progress, then reports the outcome.
    pub fn poll(&mut self) -> JobPoll<T> {
        while let Ok(p) = self.progress_rx.try_recv() {
            self.progress = p;
        }
        match self.result_rx.try_recv() {
            Ok(Ok(value)) => JobPoll::Finished(value),
            Ok(Err(msg)) => JobPoll::Panicked(msg),
            Err(mpsc::TryRecvError::Empty) => JobPoll::Running,
            Err(mpsc::TryRecvError::Disconnected) => {
                JobPoll::Panicked("background job ended without a result".to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn finish<T: Send + 'static>(job: &mut BackgroundJob<T>) -> JobPoll<T> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match job.poll() {
                JobPoll::Running if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                other => return other,
            }
        }
    }

    #[test]
    fn fraction_is_bounded_and_safe_on_zero_total() {
        let p = |done, total| Progress {
            done,
            total,
            label: String::new(),
        };
        assert_eq!(p(0, 0).fraction(), 0.0);
        assert_eq!(p(1, 4).fraction(), 0.25);
        assert_eq!(p(9, 4).fraction(), 1.0);
    }

    #[test]
    fn job_runs_off_thread_and_returns_its_result() {
        let caller = std::thread::current().id();
        let mut job =
            BackgroundJob::spawn("t", || {}, move |_| std::thread::current().id() != caller);

        match finish(&mut job) {
            JobPoll::Finished(off_thread) => assert!(off_thread),
            _ => panic!("job did not finish"),
        }
    }

    #[test]
    fn progress_reports_reach_the_poller_and_wake_the_ui() {
        let wakeups = Arc::new(AtomicUsize::new(0));
        let w = Arc::clone(&wakeups);
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let mut job = BackgroundJob::spawn(
            "t",
            move || {
                w.fetch_add(1, Ordering::SeqCst);
            },
            move |sink| {
                sink.report(1, 3, "first");
                sink.report(2, 3, "second");
                gate_rx.recv().unwrap(); // stay running until the test checks
            },
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        while job.progress().done < 2 && Instant::now() < deadline {
            assert!(matches!(job.poll(), JobPoll::Running));
            std::thread::sleep(Duration::from_millis(5));
        }

        assert_eq!(
            job.progress(),
            &Progress {
                done: 2,
                total: 3,
                label: "second".into()
            }
        );
        assert!(wakeups.load(Ordering::SeqCst) >= 2);
        gate_tx.send(()).unwrap();
        assert!(matches!(finish(&mut job), JobPoll::Finished(())));
    }

    #[test]
    fn a_panicking_job_is_reported_instead_of_hanging() {
        let mut job: BackgroundJob<()> = BackgroundJob::spawn("t", || {}, |_| panic!("boom"));

        match finish(&mut job) {
            JobPoll::Panicked(msg) => assert!(msg.contains("boom"), "{msg}"),
            JobPoll::Finished(()) => panic!("unexpected success"),
            JobPoll::Running => panic!("job never reported"),
        }
    }

    #[test]
    fn finished_job_wakes_the_ui() {
        let wakeups = Arc::new(AtomicUsize::new(0));
        let w = Arc::clone(&wakeups);
        let mut job = BackgroundJob::spawn(
            "t",
            move || {
                w.fetch_add(1, Ordering::SeqCst);
            },
            |_| 7,
        );
        assert!(matches!(finish(&mut job), JobPoll::Finished(7)));
        assert!(wakeups.load(Ordering::SeqCst) >= 1);
    }
}
