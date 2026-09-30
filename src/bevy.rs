//! Optional Bevy scheduling integration for Terrarium's imperative scene API.

use std::time::Duration;

/// Bevy's application and plugin APIs.
pub use bevy_app as app;
use bevy_app::{App as BevyApp, Plugin, PluginsState, PostUpdate, TaskPoolPlugin};
/// Bevy's ECS APIs.
pub use bevy_ecs as ecs;
use bevy_ecs::prelude::*;
/// Bevy's time APIs.
pub use bevy_time as time;
use bevy_time::{Time, TimePlugin, TimeUpdateStrategy};

use crate::{AppCreationError, Workspace, egui};

/// Integrates an imperative [`Workspace`] into a Bevy app.
///
/// Missing task-pool and time plugins are installed automatically. Add customized versions of
/// those plugins first if needed.
#[derive(Default)]
pub struct TerrariumPlugin;

impl Plugin for TerrariumPlugin {
    fn build(&self, app: &mut BevyApp) {
        if !app.is_plugin_added::<TaskPoolPlugin>() {
            app.add_plugins(TaskPoolPlugin::default());
        }
        if !app.is_plugin_added::<TimePlugin>() {
            app.add_plugins(TimePlugin);
        }
        app.world_mut().init_non_send::<Workspace>();
        app.world_mut().init_non_send::<EguiContext>();
        app.add_systems(PostUpdate, advance_workspace.in_set(TerrariumSet::Advance));
    }
}

/// Ordering boundaries for Terrarium systems in [`PostUpdate`].
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerrariumSet {
    /// Advances camera motion, tweens, and physics once using Bevy's virtual frame time.
    ///
    /// Systems in `Update` run before this phase. Systems in `PostUpdate` can declare
    /// `.before(TerrariumSet::Advance)` or `.after(TerrariumSet::Advance)` as appropriate.
    Advance,
}

/// Access to the host's shared egui context from a Bevy system.
#[derive(Default, Clone)]
pub struct EguiContext(egui::Context);

impl EguiContext {
    /// Returns the shared egui context.
    pub fn get(&self) -> &egui::Context {
        &self.0
    }
}

fn advance_workspace(time: Res<Time>, mut workspace: NonSendMut<Workspace>) {
    workspace.update(time.delta_secs());
}

pub(crate) fn attach_app(
    mut app: BevyApp,
    workspace: &mut Workspace,
    context: &egui::Context,
) -> Result<BevyApp, AppCreationError> {
    if !app.is_plugin_added::<TerrariumPlugin>() {
        if matches!(
            app.plugins_state(),
            PluginsState::Finished | PluginsState::Cleaned
        ) {
            return Err(std::io::Error::other(
                "install TerrariumPlugin before finalizing the Bevy app's plugins",
            )
            .into());
        }
        app.add_plugins(TerrariumPlugin);
    }

    // Preserve any scene supplied by the caller before scripts and the initializer run.
    std::mem::swap(workspace, &mut app.world_mut().non_send_mut::<Workspace>());
    app.world_mut()
        .insert_non_send(EguiContext(context.clone()));
    Ok(app)
}

pub(crate) fn finalize_app(app: &mut BevyApp, workspace: &mut Workspace) -> bool {
    with_workspace(app, workspace, finish_plugins)
}

pub(crate) fn update_app(
    app: &mut BevyApp,
    workspace: &mut Workspace,
    context: &egui::Context,
    delta: f32,
) -> Option<f32> {
    with_workspace(app, workspace, |app| {
        if !finish_plugins(app) {
            return None;
        }
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            delta,
        )));
        app.update();
        if app.should_exit().is_some() {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        Some(app.world().resource::<Time>().delta_secs())
    })
}

fn finish_plugins(app: &mut BevyApp) -> bool {
    match app.plugins_state() {
        PluginsState::Adding => {
            #[cfg(not(target_arch = "wasm32"))]
            bevy_tasks::tick_global_task_pools_on_main_thread();
            false
        }
        PluginsState::Ready => {
            app.finish();
            app.cleanup();
            true
        }
        PluginsState::Finished => {
            app.cleanup();
            true
        }
        PluginsState::Cleaned => true,
    }
}

fn with_workspace<T>(
    app: &mut BevyApp,
    workspace: &mut Workspace,
    update: impl FnOnce(&mut BevyApp) -> T,
) -> T {
    std::mem::swap(workspace, &mut app.world_mut().non_send_mut::<Workspace>());
    let scope = WorkspaceScope { app, workspace };
    update(&mut *scope.app)
}

// Move the scene, never clone it or allocate a new workspace per update. Restore it even if a
// user plugin/system panics, so imperative callbacks and the renderer always use the same scene.
struct WorkspaceScope<'a> {
    app: &'a mut BevyApp,
    workspace: &'a mut Workspace,
}

impl Drop for WorkspaceScope<'_> {
    fn drop(&mut self) {
        std::mem::swap(
            self.workspace,
            &mut self.app.world_mut().non_send_mut::<Workspace>(),
        );
    }
}
