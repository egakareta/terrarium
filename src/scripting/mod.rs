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
pub(crate) use discovery::DiscoveredScripts;
#[cfg(test)]
use discovery::{SCRIPT_MANIFEST_FILE, parse_script_manifest};
pub use discovery::{SCRIPTING_TYPES, ScriptDirectories, ScriptOptions, setup_script_project};

/// The result type returned by JavaScript runtime operations.
pub type JavaScriptResult<T> = Result<T, Error>;

mod runtime;
pub use runtime::JavaScriptRuntime;
use runtime::{BuiltModule, build_module};

struct ScriptModule {
    instance: Persistent<Object<'static>>,
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

struct ScriptHostInner {
    modules: HashMap<String, ScriptModule>,
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
pub struct ScriptHost {
    inner: Rc<RefCell<ScriptHostInner>>,
}

impl ScriptHost {
    /// Creates a JavaScript host with the built-in Terrarium API installed.
    pub fn new() -> JavaScriptResult<Self> {
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
    pub fn load_module(&self, options: ScriptOptions) -> JavaScriptResult<()> {
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
            let module = build_module(&inner.runtime, &options.module_path(), &source)?;
            call_hook(&inner.runtime, &module.instance, "onInit")?;
            call_hook(&inner.runtime, &module.instance, "onLoad")?;
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
    pub fn reload_module(&self, name: &str, source: &str) -> JavaScriptResult<()> {
        let mut inner = self.inner.borrow_mut();
        with_command_rollback(&mut inner, |inner| reload_inner(inner, name, source, None))
    }

    /// Unloads a module and invokes its unload hook.
    pub fn unload_module(&self, name: &str) -> JavaScriptResult<bool> {
        let mut inner = self.inner.borrow_mut();
        if !inner.modules.contains_key(name) {
            return Ok(false);
        }
        with_command_rollback(&mut inner, |inner| {
            let module = inner.modules.get(name).expect("module exists");
            call_hook(&inner.runtime, &module.instance, "onUnload")
        })?;
        inner.modules.remove(name);
        Ok(true)
    }

    /// Stops polling a module's native file or web URL for changes.
    pub fn stop_module_watch(&self, name: &str) -> JavaScriptResult<()> {
        if let Some(module) = self.inner.borrow_mut().modules.get_mut(name) {
            module.watch = false;
        }
        Ok(())
    }

    /// Runs module update hooks and reloads changed native files or web sources.
    pub fn update(&self, delta_time: f32) -> JavaScriptResult<()> {
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

        let names = inner.modules.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let module = inner.modules.get(&name).expect("module exists");
            if module.update_failed {
                continue;
            }
            let instance = &module.instance;
            if let Err(error) = call_update(&inner.runtime, instance, delta_time) {
                inner
                    .modules
                    .get_mut(&name)
                    .expect("module exists")
                    .update_failed = true;
                inner.last_update_error_module = Some(name);
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
    pub fn apply_commands(&self, workspace: &mut Workspace) {
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
    let replacement = build_module(&inner.runtime, &old.module_path, source)?;
    // Prepare the replacement before letting the old module tear down its scene.
    call_hook(&inner.runtime, &replacement.instance, "onInit")?;
    call_hook(&inner.runtime, &replacement.instance, "onLoad")?;
    call_hook(&inner.runtime, &replacement.instance, "onReload")?;
    call_hook(&inner.runtime, &old.instance, "onBeforeReload")?;
    call_hook(&inner.runtime, &old.instance, "onUnload")?;
    let module = ScriptModule {
        instance: replacement.instance,
        module_path: old.module_path.clone(),
        native_file: old.native_file.clone(),
        #[cfg(target_arch = "wasm32")]
        web_url: old.web_url.clone(),
        #[cfg(target_arch = "wasm32")]
        last_source: Some(source.to_owned()),
        #[cfg(target_arch = "wasm32")]
        poll_in_flight: old.poll_in_flight,
        watch: old.watch,
        update_failed: false,
        poll_interval: old.poll_interval,
        last_modified: modified.or(old.last_modified),
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
            instance: module.instance,
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

    use super::*;
    use crate::{HasPVInstance, Instance, Part};

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

        setup_script_project(&directory).unwrap();

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
            ScriptDirectories::new()
                .with_native_directory(&scripts)
                .native_options()
                .unwrap()
                .entries
                .iter()
                .map(ScriptOptions::identity)
                .collect::<Vec<_>>(),
            ["alpha", "zeta"]
        );

        for path in [
            directory.join("terrarium.d.ts"),
            directory.join("jsconfig.json"),
            scripts.join(SCRIPT_MANIFEST_FILE),
        ] {
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_readonly(true);
            fs::set_permissions(path, permissions).unwrap();
        }
        setup_script_project(&directory).unwrap();

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn runtime_keeps_global_state_between_evaluations() {
        let runtime = JavaScriptRuntime::new().expect("runtime should initialize");
        runtime
            .run("globalThis.counter = (globalThis.counter || 0) + 1;")
            .unwrap();
        runtime
            .run("if (globalThis.counter !== 1) throw new Error('state was lost');")
            .unwrap();
    }

    #[test]
    fn instance_api_applies_creation_and_update_commands() {
        let host = ScriptHost::new().unwrap();
        host.load_module(ScriptOptions::inline(
            "scene",
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
        host.load_module(ScriptOptions::inline(
            "reload",
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
        host.load_module(ScriptOptions::inline(
            "failing",
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
        host.load_module(ScriptOptions::inline(
            "dependency",
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
    fn reload_failure_keeps_the_previous_module_running() {
        let host = ScriptHost::new().unwrap();
        host.load_module(ScriptOptions::inline(
            "reload",
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
        host.load_module(ScriptOptions::inline(
            "already loaded",
            "export default class Scene { onLoad() { new Instance('Part', { name: 'kept' }); } }",
        ))
        .unwrap();
        let mut workspace = Workspace::new();
        assert!(
            host.load_module(ScriptOptions::inline(
                "scene",
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

        host.load_module(ScriptOptions::inline(
            "scene",
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
            .load_module(ScriptOptions::inline("scene", failing))
            .unwrap_err();
        assert!(host.format_error(&error).contains("module failed"));
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        assert_eq!(workspace.get_all::<Part>().count(), 0);

        host.load_module(ScriptOptions::inline(
            "scene",
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
            host.load_module(ScriptOptions::inline(
                "failing",
                r#"export default class Scene {
                    onLoad() { throw new Error('failed'); }
                }
                Promise.resolve().then(() => new Instance('Part', { name: 'deferred' }));"#,
            ))
            .is_err()
        );
        let mut workspace = Workspace::new();
        host.apply_commands(&mut workspace);
        host.load_module(ScriptOptions::inline(
            "working",
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
            .load_module(ScriptOptions::inline(
                "failing",
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
        host.load_module(ScriptOptions::inline(
            "working",
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
            .load_module(ScriptOptions::inline(
                "async scene",
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
        host.load_module(ScriptOptions::inline(
            "scene",
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
        host.load_module(ScriptOptions::inline(
            "scene",
            "export default class Scene { onLoad() { new Instance('Part', { name: 'original' }); } }",
        ))
        .unwrap();
        assert!(
            host.load_module(ScriptOptions::inline(
                "scene",
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
        host.load_module(ScriptOptions::inline(
            "scene",
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
        setup_script_project(project).unwrap();

        let options = ScriptDirectories::new()
            .with_native_directory(directory)
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
