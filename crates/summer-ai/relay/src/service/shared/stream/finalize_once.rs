use std::future::Future;

use crate::plugin::RelayStreamTaskTracker;

pub(crate) struct StreamFinalizeController<C> {
    task_tracker: RelayStreamTaskTracker,
    context: C,
    settled: bool,
}

impl<C> StreamFinalizeController<C> {
    pub(crate) fn new(task_tracker: RelayStreamTaskTracker, context: C) -> Self {
        Self {
            task_tracker,
            context,
            settled: false,
        }
    }

    pub(crate) fn is_settled(&self) -> bool {
        self.settled
    }

    pub(crate) fn settle<F, Fut>(&mut self, build: F)
    where
        F: FnOnce(&C) -> Option<Fut>,
        Fut: Future<Output = ()> + Send + 'static,
    {
        if self.settled {
            return;
        }

        self.settled = true;
        if let Some(future) = build(&self.context) {
            self.task_tracker.spawn(future);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::oneshot;

    use super::StreamFinalizeController;
    use crate::plugin::RelayStreamTaskTracker;

    #[tokio::test]
    async fn settle_spawns_finalize_only_once() {
        let tracker = RelayStreamTaskTracker::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = oneshot::channel();

        let mut controller = StreamFinalizeController::new(tracker.clone(), 7usize);
        let counter_for_first = counter.clone();
        controller.settle(|value| {
            let counter = counter_for_first.clone();
            let observed = *value;
            Some(async move {
                counter.fetch_add(observed, Ordering::SeqCst);
                let _ = tx.send(());
            })
        });
        controller.settle(|_| -> Option<std::future::Ready<()>> {
            panic!("second finalize should not be scheduled")
        });

        rx.await.expect("first finalize should run");
        tracker.close();
        tracker.wait().await;

        assert_eq!(counter.load(Ordering::SeqCst), 7);
        assert!(controller.is_settled());
    }
}
