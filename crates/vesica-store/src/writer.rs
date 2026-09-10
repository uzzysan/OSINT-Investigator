//! The single writer task. See 03-architecture.md section 3.7:
//!
//! > SQLite allows one writer. Writes go through a single writer task fed
//! > by a channel; reads use a connection pool. This is simpler than
//! > optimistic retries and makes lock contention impossible rather than
//! > merely unlikely.
//!
//! [`spawn`] hands the sole read-write connection to a dedicated task and
//! returns a [`WriterHandle`]. Every write, from every caller, funnels
//! through that one task's channel and therefore runs strictly one at a
//! time against that one connection — there is structurally no way for two
//! writes to race for SQLite's single write lock, which is why nothing in
//! this crate retries on `SQLITE_BUSY`.

use std::future::Future;
use std::pin::Pin;

use sqlx::SqliteConnection;
use tokio::sync::{mpsc, oneshot};

use crate::error::StoreError;

/// A future borrowing the connection handed to it, boxed so a job of any
/// concrete type can travel through the writer's channel.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A unit of work the writer task runs against its connection. The job
/// itself is responsible for delivering its result (it captures the
/// `oneshot::Sender` that will carry it) — the task only calls it and awaits
/// the future it returns, so the job's actual output type never has to be
/// named here.
type Job = Box<dyn for<'c> FnOnce(&'c mut SqliteConnection) -> BoxFuture<'c, ()> + Send>;

/// A cheap-to-clone handle to the single writer task. Every clone shares the
/// same channel, so every caller's writes are serialised against every other
/// caller's, regardless of how many `WriterHandle`s exist.
#[derive(Clone)]
pub struct WriterHandle {
    sender: mpsc::Sender<Job>,
}

impl WriterHandle {
    /// Runs `f` against the single write connection and returns its result.
    ///
    /// `f` gets exclusive access to the connection for the lifetime of the
    /// call: the writer task does not start the next queued job until `f`'s
    /// future resolves, so nothing else ever touches this connection while
    /// `f` is running. Callers do not choose when their job runs relative to
    /// others' — the channel is strict FIFO.
    pub async fn write<F, T>(&self, f: F) -> Result<T, StoreError>
    where
        F: for<'c> FnOnce(&'c mut SqliteConnection) -> BoxFuture<'c, Result<T, StoreError>>
            + Send
            + 'static,
        T: Send + 'static,
    {
        let (reply_tx, reply_rx) = oneshot::channel::<Result<T, StoreError>>();

        let job: Job = Box::new(move |conn| {
            Box::pin(async move {
                let result = f(conn).await;
                // If the caller dropped `reply_rx` (e.g. it was cancelled),
                // there is nothing to do with a send failure — the job still
                // ran against the connection either way.
                let _ = reply_tx.send(result);
            })
        });

        self.sender
            .send(job)
            .await
            .map_err(|_| StoreError::WriterUnavailable)?;

        reply_rx.await.map_err(|_| StoreError::WriterUnavailable)?
    }
}

/// Spawns the writer task owning `conn` and returns a handle to it. The
/// channel is bounded so a burst of writers applies natural backpressure
/// rather than growing without limit; it does not change the serialisation
/// guarantee, only how many callers can be queued waiting for their turn.
pub fn spawn(mut conn: SqliteConnection) -> WriterHandle {
    let (sender, mut receiver) = mpsc::channel::<Job>(256);

    tokio::spawn(async move {
        while let Some(job) = receiver.recv().await {
            job(&mut conn).await;
        }
        // All `WriterHandle`s (and thus all senders) have been dropped;
        // `conn` is dropped here, closing the write connection.
    });

    WriterHandle { sender }
}
