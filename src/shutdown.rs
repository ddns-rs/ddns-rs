use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{broadcast, Notify};

#[derive(Debug)]
pub struct Shutdown {
    shutdown: AtomicBool,
    sender: broadcast::Sender<()>,
    notify: Notify,
}

impl Shutdown {
    pub fn new() -> Shutdown {
        let (sender, _) = broadcast::channel(1);
        let notify = Notify::new();
        Shutdown {
            shutdown: AtomicBool::new(false),
            sender,
            notify,
        }
    }

    pub async fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if self.sender.send(()).is_err() {
            return;
        }

        loop {
            if 0 == self.sender.receiver_count() {
                return;
            }
            self.notify.notified().await;
        }
    }

    pub async fn receive(&self) {
        if self.shutdown.load(Ordering::Relaxed) {
            return;
        }
        let mut receiver = self.sender.subscribe();
        let _ = receiver.recv().await;
        self.notify.notify_waiters();
    }
}
