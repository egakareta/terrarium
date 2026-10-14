use std::collections::HashMap;

use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};
use egui_ltreeview::{Action, NodeBuilder, TreeView, TreeViewBuilder, TreeViewState};
use egui_phosphor::regular;
use trscene::{Instance, InstanceId, Workspace};

use super::{Property, PropertyError, PropertyValue};
use crate::font_market::FontMarket;

/// Properties shared by the current selection.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SelectionProperty {
    /// Field descriptor from the first selected instance.
    pub property: Property,
    /// Whether selected instances have different values for this field.
    pub mixed: bool,
}

/// A dockable workspace inspector with a persistent selection.
///
/// Engines create one automatically with the `devtools` feature enabled. The
/// window starts hidden; F12 toggles it. Ctrl-click (Command-click on macOS)
/// toggles rows, Shift-click selects a visible range, and Delete destroys the
/// selection while the inspector is active and no text editor has focus. The
/// Explorer and Properties panes can be moved, split, or detached into windows.
pub struct Devtools {
    visible: bool,
    selected: Vec<InstanceId>,
    anchor: Option<InstanceId>,
    active: bool,
    last_input_frame: Option<u64>,
    error: Option<String>,
    json_drafts: HashMap<String, String>,
    font_market: FontMarket,
    base_font_definitions: Option<egui::FontDefinitions>,
    dock_state: DockState<DevtoolsTab>,
    tree_state: TreeViewState<InstanceId>,
}

impl Default for Devtools {
    fn default() -> Self {
        Self {
            visible: false,
            selected: Vec::new(),
            anchor: None,
            active: false,
            last_input_frame: None,
            error: None,
            json_drafts: HashMap::new(),
            font_market: FontMarket::default(),
            base_font_definitions: None,
            dock_state: default_dock_state(),
            tree_state: TreeViewState::default(),
        }
    }
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

    /// Sets the application's starting fonts used when Font Market applies a downloaded font.
    pub fn set_base_font_definitions(&mut self, fonts: egui::FontDefinitions) {
        self.base_font_definitions = Some(fonts);
    }

    /// Selects a row using the same modifiers as the workspace tree.
    ///
    /// `visible_order` lists expanded rows from top to bottom.
    pub fn select(
        &mut self,
        id: InstanceId,
        modifiers: trscene::Modifiers,
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
        properties_for_selection(&self.selected, workspace)
    }

    /// Applies an edit to a common editable field of the selection.
    pub fn edit_property(
        &mut self,
        workspace: &mut Workspace,
        path: &str,
        value: PropertyValue,
    ) -> Result<(), PropertyError> {
        self.prune(workspace);
        edit_selection_property(workspace, &self.selected, path, value)
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

    /// Processes inspector shortcuts for hosts that do not draw an egui frame.
    pub fn process_input(&mut self, input: &trscene::FrameInput, workspace: &mut Workspace) {
        if input.pressed.iter().any(|key| key == "F12") {
            self.set_visible(!self.visible);
        }
        self.prune(workspace);
        if self.visible
            && self.active
            && !input.keyboard_captured
            && input.pressed.iter().any(|key| key == "Delete")
        {
            self.destroy_selection(workspace);
        }
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
        let dock_id = egui::Id::new(("terrarium_devtools_dock", workspace.id()));
        let mut open = true;
        let mut delete_clicked = false;
        let mut tree_selection_changed = false;
        let mut panel_layers = Vec::new();
        let window = egui::Window::new("DevTools")
            .id(id)
            .open(&mut open)
            .default_pos([48.0, 48.0])
            .resizable(true)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.menu_button(format!("{} Panels", regular::SLIDERS_HORIZONTAL), |ui| {
                        self.show_panel_toggle(ui, DevtoolsTab::Explorer);
                        self.show_panel_toggle(ui, DevtoolsTab::Properties);
                        self.show_panel_toggle(ui, DevtoolsTab::FontMarket);
                    });
                    if ui
                        .button(format!("{} Reset layout", regular::ARROW_COUNTER_CLOCKWISE))
                        .clicked()
                    {
                        self.dock_state = default_dock_state();
                    }
                    ui.separator();
                    ui.label(format!("{} selected", self.selected.len()));
                    delete_clicked = ui
                        .add_enabled(
                            self.selected.iter().any(|&id| id != workspace.id()),
                            egui::Button::new(format!("{} Delete selection", regular::TRASH)),
                        )
                        .clicked();
                });
                ui.separator();
                let mut viewer = DevtoolsTabViewer {
                    workspace,
                    selected: &mut self.selected,
                    anchor: &mut self.anchor,
                    tree_state: &mut self.tree_state,
                    selection_changed: &mut tree_selection_changed,
                    error: &mut self.error,
                    json_drafts: &mut self.json_drafts,
                    font_market: &mut self.font_market,
                    base_font_definitions: self.base_font_definitions.as_ref(),
                    context,
                    panel_layers: &mut panel_layers,
                };
                DockArea::new(&mut self.dock_state)
                    .id(dock_id)
                    .style(Style::from_egui(ui.style().as_ref()))
                    .show_inside(ui, &mut viewer);
            });
        if context.input(|input| input.pointer.any_pressed()) {
            self.active = tree_selection_changed
                || context
                    .input(|input| input.pointer.interact_pos())
                    .is_some_and(|position| {
                        let layer = context.layer_id_at(position);
                        window.as_ref().is_some_and(|window| {
                            window.response.rect.contains(position)
                                && layer == Some(window.response.layer_id)
                        }) || layer.is_some_and(|layer| panel_layers.contains(&layer))
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

    fn show_panel_toggle(&mut self, ui: &mut egui::Ui, tab: DevtoolsTab) {
        let mut is_open = self.dock_state.find_tab(&tab).is_some();
        if ui.checkbox(&mut is_open, tab.title()).changed() {
            if is_open {
                self.dock_state.main_surface_mut().push_to_focused_leaf(tab);
            } else if let Some(path) = self.dock_state.find_tab(&tab) {
                self.dock_state.remove_tab(path);
            }
        }
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum DevtoolsTab {
    Explorer,
    Properties,
    FontMarket,
}

impl DevtoolsTab {
    fn title(self) -> String {
        match self {
            Self::Explorer => format!("{} Explorer", regular::TREE_STRUCTURE),
            Self::Properties => format!("{} Properties", regular::GEAR),
            Self::FontMarket => format!("{} Font Market", regular::TEXT_T),
        }
    }
}

struct DevtoolsTabViewer<'a> {
    workspace: &'a mut Workspace,
    selected: &'a mut Vec<InstanceId>,
    anchor: &'a mut Option<InstanceId>,
    tree_state: &'a mut TreeViewState<InstanceId>,
    selection_changed: &'a mut bool,
    error: &'a mut Option<String>,
    json_drafts: &'a mut HashMap<String, String>,
    font_market: &'a mut FontMarket,
    base_font_definitions: Option<&'a egui::FontDefinitions>,
    context: &'a egui::Context,
    panel_layers: &'a mut Vec<egui::LayerId>,
}

impl TabViewer for DevtoolsTabViewer<'_> {
    type Tab = DevtoolsTab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn on_tab_button(&mut self, tab: &mut Self::Tab, response: &egui::Response) {
        let label = match tab {
            DevtoolsTab::Explorer => "Explorer",
            DevtoolsTab::Properties => "Properties",
            DevtoolsTab::FontMarket => "Font Market",
        };
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        self.panel_layers.push(ui.layer_id());
        match tab {
            DevtoolsTab::Explorer => self.show_explorer(ui),
            DevtoolsTab::Properties => self.show_properties(ui),
            DevtoolsTab::FontMarket => {
                self.font_market
                    .show(ui, self.context, self.base_font_definitions)
            }
        }
    }
}

impl DevtoolsTabViewer<'_> {
    fn show_explorer(&mut self, ui: &mut egui::Ui) {
        if self.tree_state.selected().as_slice() != self.selected.as_slice() {
            self.tree_state.set_selected(self.selected.clone());
        }
        egui::ScrollArea::vertical()
            .id_salt("tree")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (_, actions) =
                    TreeView::new(ui.make_persistent_id(("workspace_tree", self.workspace.id())))
                        .allow_multi_selection(true)
                        .range_selection_modifier(egui::Modifiers::SHIFT)
                        .set_selection_modifier(egui::Modifiers::COMMAND)
                        .allow_drag_and_drop(false)
                        .show_state(ui, self.tree_state, |builder| {
                            build_tree(builder, self.workspace, true);
                        });
                for action in actions {
                    if let Action::SetSelected(selection) = action {
                        let modifiers = ui.input(|input| input.modifiers);
                        let previous = &*self.selected;
                        if !modifiers.shift {
                            if selection.len() == 1 {
                                *self.anchor = selection.first().copied();
                            } else {
                                let changed: Vec<_> = previous
                                    .iter()
                                    .filter(|id| !selection.contains(id))
                                    .chain(selection.iter().filter(|id| !previous.contains(id)))
                                    .copied()
                                    .collect();
                                if changed.len() == 1 {
                                    *self.anchor = changed.first().copied();
                                }
                            }
                        }
                        *self.selected = selection;
                        *self.selection_changed = true;
                        *self.error = None;
                        self.json_drafts.clear();
                    }
                }
            });
    }

    fn show_properties(&mut self, ui: &mut egui::Ui) {
        if self.selected.is_empty() {
            ui.label("Select an instance to inspect its properties.");
        }
        if let Some(error) = self.error.as_deref() {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        egui::ScrollArea::vertical()
            .id_salt("properties")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for field in properties_for_selection(self.selected, self.workspace) {
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
                        let path = field.property.path;
                        if let PropertyValue::Json(json) = &value {
                            self.json_drafts.insert(path.clone(), json.clone());
                        }
                        *self.error =
                            edit_selection_property(self.workspace, self.selected, &path, value)
                                .err()
                                .map(|error| error.to_string());
                        if self.error.is_none() {
                            self.json_drafts.remove(&path);
                        }
                    }
                }
            });
    }
}

fn default_dock_state() -> DockState<DevtoolsTab> {
    let mut state = DockState::new(vec![DevtoolsTab::Explorer, DevtoolsTab::FontMarket]);
    state
        .main_surface_mut()
        .split_below(NodeIndex::root(), 0.55, vec![DevtoolsTab::Properties]);
    state
}

fn properties_for_selection(
    selected: &[InstanceId],
    workspace: &Workspace,
) -> Vec<SelectionProperty> {
    let mut instances = selected.iter().filter_map(|&id| resolve(workspace, id));
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

fn edit_selection_property(
    workspace: &mut Workspace,
    selected: &[InstanceId],
    path: &str,
    value: PropertyValue,
) -> Result<(), PropertyError> {
    let Some(property) = properties_for_selection(selected, workspace)
        .into_iter()
        .find(|property| property.property.path == path && property.property.editable)
    else {
        return Err(PropertyError::NotFound);
    };
    if !compatible(&property.property.value, &value) {
        return Err(PropertyError::InvalidValue);
    }
    for &id in selected {
        resolve(workspace, id)
            .ok_or(PropertyError::NotFound)?
            .validate_property(path, &value)?;
    }
    for &id in selected {
        resolve_mut(workspace, id)
            .ok_or(PropertyError::NotFound)?
            .set_property(path, value.clone())?;
    }
    Ok(())
}

fn show_development_notice(context: &egui::Context) {
    egui::Area::new(egui::Id::new("terrarium_development_notice"))
        .order(egui::Order::Foreground)
        .interactable(false)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -8.0))
        .show(context, |ui| {
            ui.add(
                egui_text_outline::OutlinedLabel::new(
                    "Development build: Press F12 to enable DevTools",
                )
                .text_color(egui::Color32::RED)
                .outline(egui::Stroke::new(1.5, egui::Color32::BLACK)),
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

fn build_tree(builder: &mut TreeViewBuilder<'_, InstanceId>, instance: &dyn Instance, root: bool) {
    let label = format!("{} ({})", instance.name(), instance.class_name());
    if instance.children().is_empty() {
        let icon = icon_for_instance(instance);
        builder.node(
            NodeBuilder::leaf(instance.id())
                .icon(move |ui| {
                    ui.label(icon);
                })
                .label(label),
        );
    } else {
        let is_open = builder.node(
            NodeBuilder::dir(instance.id())
                .default_open(root)
                .icon(|ui| {
                    ui.label(regular::FOLDER);
                })
                .label(label),
        );
        if is_open {
            for child in instance.children() {
                build_tree(builder, child.as_ref(), false);
            }
        }
        builder.close_dir();
    }
}

fn icon_for_instance(instance: &dyn Instance) -> &'static str {
    match instance.class_name() {
        "Camera" => regular::CAMERA,
        "PointLight" | "SpotLight" | "SurfaceLight" => regular::LIGHTBULB,
        _ => regular::CUBE,
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
