//! Displays ticks from an integrated server or an optional remote WebSocket server.

use terrarium::*;

#[derive(Default)]
struct NetworkApp {
    #[cfg(feature = "network")]
    address: String,
    #[cfg(feature = "network")]
    error: Option<String>,
}

impl App for NetworkApp {
    fn ui(&mut self, engine: &mut Engine, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("network").show(ui, |ui| {
            ui.heading("Server tick");
            let client = engine.client();
            ui.label(if client.is_integrated() {
                "Integrated server"
            } else {
                "Remote server"
            });
            ui.label(format!("Status: {:?}", client.status()));
            match client.tick() {
                Some(tick) => ui.label(format!("Tick: {tick}")),
                None => ui.label("Waiting for the first tick"),
            };
            if ui.button("Use integrated server").clicked() {
                engine.set_client(Client::default());
            }
            #[cfg(feature = "network")]
            {
                ui.separator();
                ui.text_edit_singleline(&mut self.address);
                if ui.button("Connect").clicked() {
                    match Client::connect(self.address.clone()) {
                        Ok(client) => {
                            engine.set_client(client);
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                if let Some(error) = &self.error {
                    ui.label(error);
                }
            }
            #[cfg(not(feature = "network"))]
            ui.label("Enable the network feature to connect to a remote server.");
        });
    }
}

fn main() {
    Terrarium::new()
        .with_title("Server tick")
        .run(|engine| {
            engine
                .workspace_mut()
                .add_child(Part::new().with_size(glam::Vec3::splat(2.0)));
            Ok::<_, AppCreationError>(NetworkApp {
                #[cfg(feature = "network")]
                address: "ws://127.0.0.1:9001".to_owned(),
                ..Default::default()
            })
        })
        .unwrap();
}
