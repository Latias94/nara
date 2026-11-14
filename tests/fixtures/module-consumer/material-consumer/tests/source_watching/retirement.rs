use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use nara::{
    ProductConfiguration,
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
        RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
    },
    asset_watch::AssetWatchRuntimeObserver,
    ecs::Resource,
};

const OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("consumer.palette.source-watching.close");
const PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("consumer.palette.source-watching.close");

#[derive(Clone, Default, Resource)]
pub(super) struct CloseProof {
    observer: Arc<Mutex<Option<AssetWatchRuntimeObserver>>>,
    joined_with_world: Arc<AtomicBool>,
    world_dropped: Arc<AtomicBool>,
}

impl ProductConfiguration for CloseProof {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"consumer.palette.source-watching.close.v1\0");
    }
}

impl CloseProof {
    pub(super) fn observe(&self, observer: AssetWatchRuntimeObserver) {
        assert!(self.observer.lock().unwrap().replace(observer).is_none());
    }

    pub(super) fn assert_joined_before_world_drop(&self) {
        assert!(self.joined_with_world.load(Ordering::SeqCst));
        assert!(self.world_dropped.load(Ordering::SeqCst));
    }
}

#[derive(Resource)]
struct WorldLifetime(Arc<AtomicBool>);

impl Drop for WorldLifetime {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

pub(super) struct ObserveClose(pub(super) CloseProof);

impl Plugin for ObserveClose {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.palette.source-watching.close"),
            PluginCategory::Runtime,
        )
        .shutdown_obligations(&[OBLIGATION]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(self.0.clone())?
            .insert_resource(WorldLifetime(Arc::clone(&self.0.world_dropped)))?
            .register_plugin_runtime_close_participant(
                OBLIGATION,
                PARTICIPANT,
                JoinBeforeWorldDrop(self.0.clone()),
            )?;
        Ok(())
    }
}

struct JoinBeforeWorldDrop(CloseProof);

impl JoinBeforeWorldDrop {
    fn poll(&self, context: &RuntimeCloseContext<'_>) -> RuntimeCloseProgress {
        assert!(context.world().contains_resource::<WorldLifetime>());
        assert!(!self.0.world_dropped.load(Ordering::SeqCst));
        let observer = self.0.observer.lock().unwrap();
        let Some(observer) = observer.as_ref() else {
            return RuntimeCloseProgress::Complete;
        };
        let stats = observer.stats();
        if stats.active() != 0 || stats.retiring() != 0 {
            return RuntimeCloseProgress::Pending;
        }
        assert!(stats.started() > 0);
        assert_eq!(stats.started(), stats.joined());
        self.0.joined_with_world.store(true, Ordering::SeqCst);
        RuntimeCloseProgress::Complete
    }
}

impl RuntimeCloseParticipant for JoinBeforeWorldDrop {
    fn begin_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.poll(context))
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.poll(context))
    }
}
