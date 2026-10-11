use eframe::egui;

pub fn run(
    title: &'static str,
    install_fonts: impl FnOnce(&mut egui::FontDefinitions) + 'static,
    draw: impl FnMut(&mut egui::Ui) + 'static,
) -> eframe::Result<()> {
    eframe::run_native(
        title,
        eframe::NativeOptions::default(),
        Box::new(move |creation_context| {
            let mut fonts = egui::FontDefinitions::default();
            install_fonts(&mut fonts);
            creation_context.egui_ctx.set_fonts(fonts);
            Ok(Box::new(Showcase {
                draw: Box::new(draw),
            }))
        }),
    )
}

struct Showcase {
    draw: Box<dyn FnMut(&mut egui::Ui)>,
}

impl eframe::App for Showcase {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        (self.draw)(ui);
    }
}
