use terrarium::{
    App, AppBuilder, AppCreationError, Engine, eframe, egui,
    egui::{FontId, RichText},
};

struct CloseAfterFrames {
    completed: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    limit: usize,
}

impl App for CloseAfterFrames {
    fn ui(&mut self, _engine: &mut Engine, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.label(RichText::new("i love headless tests").font(FontId::proportional(40.0)));
    }

    fn after_update(
        &mut self,
        _engine: &mut Engine,
        context: &egui::Context,
        _frame: &mut eframe::Frame,
    ) {
        use std::sync::atomic::Ordering;

        let completed = self.completed.fetch_add(1, Ordering::Relaxed) + 1;
        if completed == self.limit {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn main() {
    AppBuilder::new()
        .run(
            |_creation_context, _engine| -> Result<CloseAfterFrames, AppCreationError> {
                Ok(CloseAfterFrames {
                    completed: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                    limit: 30,
                })
            },
        )
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_runs_until_close_is_requested() -> Result<(), AppCreationError> {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let completed = Arc::new(AtomicUsize::new(0));
        let app_completed = Arc::clone(&completed);
        AppBuilder::new()
            .with_size([32, 32])
            .with_env_logger(false)
            .with_headless(None)
            .run(move |_creation_context, _engine| {
                Ok::<_, AppCreationError>(CloseAfterFrames {
                    completed: app_completed,
                    limit: 3,
                })
            })?;

        assert_eq!(completed.load(Ordering::Relaxed), 3);
        Ok(())
    }
}
