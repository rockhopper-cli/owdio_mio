// src/ui/async_job.rs
use crate::pipelines::PipelineMessage;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

pub struct AsyncJob<T> {
    pub is_processing: bool,
    pub progress: f32,
    pub progress_text: String,
    pub cancel_token: Arc<AtomicBool>,
    pub receiver: Option<Receiver<PipelineMessage<T>>>,
}

impl<T> Default for AsyncJob<T> {
    fn default() -> Self {
        Self {
            is_processing: false,
            progress: 0.0,
            progress_text: "Idle".to_string(),
            cancel_token: Arc::new(AtomicBool::new(false)),
            receiver: None,
        }
    }
}

impl<T: Send + 'static> AsyncJob<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawns a background task with an auto-wired cancellation token and message sender.
    pub fn spawn<F>(&mut self, initial_text: &str, task: F)
    where
        F: FnOnce(Arc<AtomicBool>, Sender<PipelineMessage<T>>) + Send + 'static,
    {
        let (tx, rx) = channel();
        self.is_processing = true;
        self.progress = 0.05;
        self.progress_text = initial_text.to_string();
        self.cancel_token = Arc::new(AtomicBool::new(false));
        self.receiver = Some(rx);

        let cancel = self.cancel_token.clone();
        thread::spawn(move || {
            task(cancel, tx);
        });
    }

    pub fn cancel(&mut self) {
        if self.is_processing {
            self.cancel_token.store(true, Ordering::Relaxed);
            self.progress_text = "Cancelling...".to_string();
        }
    }

    pub fn poll(&mut self, log_tx: &Sender<String>, tag: &str, mut on_success: impl FnMut(T)) {
        if let Some(ref rx) = self.receiver {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    PipelineMessage::Log(line) => {
                        let _ = log_tx.send(line);
                    }
                    PipelineMessage::Progress { progress, text } => {
                        self.progress = progress;
                        self.progress_text = text;
                    }
                    PipelineMessage::Finished(res) => {
                        self.is_processing = false;
                        match res {
                            Ok(data) => {
                                self.progress = 1.0;
                                self.progress_text = "Completed".to_string();
                                let _ =
                                    log_tx.send(format!("[{}] Task completed successfully.", tag));
                                on_success(data);
                            }
                            Err(err) => {
                                self.progress = 0.0;
                                self.progress_text = "Failed".to_string();
                                let _ = log_tx.send(format!("[{}] Error: {}", tag, err));
                            }
                        }
                    }
                }
            }
        }
    }
}
