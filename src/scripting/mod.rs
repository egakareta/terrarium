use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    rc::Rc,
    time::SystemTime as FileSystemTime,
};

use rquickjs::{Error, Function, Object, Persistent, function::Args, promise::MaybePromise};
use web_time::{Duration, Instant};

use crate::{InstanceId, Workspace};

mod scene_bridge;
use scene_bridge::{EngineCommand, apply_command, install_instance_bridge};
mod discovery;
pub(crate) use discovery::{DiscoveredScripts, ScriptOptions};
#[cfg(test)]
use discovery::{SCRIPT_MANIFEST_FILE, parse_script_manifest};
pub use discovery::{SCRIPTING_TYPES, ScriptProject, setup_script_project};

/// The result type returned by JavaScript runtime operations.
pub(crate) type JavaScriptResult<T> = Result<T, Error>;

mod runtime;
use runtime::{BuiltModule, BuiltScriptNode, JavaScriptRuntime, build_module};

struct ScriptModule {
    constructors: Vec<Persistent<Function<'static>>>,
    module_path: String,
    native_file: Option<PathBuf>,
    #[cfg(target_arch = "wasm32")]
    web_url: Option<String>,
    #[cfg(target_arch = "wasm32")]
    last_source: Option<String>,
    #[cfg(target_arch = "wasm32")]
    poll_in_flight: bool,
    watch: bool,
    update_failed: bool,
    last_modified: Option<FileSystemTime>,
    poll_interval: Duration,
    last_poll: Instant,
}

struct ScriptDependencyNode {
    instance: Persistent<Object<'static>>,
    owners: Vec<String>,
}

#[derive(Default)]
struct ScriptDependencyGraph {
    nodes: HashMap<Persistent<Function<'static>>, ScriptDependencyNode>,
    order: Vec<Persistent<Function<'static>>>,
}

impl ScriptDependencyGraph {
    fn known_instances(
        &self,
    ) -> HashMap<Persistent<Function<'static>>, Persistent<Object<'static>>> {
        self.nodes
            .iter()
            .map(|(constructor, node)| (constructor.clone(), node.instance.clone()))
            .collect()
    }

    fn retain(&mut self, owner: &str, nodes: &[BuiltScriptNode]) {
        for built_node in nodes {
            if let Some(node) = self.nodes.get_mut(&built_node.constructor) {
                node.owners.push(owner.to_owned());
            } else {
                self.nodes.insert(
                    built_node.constructor.clone(),
                    ScriptDependencyNode {
                        instance: built_node.instance.clone(),
                        owners: vec![owner.to_owned()],
                    },
                );
                self.order.push(built_node.constructor.clone());
            }
        }
    }

    fn orphaned_instances(
        &self,
        owner: &str,
        constructors: &[Persistent<Function<'static>>],
        retained: &HashSet<Persistent<Function<'static>>>,
    ) -> Vec<Persistent<Object<'static>>> {
        constructors
            .iter()
            .filter(|constructor| !retained.contains(*constructor))
            .filter_map(|constructor| {
                let node = self.nodes.get(constructor)?;
                node.owners
                    .iter()
                    .all(|node_owner| node_owner == owner)
                    .then(|| node.instance.clone())
            })
            .collect()
    }

    fn release(&mut self, owner: &str, constructors: &[Persistent<Function<'static>>]) {
        for constructor in constructors {
            if let Some(node) = self.nodes.get_mut(constructor)
                && let Some(index) = node
                    .owners
                    .iter()
                    .position(|node_owner| node_owner == owner)
            {
                node.owners.remove(index);
            }
        }
        self.nodes.retain(|_, node| !node.owners.is_empty());
        self.order
            .retain(|constructor| self.nodes.contains_key(constructor));
    }
}

struct ScriptHostInner {
    modules: HashMap<String, ScriptModule>,
    dependency_graph: ScriptDependencyGraph,
    handles: HashMap<u64, InstanceId>,
    live_handles: Rc<RefCell<HashSet<u64>>>,
    commands: Rc<RefCell<Vec<EngineCommand>>>,
    time: f32,
    last_update_error_module: Option<String>,
    #[cfg(target_arch = "wasm32")]
    poll_results: Rc<RefCell<Vec<(String, String, Result<String, String>)>>>,
    runtime: JavaScriptRuntime,
}

/// Hosts JavaScript modules and applies their scene commands to a workspace.
#[derive(Clone)]
pub(crate) struct ScriptHost {
    inner: Rc<RefCell<ScriptHostInner>>,
}

impl ScriptHost {
    /// Creates a JavaScript host with the built-in Terrarium API installed.
    pub(crate) fn new() -> JavaScriptResult<Self> {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let next_handle = Rc::new(Cell::new(1));
        let live_handles = Rc::new(RefCell::new(HashSet::new()));
        let runtime = JavaScriptRuntime::new()?;
        install_instance_bridge(
            &runtime,
            Rc::clone(&commands),
            Rc::clone(&next_handle),
            Rc::clone(&live_handles),
        )
        .map_err(|error| {
            if error.is_exception() {
                Error::new_from_js_message(
                    "Terrarium JS API",
                    "initialized API",
                    runtime.format_error(&error),
                )
            } else {
                error
            }
        })?;

        Ok(Self {
            inner: Rc::new(RefCell::new(ScriptHostInner {
                modules: HashMap::new(),
                dependency_graph: ScriptDependencyGraph::default(),
                handles: HashMap::new(),
                live_handles,
                commands,
                time: 0.0,
                last_update_error_module: None,
                #[cfg(target_arch = "wasm32")]
                poll_results: Rc::new(RefCell::new(Vec::new())),
                runtime,
            })),
        })
    }

    /// Loads and initializes a JavaScript module.
    pub(crate) fn load_module(&self, options: ScriptOptions) -> JavaScriptResult<()> {
        let name = options.identity();
        let mut inner = self.inner.borrow_mut();
        if inner.modules.contains_key(&name) {
            return Err(Error::new_from_js_message(
                "module",
                "unused module name",
                format!("`{name}` is already loaded; use reload_module"),
            ));
        }
        let previous_source = inner.runtime.register_source(&options);
        let result = with_command_rollback(&mut inner, |inner| {
            let source = options.load_source()?;
            let known_instances = inner.dependency_graph.known_instances();
            let module = build_module(
                &inner.runtime,
                &options.module_path(),
                &source,
                &known_instances,
            )?;
            let new_instances = newly_created_instances(&module.nodes);
            call_hooks(&inner.runtime, &new_instances, "onInit", false)?;
            call_hooks(&inner.runtime, &new_instances, "onLoad", false)?;
            inner.dependency_graph.retain(&name, &module.nodes);
            inner
                .modules
                .insert(name, ScriptModule::from_options(module, options));
            Ok(())
        });
        if result.is_err() {
            inner.runtime.restore_source(previous_source);
        }
        result
    }

    pub(crate) fn register_sources(&self, options: &[ScriptOptions]) {
        self.inner.borrow().runtime.register_sources(options);
    }

    /// Reloads a module while retaining the previous scene and module when a hook fails.
    #[cfg(test)]
    pub(crate) fn reload_module(&self, name: &str, source: &str) -> JavaScriptResult<()> {
        let mut inner = self.inner.borrow_mut();
        with_command_rollback(&mut inner, |inner| reload_inner(inner, name, source, None))
    }

    /// Unloads a module and invokes its unload hook.
    #[cfg(test)]
    pub(crate) fn unload_module(&self, name: &str) -> JavaScriptResult<bool> {
        let mut inner = self.inner.borrow_mut();
        if !inner.modules.contains_key(name) {
            return Ok(false);
        }
        let constructors = inner
            .modules
            .get(name)
            .expect("module exists")
            .constructors
            .clone();
        let orphaned =
            inner
                .dependency_graph
                .orphaned_instances(name, &constructors, &HashSet::new());
        with_command_rollback(&mut inner, |inner| {
            call_hooks(&inner.runtime, &orphaned, "onUnload", true)
        })?;
        let module = inner.modules.remove(name).expect("module exists");
        inner.dependency_graph.release(name, &module.constructors);
        Ok(true)
    }

    /// Stops polling a module's native file or web URL for changes.
    /// Runs module update hooks and reloads changed native files or web sources.
    pub(crate) fn update(&self, delta_time: f32) -> JavaScriptResult<()> {
        let mut inner = self.inner.borrow_mut();
        inner.last_update_error_module = None;
        inner.time += delta_time;
        let time = inner.time;
        inner
            .runtime
            .with_context(|ctx| ctx.globals().set("time", time))?;
        let now = Instant::now();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let changed = inner
                .modules
                .iter_mut()
                .filter_map(|(name, module)| {
                    if !module.watch || now.duration_since(module.last_poll) < module.poll_interval
                    {
                        return None;
                    }
                    module.last_poll = now;
                    let path = module.native_file.clone()?;
                    let modified = fs::metadata(&path).ok()?.modified().ok()?;
                    if module.last_modified == Some(modified) {
                        return None;
                    }
                    Some((name.clone(), path, modified))
                })
                .collect::<Vec<_>>();

            for (name, path, modified) in changed {
                let result = with_command_rollback(&mut inner, |inner| {
                    let source = fs::read_to_string(&path).map_err(|error| {
                        Error::new_from_js_message("script file", "string", error.to_string())
                    })?;
                    reload_inner(inner, &name, &source, Some(modified))
                });
                if let Err(error) = result {
                    // Wait for another file change rather than retrying a broken source every frame.
                    inner
                        .modules
                        .get_mut(&name)
                        .expect("module exists")
                        .last_modified = Some(modified);
                    inner.last_update_error_module = Some(name);
                    return Err(error);
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        poll_web_sources(&mut inner, now)?;

        let updates = inner
            .dependency_graph
            .order
            .iter()
            .filter_map(|constructor| {
                let node = inner.dependency_graph.nodes.get(constructor)?;
                let active_owners = node
                    .owners
                    .iter()
                    .filter(|owner| {
                        inner
                            .modules
                            .get(owner.as_str())
                            .is_some_and(|module| !module.update_failed)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                (!active_owners.is_empty()).then(|| (node.instance.clone(), active_owners))
            })
            .collect::<Vec<_>>();
        for (instance, owners) in updates {
            if let Err(error) = call_update(&inner.runtime, &instance, delta_time) {
                for owner in &owners {
                    if let Some(module) = inner.modules.get_mut(owner) {
                        module.update_failed = true;
                    }
                }
                inner.last_update_error_module = owners.first().cloned();
                return Err(error);
            }
        }
        Ok(())
    }

    pub(crate) fn format_error(&self, error: &Error) -> String {
        self.inner.borrow().runtime.format_error(error)
    }

    pub(crate) fn take_last_update_error_module(&self) -> Option<String> {
        self.inner.borrow_mut().last_update_error_module.take()
    }

    /// Applies queued JavaScript scene commands to a workspace.
    pub(crate) fn apply_commands(&self, workspace: &mut Workspace) {
        let mut inner = self.inner.borrow_mut();
        let commands = inner.commands.borrow_mut().drain(..).collect::<Vec<_>>();
        let live_handles = Rc::clone(&inner.live_handles);
        let mut live_handles = live_handles.borrow_mut();
        for command in commands {
            apply_command(&mut inner.handles, &mut live_handles, workspace, command);
        }
    }
}

fn with_command_rollback<T>(
    inner: &mut ScriptHostInner,
    operation: impl FnOnce(&mut ScriptHostInner) -> JavaScriptResult<T>,
) -> JavaScriptResult<T> {
    let start = inner.commands.borrow().len();
    let result = operation(inner);
    if result.is_err() {
        let abandoned = inner
            .commands
            .borrow_mut()
            .drain(start..)
            .filter_map(|command| match command {
                EngineCommand::AddPart { handle, .. } => Some(handle),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut live_handles = inner.live_handles.borrow_mut();
        for handle in abandoned {
            live_handles.remove(&handle);
        }
    }
    result
}

fn reload_inner(
    inner: &mut ScriptHostInner,
    name: &str,
    source: &str,
    modified: Option<FileSystemTime>,
) -> JavaScriptResult<()> {
    let old = inner
        .modules
        .get(name)
        .ok_or_else(|| Error::new_from_js_message("module", "loaded module", name))?;
    let old_constructors = old.constructors.clone();
    let old_module_path = old.module_path.clone();
    let old_native_file = old.native_file.clone();
    #[cfg(target_arch = "wasm32")]
    let old_web_url = old.web_url.clone();
    #[cfg(target_arch = "wasm32")]
    let old_poll_in_flight = old.poll_in_flight;
    let old_watch = old.watch;
    let old_poll_interval = old.poll_interval;
    let old_last_modified = old.last_modified;
    let known_instances = inner.dependency_graph.known_instances();
    let replacement = build_module(&inner.runtime, &old_module_path, source, &known_instances)?;
    let replacement_constructors = replacement
        .nodes
        .iter()
        .map(|node| node.constructor.clone())
        .collect::<HashSet<_>>();
    let orphaned = inner.dependency_graph.orphaned_instances(
        name,
        &old_constructors,
        &replacement_constructors,
    );
    let new_instances = newly_created_instances(&replacement.nodes);
    // Prepare the replacement before letting the old module tear down its scene.
    call_hooks(&inner.runtime, &new_instances, "onInit", false)?;
    call_hooks(&inner.runtime, &new_instances, "onLoad", false)?;
    call_hooks(&inner.runtime, &new_instances, "onReload", false)?;
    call_hooks(&inner.runtime, &orphaned, "onBeforeReload", true)?;
    call_hooks(&inner.runtime, &orphaned, "onUnload", true)?;
    inner.dependency_graph.retain(name, &replacement.nodes);
    inner.dependency_graph.release(name, &old_constructors);
    let module = ScriptModule {
        constructors: replacement
            .nodes
            .into_iter()
            .map(|node| node.constructor)
            .collect(),
        module_path: old_module_path,
        native_file: old_native_file,
        #[cfg(target_arch = "wasm32")]
        web_url: old_web_url,
        #[cfg(target_arch = "wasm32")]
        last_source: Some(source.to_owned()),
        #[cfg(target_arch = "wasm32")]
        poll_in_flight: old_poll_in_flight,
        watch: old_watch,
        update_failed: false,
        poll_interval: old_poll_interval,
        last_modified: modified.or(old_last_modified),
        last_poll: Instant::now(),
    };
    inner.modules.insert(name.to_owned(), module);
    Ok(())
}

impl ScriptModule {
    fn from_options(module: BuiltModule, options: ScriptOptions) -> Self {
        let last_modified = options
            .native_file
            .as_ref()
            .and_then(|path| fs::metadata(path).ok()?.modified().ok());
        Self {
            constructors: module
                .nodes
                .into_iter()
                .map(|node| node.constructor)
                .collect(),
            module_path: options.module_path(),
            native_file: options.native_file,
            #[cfg(target_arch = "wasm32")]
            web_url: options.web_url,
            #[cfg(target_arch = "wasm32")]
            last_source: options.source,
            #[cfg(target_arch = "wasm32")]
            poll_in_flight: false,
            watch: options.watch,
            update_failed: false,
            last_modified,
            poll_interval: options.poll_interval,
            last_poll: Instant::now(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn poll_web_sources(inner: &mut ScriptHostInner, now: Instant) -> JavaScriptResult<()> {
    let mut results = std::mem::take(&mut *inner.poll_results.borrow_mut()).into_iter();
    while let Some((name, url, result)) = results.next() {
        let Some(module) = inner.modules.get_mut(&name) else {
            continue;
        };
        if !module.watch || !module.poll_in_flight || module.web_url.as_deref() != Some(&url) {
            continue;
        }
        module.poll_in_flight = false;
        let source = match result {
            Ok(source) => source,
            Err(message) => {
                inner.poll_results.borrow_mut().extend(results);
                inner.last_update_error_module = Some(name);
                return Err(Error::new_from_js_message(
                    "script resource",
                    "readable source",
                    message,
                ));
            }
        };
        if module.last_source.as_deref() == Some(&source) {
            continue;
        }
        if let Err(error) =
            with_command_rollback(inner, |inner| reload_inner(inner, &name, &source, None))
        {
            // Do not retry invalid source every frame; a subsequent edit will be retried.
            inner
                .modules
                .get_mut(&name)
                .expect("module exists")
                .last_source = Some(source);
            inner.poll_results.borrow_mut().extend(results);
            inner.last_update_error_module = Some(name);
            return Err(error);
        }
    }

    for (name, module) in &mut inner.modules {
        if !module.watch
            || module.poll_in_flight
            || now.duration_since(module.last_poll) < module.poll_interval
        {
            continue;
        }
        let Some(url) = module.web_url.clone() else {
            continue;
        };
        module.last_poll = now;
        module.poll_in_flight = true;
        let results = Rc::clone(&inner.poll_results);
        let name = name.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = discovery::fetch_text(&url, &format!("script `{name}`"))
                .await
                .map_err(|error| error.to_string());
            results.borrow_mut().push((name, url, result));
        });
    }
    Ok(())
}

fn call_hook(
    runtime: &JavaScriptRuntime,
    persistent: &Persistent<Object<'static>>,
    name: &str,
) -> JavaScriptResult<()> {
    runtime.with_context(|ctx| {
        let result: JavaScriptResult<()> = (|| {
            let instance = persistent.clone().restore(&ctx)?;
            let Some(method) = instance.get::<_, Option<Function>>(name)? else {
                return Ok(());
            };
            let mut args = Args::new(ctx.clone(), 0);
            args.this(instance)?;
            let promise: MaybePromise = args.apply(&method)?;
            promise.finish::<()>()?;
            Ok(())
        })();
        // Promise callbacks scheduled by the hook must run before its commands are committed
        // or rolled back. Save its original exception while running those jobs.
        let exception = result
            .as_ref()
            .err()
            .filter(|error| error.is_exception())
            .map(|_| ctx.catch());
        while ctx.execute_pending_job() {}
        if let Some(exception) = exception {
            return Err(ctx.throw(exception));
        }
        result
    })
}

fn call_hooks(
    runtime: &JavaScriptRuntime,
    instances: &[Persistent<Object<'static>>],
    name: &str,
    reverse: bool,
) -> JavaScriptResult<()> {
    if reverse {
        for instance in instances.iter().rev() {
            call_hook(runtime, instance, name)?;
        }
    } else {
        for instance in instances {
            call_hook(runtime, instance, name)?;
        }
    }
    Ok(())
}

fn newly_created_instances(module_nodes: &[BuiltScriptNode]) -> Vec<Persistent<Object<'static>>> {
    module_nodes
        .iter()
        .filter(|node| node.is_new)
        .map(|node| node.instance.clone())
        .collect()
}

fn call_update(
    runtime: &JavaScriptRuntime,
    persistent: &Persistent<Object<'static>>,
    delta_time: f32,
) -> JavaScriptResult<()> {
    runtime.with_context(|ctx| {
        let instance = persistent.clone().restore(&ctx)?;
        let Some(method) = instance.get::<_, Option<Function>>("onUpdate")? else {
            return Ok(());
        };
        let mut args = Args::new(ctx.clone(), 1);
        args.this(instance)?;
        args.push_arg(delta_time)?;
        let _: () = args.apply(&method)?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use glam::Vec3;
    use rquickjs::Ctx;

    use super::*;
    use crate::{HasPVInstance, Instance, Part};

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn embedded_scripts_resolve_imports_without_files() {
        static BUNDLE: include_dir::Dir<'static> =
            include_dir::include_dir!("$CARGO_MANIFEST_DIR/tests/fixtures/scripts");
        let scripts = ScriptProject::embedded(&BUNDLE, "tests/fixtures/scripts")
            .native_options()
            .unwrap();

        let host = ScriptHost::new().unwrap();
        host.register_sources(&scripts.sources);
        for entry in scripts.entries {
            host.load_module(entry).unwrap();
        }
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        let names = workspace
            .get_all::<Part>()
            .map(Instance::name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["embedded helper"]);
    }

    #[test]
    fn setup_generates_a_sorted_script_manifest() {
        let directory = std::env::temp_dir().join(format!(
            "terrarium-script-setup-{}-{}",
            std::process::id(),
            FileSystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let scripts = directory.join("scripts");
        fs::create_dir_all(&scripts).unwrap();
        fs::write(scripts.join("zeta.js"), "export default class Zeta {}").unwrap();
        fs::write(scripts.join("alpha.js"), "export default class Alpha {}").unwrap();
        fs::write(
            scripts.join("math.js"),
            "export const answer = 'export default class';",
        )
        .unwrap();
        fs::write(
            scripts.join("notes.js"),
            "// export default class Fake {}\nexport const note = true;",
        )
        .unwrap();
        fs::create_dir(scripts.join("helpers")).unwrap();
        fs::write(
            scripts.join("helpers").join("other.js"),
            "export default class Other {}",
        )
        .unwrap();
        fs::write(scripts.join("notes.txt"), "not a script").unwrap();

        setup_script_project(&directory, "scripts").unwrap();

        let manifest =
            parse_script_manifest(&fs::read_to_string(scripts.join(SCRIPT_MANIFEST_FILE)).unwrap())
                .unwrap();
        assert_eq!(
            manifest
                .scripts
                .iter()
                .map(|script| script.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "zeta"]
        );
        assert_eq!(
            manifest.modules,
            ["helpers/other.js", "math.js", "notes.js"]
        );
        assert_eq!(
            ScriptProject::development(&scripts, "./scripts")
                .native_options()
                .unwrap()
                .entries
                .iter()
                .map(ScriptOptions::identity)
                .collect::<Vec<_>>(),
            ["alpha", "zeta"]
        );

        let custom_scripts = directory.join("assets/scripts");
        fs::create_dir_all(&custom_scripts).unwrap();
        fs::write(
            custom_scripts.join("custom.js"),
            "export default class Custom {}",
        )
        .unwrap();
        setup_script_project(&directory, "assets/scripts").unwrap();
        let custom_manifest = parse_script_manifest(
            &fs::read_to_string(custom_scripts.join(SCRIPT_MANIFEST_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(custom_manifest.scripts[0].path, "custom.js");

        for path in [
            directory.join("terrarium.d.ts"),
            directory.join("jsconfig.json"),
            scripts.join(SCRIPT_MANIFEST_FILE),
        ] {
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_readonly(true);
            fs::set_permissions(path, permissions).unwrap();
        }
        setup_script_project(&directory, "scripts").unwrap();

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn runtime_keeps_global_state_between_evaluations() {
        let runtime = JavaScriptRuntime::new().expect("runtime should initialize");

        runtime
            .with_context(|ctx: Ctx<'_>| {
                ctx.eval::<(), _>("globalThis.counter = (globalThis.counter || 0) + 1;")
            })
            .unwrap();
        runtime
            .with_context(|ctx: Ctx<'_>| {
                ctx.eval::<(), _>(
                    "if (globalThis.counter !== 1) throw new Error('state was lost');",
                )
            })
            .unwrap();
    }

    #[test]
    fn instance_api_applies_creation_and_update_commands() {
        let host = ScriptHost::new().unwrap();
        host.load_module(ScriptOptions::new("scene").with_source(
            r#"
                export default class Scene {
                    onLoad() {
                        this.part = new Instance("Part", {
                            name: "scripted",
                            position: [1, 2, 3],
                        });
                    }
                    onUpdate() {
                        this.part.setPosition([4, 5, 6]);
                    }
                }
            "#,
        ))
        .unwrap();

        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        host.update(1.0 / 60.0).unwrap();
        host.apply_commands(&mut workspace);

        let part = workspace.get_all::<Part>().next().unwrap();
        assert_eq!(part.name(), "scripted");
        assert_eq!(part.position(), Vec3::new(4.0, 5.0, 6.0));
    }

    #[test]
    fn reload_runs_the_replacement_load_hook() {
        let host = ScriptHost::new().unwrap();
        host.load_module(ScriptOptions::new("reload").with_source(
            r#"
                export default class Scene {
                    onLoad() {
                        new Instance("Part", { name: "old" });
                    }
                }
            "#,
        ))
        .unwrap();
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);

        host.reload_module(
            "reload",
            r#"
                export default class Scene {
                    onLoad() {
                        new Instance("Part", { name: "new" });
                    }
                    onReload() {
                        new Instance("Part", { name: "reloaded" });
                    }
                }
            "#,
        )
        .unwrap();
        host.apply_commands(&mut workspace);

        let names = workspace
            .get_all::<Part>()
            .map(Instance::name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["old", "new", "reloaded"]);
    }

    #[test]
    fn watched_module_is_not_reloaded_until_its_file_changes() {
        let path = std::env::temp_dir().join(format!(
            "terrarium-watch-{}-{}.js",
            std::process::id(),
            FileSystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &path,
            r#"
                export default class Scene {
                    onLoad() {
                        new Instance("Part", { name: "loaded" });
                    }
                    onReload() {
                        new Instance("Part", { name: "reloaded" });
                    }
                }
            "#,
        )
        .unwrap();

        let host = ScriptHost::new().unwrap();
        host.load_module(
            ScriptOptions::new("watched")
                .with_native_file(path.clone())
                .with_watch(true)
                .with_poll_interval(Duration::ZERO),
        )
        .unwrap();
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        host.update(1.0 / 60.0).unwrap();
        host.apply_commands(&mut workspace);

        assert_eq!(workspace.get_all::<Part>().count(), 1);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_update_is_disabled_until_module_reload() {
        let host = ScriptHost::new().unwrap();
        host.load_module(ScriptOptions::new("failing").with_source(
            r#"
                export default class Scene {
                    onUpdate() {
                        throw new Error("update failed");
                    }
                }
            "#,
        ))
        .unwrap();

        let error = host.update(1.0 / 60.0).unwrap_err();
        assert!(host.format_error(&error).contains("Error: update failed"));
        assert!(host.update(1.0 / 60.0).is_ok());

        host.reload_module(
            "failing",
            r#"
                export default class Scene {
                    onUpdate() {}
                }
            "#,
        )
        .unwrap();
        assert!(host.update(1.0 / 60.0).is_ok());
    }

    #[test]
    fn same_module_constructor_dependencies_are_injected() {
        let host = ScriptHost::new().unwrap();
        host.load_module(ScriptOptions::new("dependency").with_source(
            r#"
                export class Factory {
                    constructor() {
                        this.name = "injected";
                    }
                }

                export default class Scene {
                    /** @param {Factory} factory */
                    constructor(factory) {
                        this.factory = factory;
                    }
                    onLoad() {
                        new Instance("Part", { name: this.factory.name });
                    }
                }
            "#,
        ))
        .unwrap();

        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        assert_eq!(
            workspace.get_all::<Part>().next().unwrap().name(),
            "injected"
        );
    }

    #[test]
    fn imported_dependency_graph_runs_lifecycle_hooks_in_graph_order() {
        let host = ScriptHost::new().unwrap();
        let orbit_source = r#"
            globalThis.lifecycleTrace = [];
            export default class Orbit {
                onInit() { globalThis.lifecycleTrace.push("orbit:init"); }
                onLoad() { globalThis.lifecycleTrace.push("orbit:load"); }
                onUpdate() { globalThis.lifecycleTrace.push("orbit:update"); }
                onUnload() {
                    globalThis.lifecycleTrace.push("orbit:unload");
                    globalThis.lifecyclePart.setName(globalThis.lifecycleTrace.join("|"));
                }
            }
        "#;
        let ring_source = r#"
            import Orbit from "./orbit.js";
            export default class Ring {
                /** @param {Orbit} orbit */
                constructor(orbit) { this.orbit = orbit; }
                onInit() { globalThis.lifecycleTrace.push("ring:init"); }
                onLoad() { globalThis.lifecycleTrace.push("ring:load"); }
                onUpdate() { globalThis.lifecycleTrace.push("ring:update"); }
                onUnload() {
                    globalThis.lifecycleTrace.push("ring:unload");
                    globalThis.lifecyclePart.setName(globalThis.lifecycleTrace.join("|"));
                }
            }
        "#;
        host.register_sources(&[
            ScriptOptions::new("orbit.js").with_source(orbit_source),
            ScriptOptions::new("ring.js").with_source(ring_source),
        ]);
        host.load_module(ScriptOptions::new("ring.js").with_source(ring_source))
            .unwrap();
        host.load_module(ScriptOptions::new("scene.js").with_source(
            r#"
                import Ring from "./ring.js";
                import Orbit from "./orbit.js";
                export default class Scene {
                    /**
                     * @param {Ring} ring
                     * @param {Orbit} orbit
                     */
                    constructor(ring, orbit) {
                        this.ring = ring;
                        this.orbit = orbit;
                    }
                    onInit() { globalThis.lifecycleTrace.push("scene:init"); }
                    onLoad() {
                        globalThis.lifecycleTrace.push("scene:load");
                        this.part = new Instance("Part", {
                            name: globalThis.lifecycleTrace.join("|"),
                        });
                        globalThis.lifecyclePart = this.part;
                    }
                    onUpdate() {
                        globalThis.lifecycleTrace.push("scene:update");
                        this.part.setName(globalThis.lifecycleTrace.join("|"));
                    }
                    onUnload() {
                        globalThis.lifecycleTrace.push("scene:unload");
                        this.part.setName(globalThis.lifecycleTrace.join("|"));
                    }
                }
            "#,
        ))
        .unwrap();

        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        let part = workspace.get_all::<Part>().next().unwrap();
        assert_eq!(
            part.name(),
            "orbit:init|ring:init|orbit:load|ring:load|scene:init|scene:load"
        );

        host.update(1.0 / 60.0).unwrap();
        host.apply_commands(&mut workspace);
        let part = workspace.get_all::<Part>().next().unwrap();
        assert!(
            part.name()
                .ends_with("orbit:update|ring:update|scene:update")
        );

        host.unload_module("scene.js").unwrap();
        host.apply_commands(&mut workspace);
        let part = workspace.get_all::<Part>().next().unwrap();
        assert!(part.name().ends_with("scene:unload"));

        host.unload_module("ring.js").unwrap();
        host.apply_commands(&mut workspace);
        let part = workspace.get_all::<Part>().next().unwrap();
        assert!(
            part.name()
                .ends_with("scene:unload|ring:unload|orbit:unload")
        );
    }

    #[test]
    fn reload_failure_keeps_the_previous_module_running() {
        let host = ScriptHost::new().unwrap();
        host.load_module(ScriptOptions::new("reload").with_source(
            "export default class Scene { onLoad() { new Instance('Part', { name: 'old' }); } }",
        ))
        .unwrap();
        assert!(
            host.reload_module("reload", "export default class")
                .is_err()
        );

        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        assert_eq!(workspace.get_all::<Part>().next().unwrap().name(), "old");
    }

    #[test]
    fn failed_load_discards_scene_commands_and_allows_a_retry() {
        let host = ScriptHost::new().unwrap();
        host.load_module(ScriptOptions::new("already loaded").with_source(
            "export default class Scene { onLoad() { new Instance('Part', { name: 'kept' }); } }",
        ))
        .unwrap();
        let mut workspace = Workspace::new();
        assert!(
            host.load_module(ScriptOptions::new("scene").with_source(
                "export default class Scene { onLoad() { new Instance('Part'); throw new Error('failed'); } }",
            ))
            .is_err()
        );
        host.apply_commands(&mut workspace);
        assert_eq!(
            workspace
                .get_all::<Part>()
                .map(Instance::name)
                .collect::<Vec<_>>(),
            ["kept"]
        );

        host.load_module(ScriptOptions::new("scene").with_source(
            "export default class Scene { onLoad() { new Instance('Part', { name: 'retry' }); } }",
        ))
        .unwrap();
        host.apply_commands(&mut workspace);
        assert_eq!(
            workspace
                .get_all::<Part>()
                .map(Instance::name)
                .collect::<Vec<_>>(),
            ["kept", "retry"]
        );
    }

    #[test]
    fn throwing_module_body_does_not_apply_parts_or_replace_a_scene() {
        let host = ScriptHost::new().unwrap();
        let failing = r#"
            export default class Scene {}
            new Instance('Part', { name: 'orphan' });
            throw new Error('module failed');
        "#;
        let error = host
            .load_module(ScriptOptions::new("scene").with_source(failing))
            .unwrap_err();
        assert!(host.format_error(&error).contains("module failed"));
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        assert_eq!(workspace.get_all::<Part>().count(), 0);

        host.load_module(ScriptOptions::new("scene").with_source(
            "export default class Scene { onLoad() { new Instance('Part', { name: 'old' }); } }",
        ))
        .unwrap();
        host.apply_commands(&mut workspace);
        assert!(host.reload_module("scene", failing).is_err());
        host.apply_commands(&mut workspace);
        assert_eq!(
            workspace
                .get_all::<Part>()
                .map(Instance::name)
                .collect::<Vec<_>>(),
            ["old"]
        );
    }

    #[test]
    fn failed_load_discards_module_jobs_before_the_next_scene_load() {
        let host = ScriptHost::new().unwrap();
        assert!(
            host.load_module(ScriptOptions::new("failing").with_source(
                r#"export default class Scene {
                    onLoad() { throw new Error('failed'); }
                }
                Promise.resolve().then(() => new Instance('Part', { name: 'deferred' }));"#,
            ))
            .is_err()
        );
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        host.load_module(ScriptOptions::new("working").with_source(
            "export default class Scene { onLoad() { new Instance('Part', { name: 'working' }); } }",
        ))
        .unwrap();
        host.apply_commands(&mut workspace);
        assert_eq!(
            workspace
                .get_all::<Part>()
                .map(Instance::name)
                .collect::<Vec<_>>(),
            ["working"]
        );
    }

    #[test]
    fn failed_hook_discards_its_pending_jobs_before_the_next_scene_load() {
        let host = ScriptHost::new().unwrap();
        let error = host
            .load_module(ScriptOptions::new("failing").with_source(
                r#"export default class Scene {
                    onLoad() {
                        Promise.resolve().then(() => new Instance('Part', { name: 'late' }));
                        throw new Error('load failed');
                    }
                }"#,
            ))
            .unwrap_err();
        assert!(host.format_error(&error).contains("load failed"));
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        host.load_module(ScriptOptions::new("working").with_source(
            "export default class Scene { onLoad() { new Instance('Part', { name: 'working' }); } }",
        ))
        .unwrap();
        host.apply_commands(&mut workspace);
        assert_eq!(
            workspace
                .get_all::<Part>()
                .map(Instance::name)
                .collect::<Vec<_>>(),
            ["working"]
        );
    }

    #[test]
    fn rejected_async_load_does_not_create_a_scene_part() {
        let host = ScriptHost::new().unwrap();
        let error = host
            .load_module(ScriptOptions::new("async scene").with_source(
                r#"export default class Scene {
                    async onLoad() {
                        new Instance('Part', { name: 'orphan' });
                        await Promise.resolve();
                        throw new Error('async load failed');
                    }
                }"#,
            ))
            .unwrap_err();
        assert!(host.format_error(&error).contains("async load failed"));
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        assert_eq!(workspace.get_all::<Part>().count(), 0);
    }

    #[test]
    fn failed_reload_preserves_old_scene_and_running_module() {
        let host = ScriptHost::new().unwrap();
        host.load_module(ScriptOptions::new("scene").with_source(
            r#"export default class Scene {
                onLoad() { this.part = new Instance('Part', { name: 'old' }); }
                onUnload() { this.part.destroy(); }
                onUpdate() { this.part.setName('still running'); }
            }"#,
        ))
        .unwrap();
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);

        assert!(
            host.reload_module(
                "scene",
                r#"export default class Scene {
                    onLoad() { new Instance('Part', { name: 'replacement' }); throw new Error('failed'); }
                }"#,
            )
            .is_err()
        );
        host.apply_commands(&mut workspace);
        assert_eq!(workspace.get_all::<Part>().count(), 1);
        assert_eq!(workspace.get_all::<Part>().next().unwrap().name(), "old");
        assert!(
            host.reload_module(
                "scene",
                r#"export default class Scene {
                    onLoad() { new Instance('Part', { name: 'replacement' }); }
                    onReload() { throw new Error('reload failed'); }
                }"#,
            )
            .is_err()
        );
        host.apply_commands(&mut workspace);
        assert_eq!(workspace.get_all::<Part>().count(), 1);
        assert_eq!(workspace.get_all::<Part>().next().unwrap().name(), "old");
        host.update(0.1).unwrap();
        host.apply_commands(&mut workspace);
        assert_eq!(
            workspace.get_all::<Part>().next().unwrap().name(),
            "still running"
        );
    }

    #[test]
    fn duplicate_load_keeps_the_existing_module_and_scene() {
        let host = ScriptHost::new().unwrap();
        host.load_module(ScriptOptions::new("scene").with_source(
            "export default class Scene { onLoad() { new Instance('Part', { name: 'original' }); } }",
        ))
        .unwrap();
        assert!(
            host.load_module(ScriptOptions::new("scene").with_source(
                "export default class Scene { onLoad() { new Instance('Part', { name: 'duplicate' }); } }",
            ))
            .is_err()
        );
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        assert_eq!(
            workspace
                .get_all::<Part>()
                .map(Instance::name)
                .collect::<Vec<_>>(),
            ["original"]
        );
    }

    #[test]
    fn throwing_unload_does_not_apply_partial_teardown_or_replacement() {
        let host = ScriptHost::new().unwrap();
        host.load_module(ScriptOptions::new("scene").with_source(
            r#"export default class Scene {
                onLoad() { this.part = new Instance('Part', { name: 'old' }); }
                async onUnload() {
                    this.part.destroy();
                    await Promise.resolve();
                    if (!this.failedOnce) {
                        this.failedOnce = true;
                        throw new Error('unload failed');
                    }
                }
                onUpdate() { this.part.setName('still running'); }
            }"#,
        ))
        .unwrap();
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);

        assert!(host.reload_module(
            "scene",
            "export default class Scene { onLoad() { new Instance('Part', { name: 'replacement' }); } }",
        ).is_err());
        host.apply_commands(&mut workspace);
        assert_eq!(
            workspace
                .get_all::<Part>()
                .map(Instance::name)
                .collect::<Vec<_>>(),
            ["old"]
        );
        host.update(0.1).unwrap();
        host.apply_commands(&mut workspace);
        assert_eq!(
            workspace.get_all::<Part>().next().unwrap().name(),
            "still running"
        );
        assert!(host.unload_module("scene").unwrap());
        host.apply_commands(&mut workspace);
        assert_eq!(workspace.get_all::<Part>().count(), 0);
    }

    #[test]
    fn relative_directory_imports_helpers_without_loading_them_as_scenes() {
        let directory_name = format!(
            ".terrarium-script-import-{}-{}",
            std::process::id(),
            FileSystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let project = Path::new(&directory_name);
        let directory = project.join("scripts");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("math.js"),
            "export const label = 'imported';",
        )
        .unwrap();
        fs::create_dir(directory.join("helpers")).unwrap();
        fs::write(
            directory.join("helpers/suffix.js"),
            "export const suffix = 'nested';",
        )
        .unwrap();
        fs::write(
            directory.join("scene.js"),
            "import { label } from './math.js'; import { suffix } from './helpers/suffix.js'; export default class Scene { onLoad() { new Instance('Part', { name: `${label} ${suffix}` }); } }",
        )
        .unwrap();
        setup_script_project(project, "scripts").unwrap();

        let options = ScriptProject::development(directory, "./scripts")
            .native_options()
            .unwrap();
        assert_eq!(options.entries.len(), 1);
        let host = ScriptHost::new().unwrap();
        for option in options.entries {
            host.load_module(option).unwrap();
        }
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        assert_eq!(
            workspace.get_all::<Part>().next().unwrap().name(),
            "imported nested"
        );
        fs::remove_dir_all(project).unwrap();
    }
}
