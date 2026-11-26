//! The D-Bus objects ibus-daemon talks to: the factory
//! (`org.freedesktop.IBus.Factory`, ibusfactory.c) and one engine object
//! per `CreateEngine` (`org.freedesktop.IBus.Engine`, ibusengine.c, plus
//! `org.freedesktop.IBus.Service` for `Destroy`, ibusservice.c).

use karukan_im::core::keycode::Keysym;
use zbus::object_server::SignalEmitter;
use zbus::{ObjectServer, OwnedObjectPath, Variant, fdo, interface};

use crate::ibus::{LookupTable, Text, V, text_of};
use crate::session::{Session, Signal};
use crate::worker::Worker;

pub const FACTORY_PATH: &str = "/org/freedesktop/IBus/Factory";

/// `IBUS_ENGINE_PREEDIT_COMMIT`: on focus out IBus commits the preedit
/// (see `Session::deactivate`).
const PREEDIT_COMMIT: u32 = 1;

fn engine_path(id: u32) -> String {
    format!("/org/freedesktop/IBus/Engine/{id}")
}

pub struct Factory {
    worker: Worker,
    last_id: u32,
}

impl Factory {
    pub fn new(worker: Worker) -> Self {
        Self { worker, last_id: 0 }
    }
}

#[interface(name = "org.freedesktop.IBus.Factory")]
impl Factory {
    async fn create_engine(
        &mut self,
        _name: &str,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<OwnedObjectPath> {
        self.last_id += 1;
        let id = self.last_id;
        self.worker
            .run(move |sessions| {
                sessions.insert(id, Session::new());
            })
            .await;
        let path = engine_path(id);
        let worker = self.worker.clone();
        server.at(path.as_str(), Engine { id, worker }).await?;
        let worker = self.worker.clone();
        server.at(path.as_str(), Service { id, worker }).await?;
        tracing::info!("created engine {path}");
        Ok(OwnedObjectPath::try_from(path)?)
    }
}

pub struct Engine {
    id: u32,
    worker: Worker,
}

impl Engine {
    /// Run `f` on this engine's session. A call racing `Destroy` finds
    /// none and gets the default.
    async fn with_session<R: Default + Send + 'static>(
        &self,
        f: impl FnOnce(&mut Session) -> R + Send + 'static,
    ) -> R {
        let id = self.id;
        self.worker
            .run(move |sessions| sessions.get_mut(&id).map(f).unwrap_or_default())
            .await
    }

    async fn show(
        &self,
        emitter: &SignalEmitter<'_>,
        f: impl FnOnce(&mut Session) -> Vec<Signal> + Send + 'static,
    ) {
        let signals = self.with_session(f).await;
        emit(emitter, signals).await;
    }
}

async fn emit(emitter: &SignalEmitter<'_>, signals: Vec<Signal>) {
    for signal in signals {
        let sent = match signal {
            Signal::Commit(text) => Engine::commit_text(emitter, V(text)).await,
            Signal::Preedit {
                text,
                cursor,
                visible,
            } => {
                Engine::update_preedit_text(emitter, V(text), cursor, visible, PREEDIT_COMMIT).await
            }
            Signal::Aux(text) => Engine::update_auxiliary_text(emitter, V(text), true).await,
            Signal::HideAux => Engine::hide_auxiliary_text(emitter).await,
            Signal::Lookup(table) => Engine::update_lookup_table(emitter, V(table), true).await,
            Signal::HideLookup => Engine::hide_lookup_table(emitter).await,
        };
        if let Err(e) = sent {
            tracing::warn!("signal not sent: {e}");
        }
    }
}

// Ordered (`spawn = false`): keys must reach the engine as typed.
#[interface(name = "org.freedesktop.IBus.Engine", spawn = false)]
impl Engine {
    async fn process_key_event(
        &self,
        keyval: u32,
        _keycode: u32,
        state: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> bool {
        let (consumed, signals) = self
            .with_session(move |s| s.process_key(keyval, state))
            .await;
        emit(&emitter, signals).await;
        consumed
    }

    async fn candidate_clicked(
        &self,
        index: u32,
        _button: u32,
        _state: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        self.show(&emitter, move |s| s.select_candidate(index))
            .await
    }

    async fn page_up(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.show(&emitter, |s| s.press(Keysym::PAGE_UP)).await
    }

    async fn page_down(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.show(&emitter, |s| s.press(Keysym::PAGE_DOWN)).await
    }

    async fn cursor_up(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.show(&emitter, |s| s.press(Keysym::UP)).await
    }

    async fn cursor_down(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.show(&emitter, |s| s.press(Keysym::DOWN)).await
    }

    async fn reset(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.show(&emitter, Session::reset).await
    }

    async fn focus_out(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.show(&emitter, Session::deactivate).await
    }

    async fn disable(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.show(&emitter, Session::deactivate).await
    }

    async fn set_capabilities(&self, caps: u32) {
        self.with_session(move |s| s.capabilities = caps).await
    }

    async fn set_surrounding_text(&self, text: Variant<'_>, cursor_pos: u32, _anchor_pos: u32) {
        if let Some(text) = text_of(&text) {
            self.with_session(move |s| s.set_surrounding_text(text, cursor_pos))
                .await
        }
    }

    /// Read by ibus-daemon: an engine that uses the editor's text asks for
    /// it on every focus in and enable.
    #[zbus(property)]
    fn active_surrounding_text(&self) -> bool {
        true
    }

    #[zbus(signal)]
    async fn commit_text(emitter: &SignalEmitter<'_>, text: V<Text>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn update_preedit_text(
        emitter: &SignalEmitter<'_>,
        text: V<Text>,
        cursor_pos: u32,
        visible: bool,
        mode: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn update_auxiliary_text(
        emitter: &SignalEmitter<'_>,
        text: V<Text>,
        visible: bool,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn hide_auxiliary_text(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn update_lookup_table(
        emitter: &SignalEmitter<'_>,
        table: V<LookupTable>,
        visible: bool,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn hide_lookup_table(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

/// `Destroy`, sent when ibus-daemon drops the engine: save its learning
/// cache, as `karukan_engine_free` does, and unexport it.
pub struct Service {
    id: u32,
    worker: Worker,
}

#[interface(name = "org.freedesktop.IBus.Service")]
impl Service {
    async fn destroy(&self, #[zbus(object_server)] server: &ObjectServer) -> fdo::Result<()> {
        let id = self.id;
        self.worker
            .run(move |sessions| {
                if let Some(mut session) = sessions.remove(&id) {
                    session.engine.save_learning();
                }
            })
            .await;
        let path = engine_path(id);
        server.remove::<Engine, _>(path.as_str()).await?;
        server.remove::<Service, _>(path.as_str()).await?;
        tracing::info!("destroyed engine {path}");
        Ok(())
    }
}
