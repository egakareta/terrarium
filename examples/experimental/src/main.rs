use terrarium::{App, AppCreationError, Engine, Terrarium, egui};

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
        .with_scripts(terrarium::scripts!("scripts"))
        .run(initialize)
        .unwrap();
}

#[test]
fn app_loads() -> Result<(), terrarium::AppCreationError> {
    use terrarium::{Instance, Part};

    Terrarium::new()
        .with_size([32, 32])
        .with_headless(Some(1))
        .with_scripts(terrarium::scripts!("scripts"))
        .run(|engine| {
            let app = initialize(engine)?;
            assert!(engine.workspace.get_all::<Part>().any(|part| part.name() == "Orb"));
            Ok::<_, AppCreationError>(app)
        })?;
    Ok(())
}
