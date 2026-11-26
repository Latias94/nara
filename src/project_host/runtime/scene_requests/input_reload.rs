use nara_app::diagnostics::emit_report_to_tracing;
use nara_app::tasks::{TaskDomainKey, TaskHandle, TaskPoolKind, TaskSpawnRequest, TaskTerminal};
use nara_asset::AssetSourceChange;
use nara_ecs::bevy_ecs::system::SystemState;
use nara_input::{ActionMaps, InputSeatId};

use crate::project_content::{ProjectContentError, ProjectInputContent};

use super::{Arc, Context, RequestOwner, Res, ResMut, TaskPools, World, rejected};

#[derive(Default)]
pub(super) struct InputReload {
    pending: bool,
    reading: Option<InputRead>,
}

struct InputRead {
    context: Arc<Context>,
    result: TaskHandle<Result<Option<ProjectInputContent>, ProjectContentError>>,
}

impl InputReload {
    pub(super) fn cancel(&mut self) {
        self.pending = false;
        if let Some(read) = self.reading.take() {
            read.result.cancel();
        }
    }

    fn pending(&self) -> bool {
        self.pending || self.reading.is_some()
    }
}

/// An absent change means native coverage was re-established and needs a fresh read.
pub(in crate::project_host::runtime) fn queue(
    world: &mut World,
    change: Option<&AssetSourceChange>,
) -> bool {
    let Some(mut owner) = world.get_resource_mut::<RequestOwner>() else {
        return false;
    };
    let Context::Source {
        input: Some(input), ..
    } = owner.requests.source().as_ref()
    else {
        return false;
    };
    if owner.requests.is_closing() || !change.is_none_or(|change| change.path() == input.path()) {
        return false;
    }
    owner.input_reload.pending = true;
    true
}

/// Installs a compiled map before input resolution without replacing the current Scene.
/// An in-flight Scene request holds its original input owner until it finishes.
pub(in crate::project_host::runtime) fn poll(world: &mut World) -> bool {
    if !world
        .get_resource::<RequestOwner>()
        .is_some_and(|owner| owner.input_reload.pending())
    {
        return false;
    }
    let mut state = SystemState::<(
        ResMut<RequestOwner>,
        Option<Res<TaskPools>>,
        Option<ResMut<ActionMaps>>,
    )>::new(world);
    let Ok((mut owner, pools, maps)) = state.get_mut(world) else {
        return false;
    };
    if owner.requests.is_closing() {
        owner.input_reload.cancel();
        return false;
    }
    if owner.requests.active_request().is_some() {
        return true;
    }
    if let Some(read) = &mut owner.input_reload.reading {
        let Some(terminal) = read.result.try_take() else {
            return true;
        };
        let read = owner
            .input_reload
            .reading
            .take()
            .expect("the read was polled in place");
        if !owner.input_reload.pending {
            if Arc::ptr_eq(&read.context, owner.requests.source()) {
                match terminal {
                    TaskTerminal::Completed(Ok(Some(input))) => {
                        replace_input(&mut owner, maps, input)
                    }
                    TaskTerminal::Completed(Ok(None)) | TaskTerminal::Cancelled(_) => {}
                    TaskTerminal::Completed(Err(error)) => {
                        emit_report_to_tracing(error.diagnostics())
                    }
                    TaskTerminal::Failed(_) => report_unavailable(),
                }
            }
            return false;
        }
    }
    owner.input_reload.pending = false;
    let Some(pools) = pools.filter(|pools| pools.is_threaded(TaskPoolKind::Io)) else {
        report_unavailable();
        return false;
    };
    let context = Arc::clone(owner.requests.source());
    let selection = Arc::clone(&context);
    let result = pools.spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(0, TaskDomainKey::new(0x69_6e_70_75_74)),
        move |cancellation| {
            if !cancellation.is_cancelled()
                && let Context::Source {
                    candidate,
                    plan,
                    loader,
                    input: Some(previous),
                } = selection.as_ref()
            {
                loader.reload_selected_input(candidate, plan, previous)
            } else {
                Ok(None)
            }
        },
    );
    match result.into_handle() {
        Ok(result) => owner.input_reload.reading = Some(InputRead { context, result }),
        Err(_) => report_unavailable(),
    }
    owner.input_reload.pending()
}

fn replace_input(
    owner: &mut RequestOwner,
    maps: Option<ResMut<ActionMaps>>,
    replacement: ProjectInputContent,
) {
    let Context::Source {
        candidate,
        plan,
        loader,
        input: Some(previous),
    } = owner.requests.source().as_ref()
    else {
        return;
    };
    let mut maps = maps;
    let map = maps
        .as_deref_mut()
        .and_then(|maps| maps.get_mut(InputSeatId::PRIMARY));
    if let Some(map) = map.filter(|map| {
        map.bindings_handle()
            .shares_owner_with(&previous.bindings())
    }) {
        map.replace_bindings(replacement.bindings());
        let source = Context::Source {
            candidate: candidate.clone(),
            plan: plan.clone(),
            loader: Arc::clone(loader),
            input: Some(replacement),
        };
        owner.requests.replace_source(source);
    } else {
        emit_report_to_tracing(
            &rejected(
                "project.input.reload-owner-changed",
                "Action-map reload requires the installed project mapping",
            )
            .diagnostics,
        );
    }
}

fn report_unavailable() {
    emit_report_to_tracing(
        &rejected(
            "project.input.reload-task-unavailable",
            "Action-map reload could not run on an I/O worker",
        )
        .diagnostics,
    );
}
