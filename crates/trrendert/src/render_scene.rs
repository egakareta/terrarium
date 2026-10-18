use glam::Mat4;

use crate::{Color3, Face, Lighting, Mesh, MeshMaterialSlots, PartShape, Texture, TextureHandle};

/// Rendering technique used by an outline.
#[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutlineMode {
    /// Hard screen-space edge over the selected object group.
    #[default]
    Toon,
    /// Trace the silhouette of the selected object group's rendered mask.
    Silhouette,
    /// Expand back faces using the GPU stencil buffer.
    Stencil,
}

impl OutlineMode {
    /// Returns the zero-based renderer slot for this mode.
    pub const fn index(self) -> usize {
        match self {
            Self::Toon => 0,
            Self::Silhouette => 1,
            Self::Stencil => 2,
        }
    }
}

/// A borrowed snapshot of the data needed to render one scene.
pub struct RenderScene<'a> {
    /// Stable identity of the scene, used to invalidate scene-owned GPU caches.
    pub id: u64,
    /// Revision incremented whenever scene objects are added or removed.
    pub structure_revision: u64,
    /// Camera values used for visibility, projection, and shading.
    pub camera: RenderCamera,
    /// Scene-wide lighting values.
    pub lighting: &'a Lighting,
    /// Root nodes of the scene graph.
    pub nodes: Vec<RenderNode<'a>>,
    /// Outline groups selected by the scene owner.
    pub outlines: Vec<RenderOutline>,
    /// CPU textures and their revisions, indexed by workspace-local texture handle.
    pub textures: Vec<RenderTexture<'a>>,
}

impl<'asset> RenderScene<'asset> {
    /// Iterates over the scene nodes with their resolved world transforms.
    #[doc(hidden)]
    pub fn nodes(&self) -> impl Iterator<Item = (&RenderNode<'asset>, Mat4)> + '_ {
        RenderNodeIter {
            stack: self
                .nodes
                .iter()
                .rev()
                .map(|node| (node, Mat4::IDENTITY))
                .collect(),
        }
    }
}

/// Camera properties consumed by the renderer.
#[derive(Clone, Copy, Debug)]
pub struct RenderCamera {
    /// World-space pivot, including scale.
    pub pivot: Mat4,
    /// World-space rigid pose without scale.
    pub pose: Mat4,
    /// World-space view-projection matrix.
    pub view_projection: Mat4,
    /// Projection matrix.
    pub projection: Mat4,
    /// Normalized world-space forward direction.
    pub forward: glam::Vec3,
    /// Vertical field of view in radians.
    pub fovy: f32,
    /// Width divided by height.
    pub aspect: f32,
    /// Near clipping distance.
    pub znear: f32,
    /// Far clipping distance.
    pub zfar: f32,
}

impl RenderCamera {
    /// Returns the camera's world-space position.
    pub fn position(self) -> glam::Vec3 {
        self.pivot.w_axis.truncate()
    }
}

/// Transform applied by a node in the render hierarchy.
#[derive(Clone, Copy, Debug)]
pub enum RenderTransform {
    /// Parent-relative transform, composed with the parent's world transform.
    Local(Mat4),
    /// Absolute world transform used when a parent-relative transform cannot be represented.
    World(Mat4),
}

impl RenderTransform {
    fn resolve(self, parent: Mat4) -> Mat4 {
        match self {
            Self::Local(local) => parent * local,
            Self::World(world) => world,
        }
    }

    /// Expresses a world transform relative to `parent` when possible.
    ///
    /// Singular parent transforms retain the world transform explicitly.
    pub fn from_world(parent: Mat4, world: Mat4) -> Self {
        let inverse = parent.inverse();
        if inverse.is_finite() {
            let local = inverse * world;
            if local.is_finite() {
                return Self::Local(local);
            }
        }
        Self::World(world)
    }
}

/// Geometry assigned to a node's mesh component.
#[derive(Clone, Copy)]
pub enum RenderGeometry<'a> {
    /// One of the built-in primitive shapes.
    Primitive(PartShape),
    /// Custom geometry and its revision.
    Custom {
        /// CPU-side mesh data.
        mesh: &'a Mesh,
        /// Incremented when the source mesh changes.
        revision: u64,
    },
}

/// Geometry and surface properties assigned to a scene node.
pub struct RenderMesh<'a> {
    /// Geometry used by this mesh component.
    pub geometry: RenderGeometry<'a>,
    /// Per-object tint.
    pub color: Color3,
    /// Transparency in `[0, 1]`.
    pub transparency: f32,
    /// Bounding-sphere radius in object space.
    pub bounding_radius: f32,
    /// Per-face material slots.
    pub materials: &'a MeshMaterialSlots,
}

/// A node in the borrowed scene hierarchy.
pub struct RenderNode<'a> {
    /// Stable node identity, unique within its scene.
    pub id: u64,
    /// Instance name, useful to scene consumers and debugging tools.
    pub name: &'a str,
    /// Parent-relative or fallback world transform.
    pub transform: RenderTransform,
    /// Optional mesh component.
    pub mesh: Option<RenderMesh<'a>>,
    /// Optional local-light component.
    pub light: Option<RenderLight>,
    /// Child nodes in insertion order.
    pub children: Vec<RenderNode<'a>>,
}

pub(crate) struct RenderNodeIter<'scene, 'asset> {
    stack: Vec<(&'scene RenderNode<'asset>, Mat4)>,
}

impl<'scene, 'asset> Iterator for RenderNodeIter<'scene, 'asset> {
    type Item = (&'scene RenderNode<'asset>, Mat4);

    fn next(&mut self) -> Option<Self::Item> {
        let (node, parent_transform) = self.stack.pop()?;
        let world_transform = node.transform.resolve(parent_transform);
        self.stack.extend(
            node.children
                .iter()
                .rev()
                .map(|child| (child, world_transform)),
        );
        Some((node, world_transform))
    }
}

/// The kind of local light attached to a renderable object.
#[derive(Clone, Copy, Debug)]
pub enum RenderLightKind {
    /// Omnidirectional point light.
    Point {
        /// Maximum influence distance.
        range: f32,
    },
    /// Face-oriented spot light.
    Spot {
        /// Emitting face on the parent object.
        face: Face,
        /// Full cone angle in degrees.
        angle: f32,
    },
    /// Face-oriented area light.
    Surface {
        /// Emitting face on the parent object.
        face: Face,
        /// Full emission angle in degrees.
        angle: f32,
    },
}

/// Light properties attached to a scene node.
#[derive(Clone, Copy, Debug)]
pub struct RenderLight {
    /// Stable identity of the nearest spatial node that owns this light.
    pub parent_id: u64,
    /// Light type and shape-specific settings.
    pub kind: RenderLightKind,
    /// Light color.
    pub color: Color3,
    /// Light brightness.
    pub brightness: f32,
    /// Whether this light casts shadows.
    pub shadows: bool,
}

/// An outline style applied to one or more renderable object IDs.
pub struct RenderOutline {
    /// Outline rendering mode.
    pub mode: OutlineMode,
    /// IDs of objects selected by this outline.
    pub targets: Vec<u64>,
    /// Outline color.
    pub color: Color3,
    /// Screen-space line width in pixels.
    pub width: f32,
    /// Edge threshold used by toon and silhouette detection.
    pub threshold: f32,
    /// Whether the outline renders through occluding geometry.
    pub always_on_top: bool,
}

/// A workspace-owned texture and its mutation revision.
#[derive(Clone, Copy)]
pub struct RenderTexture<'a> {
    /// Workspace-local texture handle.
    pub handle: TextureHandle,
    /// Revision incremented whenever the texture is added or edited.
    pub revision: u64,
    /// CPU-side texture data.
    pub texture: &'a Texture,
}
