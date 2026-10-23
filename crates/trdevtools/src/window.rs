use std::collections::HashMap;

use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};
use tregui::{
    egui_ltreeview::{Action, NodeBuilder, TreeView, TreeViewBuilder, TreeViewState},
    egui_phosphor::regular,
    egui_text_outline,
};
use trscene::{Instance, InstanceExt, InstanceId, Workspace};

use super::{Property, PropertyError, PropertyValue};
use crate::font_market::FontMarket;

/// Properties shared by the current selection.
#[derive(Clone, Debug)]
pub struct SelectionProperty {
    /// Field descriptor from the first selected instance.
    pub property: Property,
    /// Whether selected instances have different values for this field.
    pub mixed: bool,
}

/// Application integration used by the full-window DevTools dock.
pub trait DevtoolsHost {
    /// Returns the title used for the scene tab.
    fn scene_title(&self) -> &str;

    /// Returns mutable access to the workspace being inspected.
    fn workspace(&mut self) -> &mut Workspace;

    /// Returns the instance-tree root being inspected.
    fn root(&mut self) -> &mut dyn Instance {
        self.workspace()
    }

    /// Renders the application's scene content with the current DevTools selection.
    fn render_scene(&mut self, ui: &mut egui::Ui, selected: &[InstanceId]);
}

/// A full-window workspace inspector dock with a persistent selection.
///
/// Engines create one automatically with the `devtools` feature enabled. F12
/// toggles visibility; while hidden, only the scene is rendered. Ctrl-click
/// (Command-click on macOS) toggles rows, Shift-click selects a visible range,
/// and Delete destroys the selection while the inspector is active and no text
/// editor has focus. The Explorer and Properties panes can be moved, split, or
/// detached into windows.
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
    /// Returns whether the inspector dock is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Shows or hides the inspector dock without changing its selection.
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
    pub fn properties(&self, root: &dyn Instance) -> Vec<SelectionProperty> {
        properties_for_selection(&self.selected, root)
    }

    /// Applies an edit to a common editable field of the selection.
    pub fn edit_property(
        &mut self,
        root: &mut dyn Instance,
        path: &str,
        value: PropertyValue,
    ) -> Result<(), PropertyError> {
        self.prune(root);
        edit_selection_property(root, &self.selected, path, value)
    }

    /// Destroys selected instances and their descendants, retaining the root and persistent children.
    pub fn destroy_selection(&mut self, root: &mut dyn Instance) {
        for id in self.selected.clone() {
            if id != root.id()
                && let Some(instance) = resolve_mut(root, id)
            {
                instance.destroy();
            }
        }
        self.prune(root);
        self.error = None;
    }

    /// Activates the scene tab for camera movement unless a text editor has focus.
    ///
    /// Call before processing camera input to release keyboard capture in the same frame.
    /// [`Self::show_docked`] also calls this for standalone integrations.
    pub fn focus_scene_on_camera_input(&mut self, context: &egui::Context, workspace: &Workspace) {
        if context.text_edit_focused()
            || !context.input(|input| workspace.has_eframe_movement_input(input))
        {
            return;
        }
        context.memory_mut(|memory| {
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
        if let Some(path) = self.dock_state.find_tab(&DevtoolsTab::Scene) {
            self.dock_state
                .set_active_tab(path)
                .expect("scene tab path is valid");
            self.dock_state
                .set_focused_node_and_surface(path.node_path());
        }
    }

    /// Shows the application scene and inspector panes in a full-window dock.
    pub fn show_docked(&mut self, ui: &mut egui::Ui, host: &mut dyn DevtoolsHost) {
        let context = ui.ctx().clone();
        let frame = context.cumulative_frame_nr();
        if self.last_input_frame != Some(frame) {
            self.last_input_frame = Some(frame);
            if pressed(&context, egui::Key::F12) {
                context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F12));
                self.set_visible(!self.visible);
            }
        }
        let scene_title = host.scene_title().to_owned();
        let workspace_id = host.workspace().id();
        self.prune(host.root());
        self.focus_scene_on_camera_input(&context, host.workspace());

        if !self.visible {
            host.render_scene(ui, &self.selected);
            self.show_toggle_button(&context, workspace_id);
            show_development_notice(&context);
            return;
        }

        let scene_selection = self.selected.clone();
        let mut selection_changed = false;
        let mut panel_layers = Vec::new();
        let mut context_delete_selection = None;
        let mut viewer = DevtoolsTabViewer {
            host,
            scene_title: scene_title.clone(),
            scene_selection: &scene_selection,
            selected: &mut self.selected,
            anchor: &mut self.anchor,
            tree_state: &mut self.tree_state,
            selection_changed: &mut selection_changed,
            error: &mut self.error,
            json_drafts: &mut self.json_drafts,
            font_market: &mut self.font_market,
            base_font_definitions: self.base_font_definitions.as_ref(),
            context: &context,
            panel_layers: &mut panel_layers,
            context_delete_selection: &mut context_delete_selection,
        };
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: 8,
                    right: 8,
                    top: 4,
                    bottom: 8,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.menu_button(format!("{} Panels", regular::SLIDERS_HORIZONTAL), |ui| {
                            for tab in [
                                DevtoolsTab::Scene,
                                DevtoolsTab::Explorer,
                                DevtoolsTab::Properties,
                                DevtoolsTab::FontMarket,
                            ] {
                                show_panel_toggle(ui, &mut self.dock_state, &scene_title, tab);
                            }
                        });
                        if ui
                            .button(format!("{} Reset layout", regular::ARROW_COUNTER_CLOCKWISE))
                            .clicked()
                        {
                            self.dock_state = default_dock_state();
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(concat!("terrarium@", env!("CARGO_PKG_VERSION")));
                        });
                    });
                });
            DockArea::new(&mut self.dock_state)
                .id(egui::Id::new(("terrarium_app_dock", workspace_id)))
                .style(Style::from_egui(ui.style().as_ref()))
                .show_inside(ui, &mut viewer);
        });

        if context.input(|input| input.pointer.any_pressed()) {
            self.active = selection_changed
                || context
                    .input(|input| input.pointer.interact_pos())
                    .is_some_and(|position| {
                        context
                            .layer_id_at(position)
                            .is_some_and(|layer| panel_layers.contains(&layer))
                    });
        }

        if let Some(selection) = context_delete_selection {
            self.selected = selection;
            context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Delete));
            self.destroy_selection(host.root());
        } else if self.active
            && !context.text_edit_focused()
            && pressed(&context, egui::Key::Delete)
        {
            context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Delete));
            self.destroy_selection(host.root());
        }
    }

    fn show_toggle_button(&mut self, context: &egui::Context, workspace_id: InstanceId) {
        egui::Area::new(egui::Id::new(("terrarium_devtools_toggle", workspace_id)))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
            .show(context, |ui| {
                if ui
                    .add_sized(
                        egui::vec2(40.0, 40.0),
                        egui::Button::new(egui::RichText::new(regular::WRENCH).size(22.0)),
                    )
                    .on_hover_text("Show DevTools (F12)")
                    .clicked()
                {
                    self.set_visible(true);
                }
            });
    }

    fn prune(&mut self, root: &dyn Instance) {
        self.selected.retain(|&id| resolve(root, id).is_some());
        if self.anchor.is_some_and(|id| resolve(root, id).is_none()) {
            self.anchor = None;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum DevtoolsTab {
    Scene,
    Explorer,
    Properties,
    FontMarket,
}

impl DevtoolsTab {
    fn title(self, scene_title: &str) -> String {
        match self {
            Self::Scene => format!("{} {scene_title}", regular::CUBE),
            Self::Explorer => format!("{} Explorer", regular::TREE_STRUCTURE),
            Self::Properties => format!("{} Properties", regular::GEAR),
            Self::FontMarket => format!("{} Font Market", regular::TEXT_T),
        }
    }
}

struct DevtoolsTabViewer<'a> {
    host: &'a mut dyn DevtoolsHost,
    scene_title: String,
    scene_selection: &'a [InstanceId],
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
    context_delete_selection: &'a mut Option<Vec<InstanceId>>,
}

impl TabViewer for DevtoolsTabViewer<'_> {
    type Tab = DevtoolsTab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title(&self.scene_title).into()
    }

    fn closeable(&mut self, _tab: &mut Self::Tab) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        self.panel_layers.push(ui.layer_id());
        match tab {
            DevtoolsTab::Scene => self.host.render_scene(ui, self.scene_selection),
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
        let root = self.host.root();
        let root_id = root.id();
        let context_delete_selection = &mut *self.context_delete_selection;
        if self.tree_state.selected().as_slice() != self.selected.as_slice() {
            self.tree_state.set_selected(self.selected.clone());
        }
        egui::ScrollArea::vertical()
            .id_salt("tree")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (_, actions) =
                    TreeView::new(ui.make_persistent_id(("workspace_tree", root_id)))
                        .fallback_context_menu(|ui, ids| {
                            let can_delete = ids.iter().any(|&id| {
                                id != root_id
                                    && resolve(root, id).is_some_and(Instance::is_destroyable)
                            });
                            if ui
                                .add_enabled(
                                    can_delete,
                                    egui::Button::new(format!("{} Delete", regular::TRASH)),
                                )
                                .clicked()
                            {
                                *context_delete_selection = Some(ids.clone());
                            }
                        })
                        .allow_multi_selection(true)
                        .range_selection_modifier(egui::Modifiers::SHIFT)
                        .set_selection_modifier(egui::Modifiers::COMMAND)
                        .allow_drag_and_drop(false)
                        .show_state(ui, self.tree_state, |builder| {
                            build_tree(builder, root, true);
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
        let root = self.host.root();
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
                for field in properties_for_selection(self.selected, root) {
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
                        *self.error = edit_selection_property(root, self.selected, &path, value)
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

fn show_panel_toggle(
    ui: &mut egui::Ui,
    dock_state: &mut DockState<DevtoolsTab>,
    scene_title: &str,
    tab: DevtoolsTab,
) {
    let mut is_open = dock_state.find_tab(&tab).is_some();
    if ui.checkbox(&mut is_open, tab.title(scene_title)).changed() {
        if is_open {
            dock_state.main_surface_mut().push_to_focused_leaf(tab);
        } else if let Some(path) = dock_state.find_tab(&tab) {
            dock_state.remove_tab(path);
        }
    }
}

fn default_dock_state() -> DockState<DevtoolsTab> {
    let mut state = DockState::new(vec![DevtoolsTab::Scene]);
    let [_, right] = state.main_surface_mut().split_right(
        NodeIndex::root(),
        0.8,
        vec![DevtoolsTab::Explorer, DevtoolsTab::FontMarket],
    );
    state
        .main_surface_mut()
        .split_below(right, 0.5, vec![DevtoolsTab::Properties]);
    state
}

fn properties_for_selection(
    selected: &[InstanceId],
    root: &dyn Instance,
) -> Vec<SelectionProperty> {
    let mut instances = selected.iter().filter_map(|&id| resolve(root, id));
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
    root: &mut dyn Instance,
    selected: &[InstanceId],
    path: &str,
    value: PropertyValue,
) -> Result<(), PropertyError> {
    let Some(property) = properties_for_selection(selected, root)
        .into_iter()
        .find(|property| property.property.path == path && property.property.editable)
    else {
        return Err(PropertyError::NotFound);
    };
    if !compatible(&property.property.value, &value) {
        return Err(PropertyError::InvalidValue);
    }
    for &id in selected {
        resolve(root, id)
            .ok_or(PropertyError::NotFound)?
            .validate_property(path, &value)?;
    }
    for &id in selected {
        resolve_mut(root, id)
            .ok_or(PropertyError::NotFound)?
            .set_property(path, value.clone())?;
    }
    Ok(())
}

fn pressed(context: &egui::Context, key: egui::Key) -> bool {
    context.input(|input| {
        input.events.iter().any(|event| {
            matches!(event, egui::Event::Key { key: event_key, pressed: true, repeat: false, .. } if *event_key == key)
        })
    })
}

fn show_development_notice(context: &egui::Context) {
    egui::Area::new(egui::Id::new("terrarium_development_notice"))
        .order(egui::Order::Foreground)
        .interactable(false)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -8.0))
        .show(context, |ui| {
            ui.add(
                egui_text_outline::OutlinedLabel::new("Development build")
                    .text_color(egui::Color32::RED)
                    .outline(egui::Stroke::new(1.5, egui::Color32::BLACK)),
            );
        });
}

fn resolve(root: &dyn Instance, id: InstanceId) -> Option<&dyn Instance> {
    root.view::<dyn Instance>(id)
}

fn resolve_mut(root: &mut dyn Instance, id: InstanceId) -> Option<&mut dyn Instance> {
    root.view_mut::<dyn Instance>(id)
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
                .default_open(root || instance.is_a::<Workspace>())
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
