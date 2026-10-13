use trscene::{FrameInput, Instance, InstanceId, Workspace};

use super::{Property, PropertyError, PropertyValue};

/// Properties shared by the current selection.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SelectionProperty {
    /// Field descriptor from the first selected instance.
    pub property: Property,
    /// Whether selected instances have different values for this field.
    pub mixed: bool,
}

/// Workspace inspector state shared with the React devtools.
#[derive(Default)]
pub struct Devtools {
    visible: bool,
    selected: Vec<InstanceId>,
    anchor: Option<InstanceId>,
    active: bool,
}

impl Devtools {
    /// Returns whether the inspector is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }
    /// Shows or hides the inspector without changing selection.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        self.active = visible;
    }
    /// Returns selected instance IDs in selection order.
    pub fn selected(&self) -> &[InstanceId] {
        &self.selected
    }
    /// Processes inspector shortcuts after application callbacks.
    pub fn process_input(&mut self, input: &FrameInput, workspace: &mut Workspace) {
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
    }

    /// Replaces selection from a host tree widget, discarding unavailable instances.
    pub fn set_selection(&mut self, workspace: &Workspace, selected: Vec<InstanceId>) {
        self.selected.clear();
        for id in selected {
            if resolve(workspace, id).is_some() && !self.selected.contains(&id) {
                self.selected.push(id);
            }
        }
        self.anchor = self.selected.last().copied();
        self.active = true;
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
