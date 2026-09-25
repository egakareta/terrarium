use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fs, io,
    path::{Component, Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
    time::SystemTime as FileSystemTime,
};

use glam::Vec3;
use rquickjs::{
    Context, Ctx, Error, Function, Module, Object, Persistent, Runtime,
    function::Args,
    loader::{ImportAttributes, Loader, Resolver},
};
use serde::{Deserialize, Serialize};
use web_time::{Duration, Instant};

use crate::{
    Color3, HasBasePart, HasPVInstance, HasPart, Instance, InstanceId, Part, PartShape, Workspace,
};

const INSTANCE_API: &str = include_str!("api.js");
const SCRIPT_MANIFEST_FILE: &str = "manifest.json";

const SCRIPTING_JSCONFIG: &str = r#"{
  "compilerOptions": {
    "checkJs": false,
    "module": "ES2022",
    "moduleResolution": "Bundler",
    "target": "ES2022"
  },
  "include": ["**/*.js", "terrarium.d.ts"]
}
"#;

/// The editor declaration file installed by [`setup_script_project`].
pub const SCRIPTING_TYPES: &str = include_str!("api.d.ts");

/// The result type returned by JavaScript runtime operations.
pub type JavaScriptResult<T> = Result<T, Error>;

/// Installs Terrarium JavaScript editor support in a consumer script directory.
///
/// The generated `terrarium.d.ts` supplies global declarations for `Instance`,
/// `time`, and `log`. The generated `jsconfig.json` makes those declarations
/// available to every JavaScript file below `directory`, without requiring
/// per-file reference directives. A sorted `scripts/manifest.json` is also
/// generated from the JavaScript files directly inside `directory/scripts`.
pub fn setup_script_project(directory: impl AsRef<Path>) -> io::Result<()> {
    let directory = directory.as_ref();
    let declarations = directory.join("terrarium.d.ts");
    let config = directory.join("jsconfig.json");
    let scripts_directory = directory.join("scripts");
    fs::create_dir_all(directory)?;
    fs::create_dir_all(&scripts_directory)?;
    write_if_changed(&declarations, SCRIPTING_TYPES.as_bytes())?;
    write_if_changed(&config, SCRIPTING_JSCONFIG.as_bytes())?;
    write_script_manifest(&scripts_directory)?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct ScriptManifest {
    version: u32,
    scripts: Vec<ScriptManifestEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ScriptManifestEntry {
    name: String,
    path: String,
}

fn write_script_manifest(directory: &Path) -> io::Result<()> {
    let manifest = ScriptManifest {
        version: 1,
        scripts: script_entries(directory)?,
    };
    let contents = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
    write_if_changed(&directory.join(SCRIPT_MANIFEST_FILE), &contents)
}

fn write_if_changed(path: &Path, contents: &[u8]) -> io::Result<()> {
    match fs::read(path) {
        Ok(existing) if existing == contents => Ok(()),
        Ok(_) => fs::write(path, contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::write(path, contents),
        Err(error) => Err(error),
    }
}

fn script_entries(directory: &Path) -> io::Result<Vec<ScriptManifestEntry>> {
    let mut scripts = fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("js") {
                return Ok(None);
            }

            let name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "script filename is not UTF-8")
                })?;
            let path = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "script filename is not UTF-8")
                })?;

            Ok(Some(ScriptManifestEntry {
                name: name.to_owned(),
                path: path.to_owned(),
            }))
        })
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    scripts.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(scripts)
}

fn parse_script_manifest(source: &str) -> JavaScriptResult<ScriptManifest> {
    let manifest: ScriptManifest = serde_json::from_str(source).map_err(|error| {
        Error::new_from_js_message("script manifest", "valid JSON", error.to_string())
    })?;
    if manifest.version != 1 {
        return Err(Error::new_from_js_message(
            "script manifest version",
            "version 1",
            manifest.version.to_string(),
        ));
    }
    Ok(manifest)
}

/// Resolves a script name to native and web locations.
#[derive(Clone, Debug, Default)]
pub struct ScriptDirectories {
    native_directory: Option<PathBuf>,
    web_directory: Option<String>,
}

impl ScriptDirectories {
    /// Creates an empty set of script directories.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the directory used to load scripts on native targets.
    pub fn with_native_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.native_directory = Some(directory.into());
        self
    }

    /// Sets the URL prefix used to load scripts on web targets.
    pub fn with_web_directory(mut self, directory: impl Into<String>) -> Self {
        self.web_directory = Some(directory.into());
        self
    }

    /// Creates options for a file relative to these directories.
    pub fn options(&self, file_name: impl AsRef<Path>) -> ScriptOptions {
        let file_name = file_name.as_ref();
        let name = file_name
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("script")
            .to_owned();
        let native_file = self
            .native_directory
            .as_ref()
            .map(|directory| directory.join(file_name));
        let web_url = self.web_directory.as_ref().map(|directory| {
            let directory = directory.trim_end_matches('/');
            let file_name = file_name.to_string_lossy().replace('\\', "/");
            format!("{directory}/{file_name}")
        });

        ScriptOptions {
            name: Some(name),
            source: None,
            native_file,
            web_url,
            watch: true,
            poll_interval: Duration::from_millis(250),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native_options(&self) -> JavaScriptResult<Vec<ScriptOptions>> {
        let directory = self.native_directory.as_ref().ok_or_else(|| {
            Error::new_from_js_message(
                "script directory",
                "a native directory",
                "none was configured",
            )
        })?;
        let manifest_path = directory.join(SCRIPT_MANIFEST_FILE);
        let entries = match fs::read_to_string(&manifest_path) {
            Ok(source) => parse_script_manifest(&source)?.scripts,
            Err(error) if error.kind() == io::ErrorKind::NotFound => script_entries(directory)
                .map_err(|error| {
                    Error::new_from_js_message(
                        "script directory",
                        "readable directory",
                        format!("{}: {error}", directory.display()),
                    )
                })?,
            Err(error) => {
                return Err(Error::new_from_js_message(
                    "script manifest",
                    "readable file",
                    format!("{}: {error}", manifest_path.display()),
                ));
            }
        };

        Ok(entries
            .into_iter()
            .map(|entry| self.options(entry.path).with_name(entry.name))
            .collect())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) async fn web_options(&self) -> JavaScriptResult<Vec<ScriptOptions>> {
        let manifest_url = self.web_url(SCRIPT_MANIFEST_FILE)?;
        let source = fetch_text(&manifest_url, "script manifest").await?;
        let manifest = parse_script_manifest(&source)?;
        let mut options = Vec::with_capacity(manifest.scripts.len());

        for entry in manifest.scripts {
            let url = self.web_url(&entry.path)?;
            let source = fetch_text(&url, &format!("script `{}`", entry.name)).await?;
            options.push(
                self.options(&entry.path)
                    .with_name(entry.name)
                    .with_source(source)
                    .with_web_url(url)
                    .with_watch(false),
            );
        }

        Ok(options)
    }

    #[cfg(target_arch = "wasm32")]
    fn web_url(&self, file_name: impl AsRef<Path>) -> JavaScriptResult<String> {
        let directory = self.web_directory.as_ref().ok_or_else(|| {
            Error::new_from_js_message("script directory", "a web directory", "none was configured")
        })?;
        let directory = directory.trim_end_matches('/');
        let file_name = file_name.as_ref().to_string_lossy().replace('\\', "/");
        if directory.is_empty() {
            Ok(file_name)
        } else {
            Ok(format!("{directory}/{file_name}"))
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn fetch_text(url: &str, resource: &str) -> JavaScriptResult<String> {
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or_else(|| {
        Error::new_from_js_message(
            "script resource",
            "browser window",
            format!("{resource}: window is unavailable"),
        )
    })?;
    let response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|error| {
            Error::new_from_js_message(
                "script resource",
                "successful request",
                format!("{resource}: {error:?}"),
            )
        })?
        .dyn_into::<web_sys::Response>()
        .map_err(|error| {
            Error::new_from_js_message(
                "script resource",
                "HTTP response",
                format!("{resource}: {error:?}"),
            )
        })?;
    if !response.ok() {
        return Err(Error::new_from_js_message(
            "script resource",
            "successful HTTP response",
            format!("{resource}: {url} returned HTTP {}", response.status()),
        ));
    }
    let text = JsFuture::from(response.text().map_err(|error| {
        Error::new_from_js_message(
            "script resource",
            "readable response",
            format!("{resource}: {error:?}"),
        )
    })?)
    .await
    .map_err(|error| {
        Error::new_from_js_message(
            "script resource",
            "text response",
            format!("{resource}: {error:?}"),
        )
    })?;
    text.as_string().ok_or_else(|| {
        Error::new_from_js_message(
            "script resource",
            "text response",
            format!("{resource}: response was not a string"),
        )
    })
}

/// Describes one JavaScript module source and its optional reload locations.
#[derive(Clone, Debug)]
pub struct ScriptOptions {
    name: Option<String>,
    source: Option<String>,
    native_file: Option<PathBuf>,
    web_url: Option<String>,
    watch: bool,
    poll_interval: Duration,
}

impl ScriptOptions {
    /// Creates options for a named inline module.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            source: None,
            native_file: None,
            web_url: None,
            watch: false,
            poll_interval: Duration::from_millis(250),
        }
    }

    /// Creates options for an inline module.
    pub fn inline(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self::new(name).with_source(source)
    }

    /// Overrides the module identity.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets inline module source.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Sets the native file used as the module source.
    pub fn with_native_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.native_file = Some(path.into());
        self
    }

    /// Sets the web URL used as the module source.
    pub fn with_web_url(mut self, url: impl Into<String>) -> Self {
        self.web_url = Some(url.into());
        self
    }

    /// Enables or disables native file polling for reloads.
    pub fn with_watch(mut self, watch: bool) -> Self {
        self.watch = watch;
        self
    }

    /// Sets the native file polling interval.
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub(crate) fn identity(&self) -> String {
        self.name
            .clone()
            .or_else(|| {
                self.native_file
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .or_else(|| self.web_url.clone())
            .unwrap_or_else(|| "script".to_owned())
    }

    fn module_path(&self) -> String {
        #[cfg(target_arch = "wasm32")]
        let path = self.web_url.clone();
        #[cfg(not(target_arch = "wasm32"))]
        let path = self
            .native_file
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        path.unwrap_or_else(|| self.identity())
    }

    fn load_source(&self) -> JavaScriptResult<String> {
        if let Some(source) = &self.source {
            return Ok(source.clone());
        }
        if let Some(path) = &self.native_file {
            return fs::read_to_string(path).map_err(|error| {
                Error::new_from_js_message("script file", "string", error.to_string())
            });
        }
        Err(Error::new_from_js_message(
            "script source",
            "string",
            "no inline source or native file was provided",
        ))
    }
}

/// Owns a QuickJS runtime and evaluates JavaScript source in its context.
pub struct JavaScriptRuntime {
    context: Context,
    sources: Arc<Mutex<HashMap<String, String>>>,
    _runtime: Runtime,
}

struct ScriptResolver;

fn normalized_module_path(path: &Path) -> String {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized.to_string_lossy().into_owned()
}

impl Resolver for ScriptResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> JavaScriptResult<String> {
        if !name.starts_with("./") && !name.starts_with("../") {
            return Err(Error::new_resolving_message(
                base,
                name,
                "only relative script imports are supported",
            ));
        }
        let path = Path::new(base).parent().unwrap_or_else(|| Path::new(""));
        Ok(normalized_module_path(&path.join(name)))
    }
}

struct ScriptLoader {
    sources: Arc<Mutex<HashMap<String, String>>>,
}

impl Loader for ScriptLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> JavaScriptResult<Module<'js>> {
        #[cfg(not(target_arch = "wasm32"))]
        if Path::new(name).is_absolute() {
            let source = fs::read_to_string(name)
                .map_err(|error| Error::new_loading_message(name, error.to_string()))?;
            return Module::declare(ctx.clone(), name, source);
        }

        let sources = self.sources.lock().expect("script sources lock poisoned");
        let source = sources
            .get(name)
            .ok_or_else(|| Error::new_loading_message(name, "script was not preloaded"))?;
        Module::declare(ctx.clone(), name, source.as_str())
    }
}

impl JavaScriptRuntime {
    /// Creates an isolated JavaScript runtime.
    pub fn new() -> JavaScriptResult<Self> {
        let runtime = Runtime::new()?;
        let sources = Arc::new(Mutex::new(HashMap::new()));
        runtime.set_loader(
            ScriptResolver,
            ScriptLoader {
                sources: Arc::clone(&sources),
            },
        );
        // NOTE this could be like 960kb but maybe we decide this later
        #[cfg(target_arch = "wasm32")]
        runtime.set_max_stack_size(512 * 1024);
        let context = Context::full(&runtime)?;
        Ok(Self {
            context,
            sources,
            _runtime: runtime,
        })
    }

    /// Evaluates a JavaScript program.
    pub fn run(&self, source: &str) -> JavaScriptResult<()> {
        self.with_context(|ctx| ctx.eval::<(), _>(source))
    }

    fn register_sources(&self, options: &[ScriptOptions]) {
        let mut sources = self.sources.lock().expect("script sources lock poisoned");
        for option in options {
            if let Some(source) = &option.source {
                sources.insert(
                    normalized_module_path(Path::new(&option.module_path())),
                    source.clone(),
                );
            }
        }
    }

    /// Formats a JavaScript error, including the pending exception's message and stack trace.
    pub fn format_error(&self, error: &Error) -> String {
        self.with_context(|ctx| {
            if !error.is_exception() {
                return error.to_string();
            }

            let value = ctx.catch();
            if let Some(exception) = value.as_exception() {
                return exception.to_string();
            }
            if let Some(string) = value.as_string().and_then(|value| value.to_string().ok()) {
                return string;
            }
            format!("threw {}: {value:?}", value.type_name())
        })
    }

    fn with_context<F, R>(&self, callback: F) -> R
    where
        F: FnOnce(Ctx<'_>) -> R,
    {
        self.context.with(callback)
    }
}

struct ScriptModule {
    instance: Persistent<Object<'static>>,
    module_path: String,
    native_file: Option<PathBuf>,
    watch: bool,
    update_failed: bool,
    last_modified: Option<FileSystemTime>,
    poll_interval: Duration,
    last_poll: Instant,
}

struct ScriptHostInner {
    modules: HashMap<String, ScriptModule>,
    handles: HashMap<u64, InstanceId>,
    commands: Rc<RefCell<Vec<EngineCommand>>>,
    time: f32,
    last_update_error_module: Option<String>,
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
        let runtime = JavaScriptRuntime::new()?;
        install_instance_bridge(&runtime, Rc::clone(&commands), Rc::clone(&next_handle)).map_err(
            |error| {
                if error.is_exception() {
                    Error::new_from_js_message(
                        "Terrarium JS API",
                        "initialized API",
                        runtime.format_error(&error),
                    )
                } else {
                    error
                }
            },
        )?;

        Ok(Self {
            inner: Rc::new(RefCell::new(ScriptHostInner {
                modules: HashMap::new(),
                handles: HashMap::new(),
                commands,
                time: 0.0,
                last_update_error_module: None,
                runtime,
            })),
        })
    }

    /// Loads and initializes a JavaScript module.
    pub fn load_module(&self, options: ScriptOptions) -> JavaScriptResult<()> {
        let name = options.identity();
        let source = options.load_source()?;
        let mut inner = self.inner.borrow_mut();
        inner
            .runtime
            .register_sources(std::slice::from_ref(&options));
        let module = build_module(&inner.runtime, &options.module_path(), &source)?;
        call_hook(&inner.runtime, &module.instance, "onInit")?;
        call_hook(&inner.runtime, &module.instance, "onLoad")?;
        inner
            .modules
            .insert(name, ScriptModule::from_options(module, options));
        Ok(())
    }

    pub(crate) fn register_sources(&self, options: &[ScriptOptions]) {
        self.inner.borrow().runtime.register_sources(options);
    }

    /// Reloads a module while retaining the previous module when compilation fails.
    pub fn reload_module(&self, name: &str, source: &str) -> JavaScriptResult<()> {
        let mut inner = self.inner.borrow_mut();
        let old = inner
            .modules
            .get(name)
            .ok_or_else(|| Error::new_from_js_message("module", "loaded module", name))?;
        let native_file = old.native_file.clone();
        let module_path = old.module_path.clone();
        let watch = old.watch;
        let last_modified = old.last_modified;
        let poll_interval = old.poll_interval;
        let replacement = build_module(&inner.runtime, &module_path, source)?;

        call_hook(&inner.runtime, &old.instance, "onBeforeReload")?;
        call_hook(&inner.runtime, &old.instance, "onUnload")?;
        call_hook(&inner.runtime, &replacement.instance, "onInit")?;
        call_hook(&inner.runtime, &replacement.instance, "onLoad")?;
        call_hook(&inner.runtime, &replacement.instance, "onReload")?;
        let BuiltModule { instance } = replacement;
        inner.modules.insert(
            name.to_owned(),
            ScriptModule {
                instance,
                module_path,
                native_file,
                watch,
                update_failed: false,
                poll_interval,
                last_modified,
                last_poll: Instant::now(),
            },
        );
        Ok(())
    }

    /// Unloads a module and invokes its unload hook.
    pub fn unload_module(&self, name: &str) -> JavaScriptResult<bool> {
        let mut inner = self.inner.borrow_mut();
        let Some(module) = inner.modules.get(name) else {
            return Ok(false);
        };
        call_hook(&inner.runtime, &module.instance, "onUnload")?;
        inner.modules.remove(name);
        Ok(true)
    }

    /// Stops polling a module's native source file for changes.
    pub fn stop_module_watch(&self, name: &str) -> JavaScriptResult<()> {
        if let Some(module) = self.inner.borrow_mut().modules.get_mut(name) {
            module.watch = false;
        }
        Ok(())
    }

    /// Runs module update hooks and reloads changed native files.
    pub fn update(&self, delta_time: f32) -> JavaScriptResult<()> {
        let mut inner = self.inner.borrow_mut();
        inner.last_update_error_module = None;
        inner.time += delta_time;
        let time = inner.time;
        inner
            .runtime
            .with_context(|ctx| ctx.globals().set("time", time))?;
        let now = Instant::now();
        let changed = inner
            .modules
            .iter_mut()
            .filter_map(|(name, module)| {
                if !module.watch || now.duration_since(module.last_poll) < module.poll_interval {
                    return None;
                }
                module.last_poll = now;
                let path = module.native_file.clone()?;
                let modified = fs::metadata(&path).ok()?.modified().ok()?;
                if module.last_modified == Some(modified) {
                    return None;
                }
                module.last_modified = Some(modified);
                Some((name.clone(), path, modified))
            })
            .collect::<Vec<_>>();

        for (name, path, modified) in changed {
            let result = (|| {
                let source = fs::read_to_string(&path).map_err(|error| {
                    Error::new_from_js_message("script file", "string", error.to_string())
                })?;
                let old = inner.modules.get(&name).expect("changed module exists");
                let native_file = old.native_file.clone();
                let module_path = old.module_path.clone();
                let watch = old.watch;
                let poll_interval = old.poll_interval;
                let replacement = build_module(&inner.runtime, &module_path, &source)?;
                call_hook(&inner.runtime, &old.instance, "onBeforeReload")?;
                call_hook(&inner.runtime, &old.instance, "onUnload")?;
                call_hook(&inner.runtime, &replacement.instance, "onInit")?;
                call_hook(&inner.runtime, &replacement.instance, "onLoad")?;
                call_hook(&inner.runtime, &replacement.instance, "onReload")?;
                let BuiltModule { instance } = replacement;
                inner.modules.insert(
                    name.clone(),
                    ScriptModule {
                        instance,
                        module_path,
                        native_file,
                        watch,
                        update_failed: false,
                        poll_interval,
                        last_modified: Some(modified),
                        last_poll: now,
                    },
                );
                Ok::<_, Error>(())
            })();
            if let Err(error) = result {
                inner.last_update_error_module = Some(name);
                return Err(error);
            }
        }

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
        for command in commands {
            apply_command(&mut inner, workspace, command);
        }
    }
}

struct BuiltModule {
    instance: Persistent<Object<'static>>,
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
            watch: options.watch,
            update_failed: false,
            last_modified,
            poll_interval: options.poll_interval,
            last_poll: Instant::now(),
        }
    }
}

fn install_instance_bridge(
    runtime: &JavaScriptRuntime,
    commands: Rc<RefCell<Vec<EngineCommand>>>,
    next_handle: Rc<Cell<u64>>,
) -> JavaScriptResult<()> {
    runtime.with_context(|ctx| {
        let create_commands = Rc::clone(&commands);
        let create_next_handle = Rc::clone(&next_handle);
        ctx.globals().set(
            "__terrarium_create_instance",
            Function::new(
                ctx.clone(),
                move |type_name: String, options: Option<Object<'_>>| {
                    if type_name != "Part" {
                        return Err(Error::new_from_js_message(
                            "Instance type",
                            "Part",
                            format!("unsupported instance type `{type_name}`"),
                        ));
                    }
                    let spec = parse_part_spec(options)?;
                    let handle = allocate_handle(&create_next_handle);
                    create_commands
                        .borrow_mut()
                        .push(EngineCommand::AddPart { handle, spec });
                    Ok::<_, Error>(handle)
                },
            )?,
        )?;

        install_instance_function(
            &ctx,
            "__terrarium_destroy_instance",
            Rc::clone(&commands),
            EngineCommand::RemoveInstance,
        )?;
        install_instance_vec3_function(
            &ctx,
            "__terrarium_set_position",
            Rc::clone(&commands),
            |handle, value| EngineCommand::SetPosition {
                handle,
                position: value,
            },
            "position",
        )?;
        install_instance_vec3_function(
            &ctx,
            "__terrarium_set_orientation",
            Rc::clone(&commands),
            |handle, value| EngineCommand::SetOrientation {
                handle,
                orientation: value,
            },
            "orientation",
        )?;
        install_instance_vec3_function(
            &ctx,
            "__terrarium_set_size",
            Rc::clone(&commands),
            |handle, value| EngineCommand::SetSize {
                handle,
                size: value,
            },
            "size",
        )?;
        install_instance_vec3_function(
            &ctx,
            "__terrarium_set_color",
            Rc::clone(&commands),
            |handle, value| EngineCommand::SetColor {
                handle,
                color: Color3::new(value.x, value.y, value.z),
            },
            "color",
        )?;

        let name_commands = Rc::clone(&commands);
        ctx.globals().set(
            "__terrarium_set_name",
            Function::new(ctx.clone(), move |handle: u64, name: String| {
                name_commands
                    .borrow_mut()
                    .push(EngineCommand::SetName { handle, name });
                Ok::<_, Error>(())
            })?,
        )?;
        let transparency_commands = Rc::clone(&commands);
        ctx.globals().set(
            "__terrarium_set_transparency",
            Function::new(ctx.clone(), move |handle: u64, value: f32| {
                transparency_commands
                    .borrow_mut()
                    .push(EngineCommand::SetTransparency {
                        handle,
                        transparency: value,
                    });
                Ok::<_, Error>(())
            })?,
        )?;
        let anchored_commands = Rc::clone(&commands);
        ctx.globals().set(
            "__terrarium_set_anchored",
            Function::new(ctx.clone(), move |handle: u64, value: bool| {
                anchored_commands
                    .borrow_mut()
                    .push(EngineCommand::SetAnchored {
                        handle,
                        anchored: value,
                    });
                Ok::<_, Error>(())
            })?,
        )?;
        let collision_commands = Rc::clone(&commands);
        ctx.globals().set(
            "__terrarium_set_can_collide",
            Function::new(ctx.clone(), move |handle: u64, value: bool| {
                collision_commands
                    .borrow_mut()
                    .push(EngineCommand::SetCanCollide {
                        handle,
                        can_collide: value,
                    });
                Ok::<_, Error>(())
            })?,
        )?;
        ctx.globals().set(
            "log",
            Function::new(ctx.clone(), |message: String| {
                log::info!("[javascript] {message}");
                Ok::<_, Error>(())
            })?,
        )?;
        ctx.eval::<(), _>(INSTANCE_API)
    })
}

fn install_instance_function(
    ctx: &Ctx<'_>,
    name: &str,
    commands: Rc<RefCell<Vec<EngineCommand>>>,
    make: impl Fn(u64) -> EngineCommand + 'static,
) -> JavaScriptResult<()> {
    ctx.globals().set(
        name,
        Function::new(ctx.clone(), move |handle: u64| {
            commands.borrow_mut().push(make(handle));
            Ok::<_, Error>(())
        })?,
    )?;
    Ok(())
}

fn install_instance_vec3_function(
    ctx: &Ctx<'_>,
    name: &str,
    commands: Rc<RefCell<Vec<EngineCommand>>>,
    make: impl Fn(u64, Vec3) -> EngineCommand + 'static,
    field: &'static str,
) -> JavaScriptResult<()> {
    ctx.globals().set(
        name,
        Function::new(ctx.clone(), move |handle: u64, values: Vec<f32>| {
            let value = parse_vec3(values, field)?;
            commands.borrow_mut().push(make(handle, value));
            Ok::<_, Error>(())
        })?,
    )?;
    Ok(())
}

fn build_module(
    runtime: &JavaScriptRuntime,
    name: &str,
    source: &str,
) -> JavaScriptResult<BuiltModule> {
    runtime.with_context(|ctx| {
        let module = Module::declare(ctx.clone(), name, source)?;
        let (module, _promise) = module.eval()?;
        while ctx.execute_pending_job() {}
        let namespace = module.namespace()?;
        let instance = construct_class(&ctx, &namespace, source, "default", &mut HashMap::new())?;
        Ok(BuiltModule {
            instance: Persistent::save(&ctx, instance),
        })
    })
}

fn construct_class<'js>(
    ctx: &Ctx<'js>,
    namespace: &Object<'js>,
    source: &str,
    class_name: &str,
    cache: &mut HashMap<String, Object<'js>>,
) -> JavaScriptResult<Object<'js>> {
    if let Some(instance) = cache.get(class_name) {
        return Ok(instance.clone());
    }
    let constructor: Function = namespace
        .get(class_name)
        .map_err(|_| Error::new_from_js_message("constructor", "exported class", class_name))?;
    let dependencies = constructor_dependencies(source, class_name);
    let mut values = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        values.push(construct_class(ctx, namespace, source, &dependency, cache)?);
    }
    let construct: Function = ctx.globals().get("__terrarium_construct")?;
    let instance: Object = construct.call((constructor, values))?;
    cache.insert(class_name.to_owned(), instance.clone());
    Ok(instance)
}

fn constructor_dependencies(source: &str, class_name: &str) -> Vec<String> {
    let class_start = if class_name == "default" {
        source.find("export default class")
    } else {
        source.find(&format!("class {class_name}"))
    };
    let Some(class_start) = class_start else {
        return Vec::new();
    };
    let rest = &source[class_start..];
    let Some(constructor_start) = rest.find("constructor(") else {
        return Vec::new();
    };
    let before = &rest[..constructor_start];
    let Some(comment_start) = before.rfind("/**") else {
        return Vec::new();
    };
    let comment = &before[comment_start..];
    comment
        .lines()
        .filter_map(|line| {
            let marker = line.find("@param")?;
            let annotation = line[marker + "@param".len()..].trim();
            let start = annotation.find('{')? + 1;
            let end = annotation[start..].find('}')? + start;
            let dependency = annotation[start..end].trim();
            (!dependency.is_empty() && dependency != "Object" && dependency != "string")
                .then(|| dependency.to_owned())
        })
        .collect()
}

fn call_hook(
    runtime: &JavaScriptRuntime,
    persistent: &Persistent<Object<'static>>,
    name: &str,
) -> JavaScriptResult<()> {
    runtime.with_context(|ctx| {
        let instance = persistent.clone().restore(&ctx)?;
        let Some(method) = instance.get::<_, Option<Function>>(name)? else {
            return Ok(());
        };
        let mut args = Args::new(ctx.clone(), 0);
        args.this(instance)?;
        let _: () = args.apply(&method)?;
        Ok(())
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

fn allocate_handle(next_handle: &Cell<u64>) -> u64 {
    let handle = next_handle.get();
    next_handle.set(handle.wrapping_add(1).max(1));
    handle
}

fn parse_vec3(values: Vec<f32>, field: &'static str) -> JavaScriptResult<Vec3> {
    match values.as_slice() {
        [x, y, z] => Ok(Vec3::new(*x, *y, *z)),
        _ => Err(Error::new_from_js_message(
            field,
            "array",
            "expected three numbers",
        )),
    }
}

#[derive(Clone)]
struct PartSpec {
    name: Option<String>,
    shape: PartShape,
    position: Vec3,
    size: Vec3,
    color: Color3,
    transparency: f32,
    anchored: bool,
    can_collide: bool,
}

fn parse_part_spec(options: Option<Object<'_>>) -> JavaScriptResult<PartSpec> {
    let Some(options) = options else {
        return Ok(PartSpec {
            name: None,
            shape: PartShape::Block,
            position: Vec3::ZERO,
            size: Vec3::ONE,
            color: Color3::WHITE,
            transparency: 0.0,
            anchored: true,
            can_collide: true,
        });
    };
    let shape = match options.get::<_, Option<String>>("shape")?.as_deref() {
        Some("ball") => PartShape::Ball,
        Some("cylinder") => PartShape::Cylinder,
        Some("wedge") => PartShape::Wedge,
        Some("cornerWedge") => PartShape::CornerWedge,
        Some("block") | None => PartShape::Block,
        Some(value) => return Err(Error::new_from_js_message("shape", "known shape", value)),
    };
    Ok(PartSpec {
        name: options.get("name")?,
        shape,
        position: parse_optional_vec3(options.get("position")?, Vec3::ZERO, "position")?,
        size: parse_optional_vec3(options.get("size")?, Vec3::ONE, "size")?,
        color: options
            .get::<_, Option<Vec<f32>>>("color")?
            .map(|value| parse_vec3(value, "color"))
            .transpose()?
            .map_or(Color3::WHITE, |value| {
                Color3::new(value.x, value.y, value.z)
            }),
        transparency: options
            .get::<_, Option<f32>>("transparency")?
            .unwrap_or(0.0),
        anchored: options.get::<_, Option<bool>>("anchored")?.unwrap_or(true),
        can_collide: options
            .get::<_, Option<bool>>("canCollide")?
            .unwrap_or(true),
    })
}

fn parse_optional_vec3(
    value: Option<Vec<f32>>,
    default: Vec3,
    field: &'static str,
) -> JavaScriptResult<Vec3> {
    value.map_or(Ok(default), |value| parse_vec3(value, field))
}

enum EngineCommand {
    AddPart { handle: u64, spec: PartSpec },
    RemoveInstance(u64),
    SetPosition { handle: u64, position: Vec3 },
    SetOrientation { handle: u64, orientation: Vec3 },
    SetSize { handle: u64, size: Vec3 },
    SetColor { handle: u64, color: Color3 },
    SetName { handle: u64, name: String },
    SetTransparency { handle: u64, transparency: f32 },
    SetAnchored { handle: u64, anchored: bool },
    SetCanCollide { handle: u64, can_collide: bool },
}

fn apply_command(inner: &mut ScriptHostInner, workspace: &mut Workspace, command: EngineCommand) {
    match command {
        EngineCommand::AddPart { handle, spec } => {
            let mut part = Part::new()
                .with_shape(spec.shape)
                .with_position(spec.position)
                .with_size(spec.size)
                .with_color(spec.color)
                .with_transparency(spec.transparency)
                .with_anchored(spec.anchored)
                .with_can_collide(spec.can_collide);
            if let Some(name) = spec.name {
                part.set_name(name);
            }
            let id = workspace.add_child(part);
            inner.handles.insert(handle, id);
        }
        EngineCommand::RemoveInstance(handle) => {
            if let Some(id) = inner.handles.remove(&handle) {
                workspace.remove_child(id);
            }
        }
        EngineCommand::SetPosition { handle, position } => {
            if let Some(part) = part_for_handle(inner, workspace, handle) {
                *part = std::mem::take(part).with_position(position);
            }
        }
        EngineCommand::SetOrientation {
            handle,
            orientation,
        } => {
            if let Some(part) = part_for_handle(inner, workspace, handle) {
                *part = std::mem::take(part).with_orientation(orientation);
            }
        }
        EngineCommand::SetSize { handle, size } => {
            if let Some(part) = part_for_handle(inner, workspace, handle) {
                *part = std::mem::take(part).with_size(size);
            }
        }
        EngineCommand::SetColor { handle, color } => {
            if let Some(part) = part_for_handle(inner, workspace, handle) {
                *part = std::mem::take(part).with_color(color);
            }
        }
        EngineCommand::SetName { handle, name } => {
            if let Some(part) = part_for_handle(inner, workspace, handle) {
                part.set_name(name);
            }
        }
        EngineCommand::SetTransparency {
            handle,
            transparency,
        } => {
            if let Some(part) = part_for_handle(inner, workspace, handle) {
                *part = std::mem::take(part).with_transparency(transparency);
            }
        }
        EngineCommand::SetAnchored { handle, anchored } => {
            if let Some(part) = part_for_handle(inner, workspace, handle) {
                *part = std::mem::take(part).with_anchored(anchored);
            }
        }
        EngineCommand::SetCanCollide {
            handle,
            can_collide,
        } => {
            if let Some(part) = part_for_handle(inner, workspace, handle) {
                *part = std::mem::take(part).with_can_collide(can_collide);
            }
        }
    }
}

fn part_for_handle<'a>(
    inner: &ScriptHostInner,
    workspace: &'a mut Workspace,
    handle: u64,
) -> Option<&'a mut Part> {
    let id = inner.handles.get(&handle).copied()?;
    workspace.get_mut::<Part>(id)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            ScriptDirectories::new()
                .with_native_directory(&scripts)
                .native_options()
                .unwrap()
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
}
