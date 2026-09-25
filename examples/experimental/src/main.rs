use std::{path::Path, sync::LazyLock};

use terrarium::{App, AppCreationError, Engine, ScriptDirectories, Terrarium, egui};

static SCRIPT_DIRECTORIES: LazyLock<ScriptDirectories> = LazyLock::new(|| {
    ScriptDirectories::new()
        .with_native_directory(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts"))
        .with_web_directory("./scripts")
});

struct GameApp;

impl App for GameApp {
    fn ui(
        &mut self,
        _engine: &mut Engine,
        ui: &mut egui::Ui,
        _frame: &mut terrarium::eframe::Frame,
    ) {
        ui.horizontal(|ui| {
            ui.label("I love egui!");
        });
    }
}

fn initialize(engine: &mut Engine) -> Result<GameApp, AppCreationError> {
    engine.with_clear_color([0.025, 0.035, 0.06, 1.0]);

    Ok(GameApp)
}

fn main() {
    Terrarium::new()
        .with_scripts_dir(SCRIPT_DIRECTORIES.clone())
        .run(initialize)
        .unwrap();
}

#[test]
fn app_loads() -> Result<(), terrarium::AppCreationError> {
    Terrarium::new()
        .with_size([32, 32])
        .with_headless(Some(1))
        .with_scripts_dir(SCRIPT_DIRECTORIES.clone())
        .run(initialize)?;
    Ok(())
}
