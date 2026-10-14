use tregui::egui;

pub fn run(
    title: &'static str,
    install_fonts: impl FnOnce(&mut egui::FontDefinitions) + 'static,
    draw: impl FnMut(&mut egui::Ui) + 'static,
) -> Result<(), trengine::AppCreationError> {
    trengine::Terrarium::new()
        .with_title(title)
        .run(move |engine| {
            let mut fonts = egui::FontDefinitions::default();
            install_fonts(&mut fonts);
            engine.egui_context().set_fonts(fonts);
            Ok::<_, trengine::AppCreationError>(Showcase {
                title,
                draw: Box::new(draw),
            })
        })
        .map(|_| ())
}

struct Showcase {
    title: &'static str,
    draw: Box<dyn FnMut(&mut egui::Ui)>,
}

impl trengine::App for Showcase {
    fn after_update(
        &mut self,
        engine: &mut trengine::Engine,
        input: &trengine::trscene::FrameInput,
        _frame: &mut trengine::Frame,
    ) {
        egui::Window::new(self.title)
            .default_pos(egui::pos2(48.0, 48.0))
            .default_size(egui::vec2(input.size[0] - 96.0, input.size[1] - 96.0))
            .show(&engine.egui_context(), |ui| (self.draw)(ui));
    }
}
