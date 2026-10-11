use std::collections::HashMap;

use trscene::{Instance, InstanceId, Workspace};

use super::{Property, PropertyError, PropertyValue};

/// Properties shared by the current selection.
#[derive(Clone, Debug)]
pub struct SelectionProperty {
    /// Field descriptor from the first selected instance.
    pub property: Property,
    /// Whether selected instances have different values for this field.
    pub mixed: bool,
}

/// A floating workspace inspector with a persistent selection.
///
/// Engines create one automatically with the `devtools` feature enabled. The
/// window starts hidden; F12 toggles it. Ctrl-click (Command-click on macOS)
/// toggles rows, Shift-click selects a visible range, and Delete destroys the
/// selection while the inspector is active and no text editor has focus.
#[derive(Default)]
pub struct Devtools {
    visible: bool,
    selected: Vec<InstanceId>,
    anchor: Option<InstanceId>,
    active: bool,
    last_input_frame: Option<u64>,
    error: Option<String>,
    json_drafts: HashMap<String, String>,
}

impl Devtools {
    /// Returns whether the inspector window is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Shows or hides the inspector without changing its selection.
    pub fn set_visible(&mut self, visible: bool) {
        if visible != self.visible {
            self.active = visible;
        }
        self.visible = visible;
        if !visible {
            self.active = false;
        }
    }

    /// Returns selected instance IDs in selection order.
    pub fn selected(&self) -> &[InstanceId] {
        &self.selected
    }

    /// Selects a row using the same modifiers as the workspace tree.
    ///
    /// `visible_order` lists expanded rows from top to bottom. Shift selects
    /// the inclusive range from the anchor; Ctrl/Command+Shift adds that range
    /// to the selection. If the anchor is collapsed, Shift selects only `id`.
    pub fn select(
        &mut self,
        id: InstanceId,
        modifiers: egui::Modifiers,
        visible_order: &[InstanceId],
    ) {
        let additive = modifiers.ctrl || modifiers.command;
        if modifiers.shift {
            let range = self.anchor.and_then(|anchor| {
                Some((
                    visible_order.iter().position(|row| *row == anchor)?,
                    visible_order.iter().position(|row| *row == id)?,
                ))
            });
            if !additive {
                self.selected.clear();
            }
            if let Some((a, b)) = range {
                for &row in &visible_order[a.min(b)..=a.max(b)] {
                    if !self.selected.contains(&row) {
                        self.selected.push(row);
                    }
                }
            } else if !self.selected.contains(&id) {
                self.selected.push(id);
            }
        } else {
            if additive {
                if let Some(index) = self.selected.iter().position(|row| *row == id) {
                    self.selected.remove(index);
                } else {
                    self.selected.push(id);
                }
            } else {
                self.selected = vec![id];
            }
            self.anchor = Some(id);
        }
        self.active = true;
        self.error = None;
        self.json_drafts.clear();
    }

    /// Returns properties common to every selected instance, marking mixed values.
    pub fn properties(&self, workspace: &Workspace) -> Vec<SelectionProperty> {
        let mut instances = self
            .selected
            .iter()
            .filter_map(|&id| resolve(workspace, id));
        let Some(first) = instances.next() else {
            return Vec::new();
        };
        let mut properties: Vec<_> = first
            .properties()
            .into_iter()
            .map(|property| SelectionProperty {
                property,
                mixed: false,
            })
            .collect();
        for instance in instances {
            let other = instance.properties();
            properties.retain_mut(|field| {
                let Some(value) = other.iter().find(|property| {
                    property.path == field.property.path
                        && compatible(&property.value, &field.property.value)
                }) else {
                    return false;
                };
                field.mixed |= value.value != field.property.value;
                field.property.editable &= value.editable;
                true
            });
        }
        properties
    }

    /// Applies an edit to a common editable field of the selection.
    pub fn edit_property(
        &mut self,
        workspace: &mut Workspace,
        path: &str,
        value: PropertyValue,
    ) -> Result<(), PropertyError> {
        self.prune(workspace);
        let Some(property) = self
            .properties(workspace)
            .into_iter()
            .find(|property| property.property.path == path && property.property.editable)
        else {
            return Err(PropertyError::NotFound);
        };
        if !compatible(&property.property.value, &value) {
            return Err(PropertyError::InvalidValue);
        }
        for &id in &self.selected {
            resolve(workspace, id)
                .ok_or(PropertyError::NotFound)?
                .validate_property(path, &value)?;
        }
        for &id in &self.selected {
            resolve_mut(workspace, id)
                .ok_or(PropertyError::NotFound)?
                .set_property(path, value.clone())?;
        }
        Ok(())
    }

    /// Destroys selected instances and their descendants; the workspace root is retained.
    pub fn destroy_selection(&mut self, workspace: &mut Workspace) {
        for id in self.selected.clone() {
            if let Some(instance) = workspace.instance_mut(id) {
                instance.destroy();
            }
        }
        self.prune(workspace);
        self.error = None;
    }

    /// Processes F12 and draws the inspector into the current egui frame.
    pub fn show(&mut self, context: &egui::Context, workspace: &mut Workspace) {
        let frame = context.cumulative_frame_nr();
        let process_input = self.last_input_frame != Some(frame);
        if process_input {
            self.last_input_frame = Some(frame);
            if pressed(context, egui::Key::F12) {
                context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F12));
                self.set_visible(!self.visible);
            }
        }
        self.prune(workspace);
        if !self.visible {
            show_development_notice(context);
            return;
        }
        let id = egui::Id::new(("terrarium_devtools", workspace.id()));
        let mut open = true;
        let mut click = None;
        let mut order = Vec::new();
        let mut delete_clicked = false;
        let window = egui::Window::new("DevTools")
            .id(id)
            .open(&mut open)
            .default_size([660.0, 500.0])
            .resizable(true)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("{} selected", self.selected.len()));
                    delete_clicked = ui
                        .add_enabled(
                            self.selected.iter().any(|&id| id != workspace.id()),
                            egui::Button::new("Delete selection"),
                        )
                        .clicked();
                });
                ui.small("Ctrl/⌘-click: toggle · Shift-click: range · Delete: destroy · F12: hide");
                ui.separator();
                let tree_height = ui.available_height();
                egui::ScrollArea::vertical()
                    .id_salt("tree")
                    .auto_shrink([false, false])
                    .max_height(tree_height)
                    .show(ui, |ui| {
                        tree(ui, workspace, &self.selected, &mut order, &mut click, true);
                    });
            });
        let properties_window = egui::Window::new("Properties")
            .id(egui::Id::new((
                "terrarium_devtools_properties",
                workspace.id(),
            )))
            .default_pos([680.0, 50.0])
            .default_size([420.0, 500.0])
            .resizable(true)
            .show(context, |ui| {
                if self.selected.is_empty() {
                    ui.label("Select an instance to inspect its properties.");
                }
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                let properties_height = ui.available_height();
                egui::ScrollArea::vertical()
                    .id_salt("properties")
                    .auto_shrink([false, false])
                    .max_height(properties_height)
                    .show(ui, |ui| {
                        for field in self.properties(workspace) {
                            let mut value = field.property.value.clone();
                            if let PropertyValue::Json(json) = &mut value
                                && let Some(draft) = self.json_drafts.get(&field.property.path)
                            {
                                json.clone_from(draft);
                            }
                            let changed = ui
                                .push_id(&field.property.path, |ui| {
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(&field.property.path);
                                            if field.mixed {
                                                ui.weak("mixed");
                                            }
                                        });
                                        ui.add_enabled_ui(field.property.editable, |ui| {
                                            edit_value(ui, &mut value)
                                        })
                                        .inner
                                    })
                                    .inner
                                })
                                .inner;
                            if changed {
                                if let PropertyValue::Json(json) = &value {
                                    self.json_drafts
                                        .insert(field.property.path.clone(), json.clone());
                                }
                                self.error = self
                                    .edit_property(workspace, &field.property.path, value)
                                    .err()
                                    .map(|error| error.to_string());
                                if self.error.is_none() {
                                    self.json_drafts.remove(&field.property.path);
                                }
                            }
                        }
                    });
            });
        if let Some((id, modifiers)) = click {
            self.select(id, modifiers, &order);
        }
        if context.input(|input| input.pointer.any_pressed()) {
            self.active = context
                .input(|input| input.pointer.interact_pos())
                .is_some_and(|position| {
                    window.iter().chain(properties_window.iter()).any(|window| {
                        window.response.rect.contains(position)
                            && context.layer_id_at(position) == Some(window.response.layer_id)
                    })
                });
        }
        if delete_clicked
            || (process_input
                && self.active
                && !context.text_edit_focused()
                && pressed(context, egui::Key::Delete))
        {
            context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Delete));
            self.destroy_selection(workspace);
        }
        self.set_visible(open);
        show_development_notice(context);
    }

    fn prune(&mut self, workspace: &Workspace) {
        self.selected.retain(|&id| resolve(workspace, id).is_some());
        if self
            .anchor
            .is_some_and(|id| resolve(workspace, id).is_none())
        {
            self.anchor = None;
        }
    }
}

fn show_development_notice(context: &egui::Context) {
    egui::Area::new(egui::Id::new("terrarium_development_notice"))
        .order(egui::Order::Foreground)
        .interactable(false)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -8.0))
        .show(context, |ui| {
            ui.colored_label(
                egui::Color32::RED,
                "Development build: Press F12 to enable DevTools",
            );
        });
}

fn pressed(context: &egui::Context, key: egui::Key) -> bool {
    context.input(|input| {
        input.events.iter().any(|event| {
            matches!(event, egui::Event::Key { key: event_key, pressed: true, repeat: false, .. } if *event_key == key)
        })
    })
}

fn resolve(workspace: &Workspace, id: InstanceId) -> Option<&dyn Instance> {
    if id == workspace.id() {
        Some(workspace)
    } else {
        workspace.instance(id)
    }
}

fn resolve_mut(workspace: &mut Workspace, id: InstanceId) -> Option<&mut dyn Instance> {
    if id == workspace.id() {
        Some(workspace)
    } else {
        workspace.instance_mut(id)
    }
}

fn compatible(a: &PropertyValue, b: &PropertyValue) -> bool {
    match (a, b) {
        (PropertyValue::Choice { options: a, .. }, PropertyValue::Choice { options: b, .. }) => {
            a == b
        }
        (PropertyValue::Vector(a), PropertyValue::Vector(b)) => a.len() == b.len(),
        _ => std::mem::discriminant(a) == std::mem::discriminant(b),
    }
}

fn tree(
    ui: &mut egui::Ui,
    instance: &dyn Instance,
    selection: &[InstanceId],
    order: &mut Vec<InstanceId>,
    click: &mut Option<(InstanceId, egui::Modifiers)>,
    root: bool,
) {
    order.push(instance.id());
    let label = format!("{} ({})", instance.name(), instance.class_name());
    let id = ui.make_persistent_id(instance.id());
    let mut row = |ui: &mut egui::Ui| {
        let response = ui.selectable_label(selection.contains(&instance.id()), &label);
        if response.clicked() {
            response.surrender_focus();
            *click = Some((instance.id(), ui.input(|input| input.modifiers)));
        }
    };
    if instance.children().is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(ui.spacing().indent);
            row(ui);
        });
    } else {
        let header =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, root)
                .show_header(ui, row);
        let open = header.is_open();
        header.body(|ui| {
            if open {
                for child in instance.children() {
                    tree(ui, child.as_ref(), selection, order, click, false);
                }
            }
        });
    }
}

fn edit_value(ui: &mut egui::Ui, value: &mut PropertyValue) -> bool {
    match value {
        PropertyValue::Bool(value) => ui.checkbox(value, "").changed(),
        PropertyValue::Number(value) => ui.add(egui::DragValue::new(value).speed(0.05)).changed(),
        PropertyValue::Text(value) => ui.text_edit_singleline(value).changed(),
        PropertyValue::Json(value) => ui.text_edit_multiline(value).changed(),
        PropertyValue::Color(value) => ui.color_edit_button_rgb(value).changed(),
        PropertyValue::Vector(values) => {
            ui.horizontal(|ui| {
                let mut changed = false;
                for (index, value) in values.iter_mut().enumerate() {
                    changed |= ui
                        .push_id(index, |ui| {
                            ui.add(egui::DragValue::new(value).speed(0.05)).changed()
                        })
                        .inner;
                }
                changed
            })
            .inner
        }
        PropertyValue::Choice { selected, options } => {
            let mut changed = false;
            egui::ComboBox::from_id_salt("choice")
                .selected_text(selected.as_str())
                .show_ui(ui, |ui| {
                    for option in options {
                        changed |= ui
                            .selectable_value(selected, option.clone(), option.as_str())
                            .changed();
                    }
                });
            changed
        }
    }
}
