use slotmap::Key;

use crate::{
    Camera, HasBasePart, HasCamera, HasLight, HasMaterials, HasPVInstance, HasPart, HasPointLight,
    HasSpotLight, HasSurfaceLight, Instance, InstanceId, Outline, Part, PointLight, RenderCamera,
    RenderGeometry, RenderLight, RenderLightKind, RenderMesh, RenderNode, RenderOutline,
    RenderScene, RenderTransform, SpotLight, SurfaceLight, Workspace, glam,
};
#[cfg(feature = "meshpart")]
use crate::{HasMeshPart, MeshPart};

fn render_id(id: InstanceId) -> u64 {
    id.data().as_ffi()
}

fn render_node<'a>(
    workspace: &'a Workspace,
    instance: &'a dyn Instance,
    parent_transform: glam::Mat4,
) -> RenderNode<'a> {
    let light = render_light(workspace, instance);
    let world_transform = instance
        .spatial()
        .map(|spatial| spatial.world_pivot())
        .or_else(|| light.as_ref().map(|(_, transform)| *transform))
        .unwrap_or(parent_transform);
    let transform = RenderTransform::from_world(parent_transform, world_transform);
    let mesh = render_mesh(instance);
    let children = instance
        .children()
        .iter()
        .map(|child| render_node(workspace, child.as_ref(), world_transform))
        .collect();

    RenderNode {
        id: render_id(instance.id()),
        name: instance.name(),
        transform,
        mesh,
        light: light.map(|(light, _)| light),
        children,
    }
}

fn render_mesh(instance: &dyn Instance) -> Option<RenderMesh<'_>> {
    if let Some(part) = instance.downcast_ref::<Part>() {
        let shape = part.shape();
        return Some(RenderMesh {
            geometry: RenderGeometry::Primitive(shape),
            color: part.color(),
            transparency: part.transparency(),
            bounding_radius: shape.bounding_radius(),
            materials: part.material_slots(),
        });
    }
    #[cfg(feature = "meshpart")]
    if let Some(part) = instance.downcast_ref::<MeshPart>() {
        return Some(RenderMesh {
            geometry: RenderGeometry::Custom {
                mesh: part.mesh(),
                revision: part.mesh_revision(),
            },
            color: part.color(),
            transparency: part.transparency(),
            bounding_radius: part.bounding_radius(),
            materials: part.material_slots(),
        });
    }
    None
}

fn render_camera(camera: &Camera) -> RenderCamera {
    RenderCamera {
        pivot: camera.pivot(),
        pose: camera.pose(),
        view_projection: camera.view_projection_matrix(),
        projection: camera.projection_matrix(),
        forward: camera.forward(),
        fovy: camera.fovy(),
        aspect: camera.aspect(),
        znear: camera.znear(),
        zfar: camera.zfar(),
    }
}

fn render_light(
    workspace: &Workspace,
    instance: &dyn Instance,
) -> Option<(RenderLight, glam::Mat4)> {
    let (parent_id, parent_transform) = render_light_parent(workspace, instance)?;
    let (kind, light): (RenderLightKind, &dyn HasLight) =
        if let Some(light) = instance.downcast_ref::<PointLight>() {
            (
                RenderLightKind::Point {
                    range: light.range(),
                },
                light,
            )
        } else if let Some(light) = instance.downcast_ref::<SpotLight>() {
            (
                RenderLightKind::Spot {
                    face: light.face(),
                    angle: light.angle(),
                },
                light,
            )
        } else if let Some(light) = instance.downcast_ref::<SurfaceLight>() {
            (
                RenderLightKind::Surface {
                    face: light.face(),
                    angle: light.angle(),
                },
                light,
            )
        } else {
            return None;
        };
    Some((
        RenderLight {
            parent_id: render_id(parent_id),
            kind,
            color: light.color(),
            brightness: light.brightness(),
            shadows: light.shadows(),
        },
        parent_transform,
    ))
}

fn render_outlines(workspace: &Workspace) -> Vec<RenderOutline> {
    workspace
        .get_all::<Outline>()
        .filter_map(|outline| render_outline(workspace, outline))
        .collect()
}

fn render_outline(workspace: &Workspace, outline: &Outline) -> Option<RenderOutline> {
    let parent_id = outline.parent()?;
    let parent = workspace.instance(parent_id)?;
    let container_id = parent.parent()?;
    let mut targets = vec![render_id(parent_id)];
    let siblings = if container_id == workspace.id() {
        workspace.children()
    } else {
        workspace.instance(container_id)?.children()
    };
    targets.extend(siblings.iter().map(|sibling| render_id(sibling.id())));
    Some(RenderOutline {
        mode: outline.mode(),
        targets,
        color: outline.color(),
        width: outline.width(),
        threshold: outline.threshold(),
        always_on_top: outline.always_on_top(),
    })
}

impl Workspace {
    /// Builds a borrowed render snapshot from the current workspace state.
    pub fn render_scene(&self) -> RenderScene<'_> {
        RenderScene {
            id: render_id(self.id()),
            structure_revision: self.lookup.generation(),
            camera: render_camera(&self.current_camera),
            lighting: &self.lighting,
            nodes: self
                .children()
                .iter()
                .map(|child| render_node(self, child.as_ref(), glam::Mat4::IDENTITY))
                .collect(),
            outlines: render_outlines(self),
            textures: self.render_textures().collect(),
        }
    }
}

fn render_light_parent(
    workspace: &Workspace,
    light: &dyn Instance,
) -> Option<(InstanceId, glam::Mat4)> {
    let mut parent_id = light.parent();
    while let Some(id) = parent_id {
        let parent = workspace.instance(id)?;
        if let Some(spatial) = parent.spatial() {
            return Some((id, spatial.world_pivot()));
        }
        parent_id = parent.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::{Instance, Outline, Part, Workspace};

    #[test]
    fn render_scene_preserves_outline_always_on_top() {
        let mut workspace = Workspace::new();
        let part_id = workspace.add_child(Part::new());
        workspace
            .get_mut::<Part>(part_id)
            .unwrap()
            .add_child(Outline::stencil().with_always_on_top(true));

        let scene = workspace.render_scene();

        assert!(scene.outlines[0].always_on_top);
    }
}
