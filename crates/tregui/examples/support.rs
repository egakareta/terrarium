use eframe::egui;

pub fn run(
    _title: &'static str,
    _install_fonts: impl FnOnce(&mut egui::FontDefinitions) + 'static,
    _draw: impl FnMut(&mut egui::Ui) + 'static,
) -> eframe::Result<()> {
    #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
    {
        Ok(())
    }

    #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
    {
        eframe::run_native(
            _title,
            eframe::NativeOptions::default(),
            Box::new(move |creation_context| {
                let mut fonts = egui::FontDefinitions::default();
                _install_fonts(&mut fonts);
                creation_context.egui_ctx.set_fonts(fonts);
                Ok(Box::new(Showcase {
                    draw: Box::new(_draw),
                }))
            }),
        )
    }
}

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
struct Showcase {
    draw: Box<dyn FnMut(&mut egui::Ui)>,
}

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
impl eframe::App for Showcase {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        (self.draw)(ui);
    }
}
