use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};

use anyhow::Error;
use bytes::Bytes;
use futures::Stream;

use crate::plugin::RelayStreamTaskTracker;
use crate::service::shared::stream::finalize_once::StreamFinalizeController;

pub(crate) type BoxFinalizeFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub(crate) trait RelayStreamFinalizer<P, S>: Clone {
    fn build_finalize_future(&self, progress: &P, settlement: S) -> Option<BoxFinalizeFuture>;
}

pub(crate) trait RelayStreamAdapter {
    type Item;
    type Progress: Clone + Default;
    type Settlement: Clone;

    fn request_id(&self) -> &str;

    fn observe(
        &mut self,
        progress: &mut Self::Progress,
        item: Self::Item,
        pending_frames: &mut VecDeque<Bytes>,
    ) -> anyhow::Result<()>;

    fn settle_on_error(&self, progress: &Self::Progress, error: &Error) -> Self::Settlement;

    fn settle_on_eof(
        &mut self,
        progress: &Self::Progress,
        pending_frames: &mut VecDeque<Bytes>,
    ) -> anyhow::Result<Self::Settlement>;

    fn settle_on_cancel(&self) -> Self::Settlement;
}

pub(crate) struct TrackedRelayStream<S, A, F>
where
    S: Stream<Item = Result<A::Item, Error>> + Unpin,
    A: RelayStreamAdapter,
    F: RelayStreamFinalizer<A::Progress, A::Settlement>,
{
    inner: S,
    adapter: A,
    finalize: StreamFinalizeController<F>,
    progress: A::Progress,
    pending_frames: VecDeque<Bytes>,
    done_emitted: bool,
}

impl<S, A, F> Unpin for TrackedRelayStream<S, A, F>
where
    S: Stream<Item = Result<A::Item, Error>> + Unpin,
    A: RelayStreamAdapter,
    F: RelayStreamFinalizer<A::Progress, A::Settlement>,
{
}

impl<S, A, F> TrackedRelayStream<S, A, F>
where
    S: Stream<Item = Result<A::Item, Error>> + Unpin,
    A: RelayStreamAdapter,
    F: RelayStreamFinalizer<A::Progress, A::Settlement>,
{
    pub(crate) fn new(
        inner: S,
        adapter: A,
        task_tracker: RelayStreamTaskTracker,
        finalizer: F,
    ) -> Self {
        Self {
            inner,
            adapter,
            finalize: StreamFinalizeController::new(task_tracker, finalizer),
            progress: A::Progress::default(),
            pending_frames: VecDeque::new(),
            done_emitted: false,
        }
    }

    fn queue_finalize(&mut self, settlement: A::Settlement) {
        let progress = self.progress.clone();
        self.finalize
            .settle(|finalizer| finalizer.build_finalize_future(&progress, settlement));
    }
}

impl<S, A, F> Stream for TrackedRelayStream<S, A, F>
where
    S: Stream<Item = Result<A::Item, Error>> + Unpin,
    A: RelayStreamAdapter + Unpin,
    F: RelayStreamFinalizer<A::Progress, A::Settlement>,
{
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().get_mut();

        if let Some(frame) = this.pending_frames.pop_front() {
            return Poll::Ready(Some(Ok(frame)));
        }
        if this.done_emitted {
            return Poll::Ready(None);
        }

        loop {
            match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(item))) => {
                    if let Err(error) =
                        this.adapter
                            .observe(&mut this.progress, item, &mut this.pending_frames)
                    {
                        let message = error.to_string();
                        let settlement = this.adapter.settle_on_error(&this.progress, &error);
                        this.queue_finalize(settlement);
                        this.done_emitted = true;
                        return Poll::Ready(Some(Err(io::Error::other(message))));
                    }

                    if let Some(frame) = this.pending_frames.pop_front() {
                        return Poll::Ready(Some(Ok(frame)));
                    }
                }
                Poll::Ready(Some(Err(error))) => {
                    tracing::warn!(
                        request_id = this.adapter.request_id(),
                        error = %error,
                        "relay stream chunk read failed"
                    );
                    let message = error.to_string();
                    let settlement = this.adapter.settle_on_error(&this.progress, &error);
                    this.queue_finalize(settlement);
                    this.done_emitted = true;
                    return Poll::Ready(Some(Err(io::Error::other(message))));
                }
                Poll::Ready(None) => {
                    let settlement = match this
                        .adapter
                        .settle_on_eof(&this.progress, &mut this.pending_frames)
                    {
                        Ok(settlement) => settlement,
                        Err(error) => {
                            let message = error.to_string();
                            let settlement = this.adapter.settle_on_error(&this.progress, &error);
                            this.queue_finalize(settlement);
                            this.done_emitted = true;
                            return Poll::Ready(Some(Err(io::Error::other(message))));
                        }
                    };

                    this.queue_finalize(settlement);
                    this.done_emitted = true;
                    if let Some(frame) = this.pending_frames.pop_front() {
                        return Poll::Ready(Some(Ok(frame)));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl<S, A, F> Drop for TrackedRelayStream<S, A, F>
where
    S: Stream<Item = Result<A::Item, Error>> + Unpin,
    A: RelayStreamAdapter,
    F: RelayStreamFinalizer<A::Progress, A::Settlement>,
{
    fn drop(&mut self) {
        if self.finalize.is_settled() {
            return;
        }

        let settlement = self.adapter.settle_on_cancel();
        self.queue_finalize(settlement);
    }
}
