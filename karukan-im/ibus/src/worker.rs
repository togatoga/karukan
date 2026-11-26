//! The thread that owns every [`Session`].
//!
//! `InputMethodEngine` is not `Send`, while D-Bus objects must be, so the
//! objects hold only an id and hand each call to this thread.

use std::collections::HashMap;
use std::sync::mpsc;

use futures_channel::oneshot;

use crate::session::Session;

pub type Sessions = HashMap<u32, Session>;
type Job = Box<dyn FnOnce(&mut Sessions) + Send>;

#[derive(Clone)]
pub struct Worker(mpsc::Sender<Job>);

impl Worker {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("karukan-engines".into())
            .spawn(move || {
                let mut sessions = Sessions::new();
                for job in rx {
                    job(&mut sessions);
                }
            })
            .expect("spawn engine thread");
        Self(tx)
    }

    /// Run `f` on the engine thread and wait for its answer.
    pub async fn run<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Sessions) -> R + Send + 'static,
    ) -> R {
        let (tx, rx) = oneshot::channel();
        self.0
            .send(Box::new(move |sessions| {
                let _ = tx.send(f(sessions));
            }))
            .expect("engine thread is running");
        rx.await.expect("engine thread answers")
    }

    pub async fn save_all(&self) {
        self.run(|sessions| sessions.values_mut().for_each(|s| s.engine.save_learning()))
            .await
    }
}
