use std::{collections::BTreeMap, sync::Arc};

use super::super::meshpart::{append_primitive, import_material};
use crate::{glam::*, *};

#[derive(Clone, Debug)]
pub(super) struct SkinBinding {
    mesh: Vec<String>,
    source: MeshHandle,
    joints: Vec<Vec<String>>,
    inverse_bind: Vec<Mat4>,
    influences: Vec<Vec<(usize, f32)>>,
}

impl Rig {
    /// Imports the selected glTF scene, its skins, and named animation clips.
    ///
    /// Imported mesh parts do not collide. Add a stable collider to the returned
    /// rig when using physics. Skinning updates CPU geometry, including normals,
    /// tangents, and bounds, so all render passes use the animated pose.
    ///
    /// Imported clips restore omitted animation targets to their rest transforms.
    ///
    /// Morph target animations are not supported. The source bytes remain
    /// available through each imported mesh handle.
    pub fn from_gltf(bytes: &[u8], workspace: &mut Workspace) -> Result<Self, GltfError> {
        let asset = Arc::new(GltfAsset::import(bytes)?);
        let document = asset.document();
        let scene = document
            .default_scene()
            .or_else(|| document.scenes().next())
            .ok_or(GltfError::NoScene)?;
        let mut names = BTreeMap::new();
        for node in document.nodes() {
            *names
                .entry(node.name().unwrap_or("").to_owned())
                .or_insert(0) += 1;
        }
        let mut used_names = std::collections::HashSet::new();
        let node_names: Vec<_> = document
            .nodes()
            .map(|node| {
                let name = node.name().unwrap_or("");
                let mut name = if name.is_empty() || names[name] > 1 {
                    format!("{name}#{}", node.index())
                } else {
                    name.to_owned()
                };
                while !used_names.insert(name.clone()) {
                    name.push('#');
                }
                name
            })
            .collect();
        let mut paths = vec![Vec::new(); document.nodes().len()];
        fn visit(
            node: gltf::Node<'_>,
            parent: &[String],
            names: &[String],
            paths: &mut [Vec<String>],
        ) {
            let mut path = parent.to_vec();
            path.push(names[node.index()].clone());
            paths[node.index()] = path.clone();
            for child in node.children() {
                visit(child, &path, names, paths);
            }
        }
        for node in scene.nodes() {
            visit(node, &[], &node_names, &mut paths);
        }
        let mut importer = Importer {
            asset: &asset,
            workspace,
            paths: &paths,
            texture_cache: Default::default(),
            skins: Vec::new(),
        };
        let mut rig = Rig::new().with_name(scene.name().unwrap_or("Rig"));
        for node in scene.nodes() {
            rig.add_child_box(importer.node(node, Mat4::IDENTITY)?);
        }
        rig.skins = importer.skins.into();
        if !rig
            .descendants()
            .any(|instance| instance.is_a::<MeshPart>())
        {
            return Err(GltfError::NoMesh);
        }
        let rest_poses: BTreeMap<_, _> = document
            .animations()
            .flat_map(|animation| animation.channels())
            .map(|channel| channel.target().node())
            .filter(|node| !paths[node.index()].is_empty())
            .map(|node| {
                (
                    node.index(),
                    Mat4::from_cols_array_2d(&node.transform().matrix()),
                )
            })
            .collect();
        for animation in document.animations() {
            let mut tracks: BTreeMap<usize, crate::interpolation::GltfTransformTrack> =
                BTreeMap::new();
            for channel in animation.channels() {
                let node = channel.target().node();
                if paths[node.index()].is_empty() {
                    continue;
                }
                let reader =
                    channel.reader(|buffer| Some(asset.buffers()[buffer.index()].0.as_slice()));
                let times = reader.read_inputs().ok_or(AnimationError::Empty)?.collect();
                let outputs = reader.read_outputs().ok_or(AnimationError::Empty)?;
                let (values, rotation) = match outputs {
                    gltf::animation::util::ReadOutputs::Translations(values) => (
                        values
                            .map(|value| Vec3::from_array(value).extend(0.0))
                            .collect(),
                        false,
                    ),
                    gltf::animation::util::ReadOutputs::Rotations(values) => {
                        (values.into_f32().map(Vec4::from_array).collect(), true)
                    }
                    gltf::animation::util::ReadOutputs::Scales(values) => (
                        values
                            .map(|value| Vec3::from_array(value).extend(0.0))
                            .collect(),
                        false,
                    ),
                    gltf::animation::util::ReadOutputs::MorphTargetWeights(_) => {
                        return Err(GltfError::UnsupportedMorphTargets);
                    }
                };
                let data = crate::interpolation::GltfChannel {
                    times,
                    values,
                    interpolation: channel.sampler().interpolation(),
                    rotation,
                };
                let track = tracks.entry(node.index()).or_insert_with(|| {
                    crate::interpolation::GltfTransformTrack {
                        rest: Mat4::from_cols_array_2d(&node.transform().matrix()),
                        translation: None,
                        rotation: None,
                        scale: None,
                    }
                });
                match channel.target().property() {
                    gltf::animation::Property::Translation => track.translation = Some(data),
                    gltf::animation::Property::Rotation => track.rotation = Some(data),
                    gltf::animation::Property::Scale => track.scale = Some(data),
                    gltf::animation::Property::MorphTargetWeights => {
                        return Err(GltfError::UnsupportedMorphTargets);
                    }
                }
            }
            if tracks.is_empty() {
                continue;
            }
            let tracks = rest_poses
                .iter()
                .map(|(&node, &rest)| match tracks.remove(&node) {
                    Some(channels) => AnimationTrack::from_gltf(paths[node].clone(), channels),
                    None => {
                        AnimationTrack::try_new(paths[node].clone(), [Keyframe::new(0.0, rest)])
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut name = animation
                .name()
                .filter(|name| !name.is_empty())
                .filter(|name| !rig.clips.contains_key(*name))
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Animation#{}", animation.index()));
            while rig.clips.contains_key(&name) {
                name.push('#');
            }
            rig.clips.insert(name, AnimationClip::try_new(tracks)?);
        }
        update_skins(&mut rig);
        Ok(rig)
    }
}

struct Importer<'a> {
    asset: &'a Arc<GltfAsset>,
    workspace: &'a mut Workspace,
    paths: &'a [Vec<String>],
    texture_cache: std::collections::HashMap<(usize, TextureColorSpace), TextureHandle>,
    skins: Vec<SkinBinding>,
}

impl Importer<'_> {
    fn node(&mut self, node: gltf::Node<'_>, parent: Mat4) -> Result<Box<dyn Instance>, GltfError> {
        let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        if !transform.inverse().is_finite() {
            return Err(AnimationError::InvalidTransform.into());
        }
        let name = self.paths[node.index()]
            .last()
            .expect("visited scene node")
            .clone();
        let mut instance: Box<dyn Instance> = if let Some(mesh) = node.mesh() {
            let mut vertices = Vec::new();
            let mut indices = Vec::new();
            let mut material_keys = Vec::new();
            let mut influences = Vec::new();
            for primitive in mesh.primitives() {
                let start = vertices.len();
                append_primitive(
                    primitive.clone(),
                    mesh.index(),
                    Mat4::IDENTITY,
                    self.asset.buffers(),
                    &mut vertices,
                    &mut indices,
                    &mut material_keys,
                )?;
                if let Some(skin) = node.skin() {
                    let count = vertices.len() - start;
                    let reader = primitive
                        .reader(|buffer| Some(self.asset.buffers()[buffer.index()].0.as_slice()));
                    let mut weights = vec![Vec::new(); count];
                    for (semantic, _) in primitive.attributes() {
                        let gltf::Semantic::Joints(set) = semantic else {
                            continue;
                        };
                        let joints: Vec<_> = reader
                            .read_joints(set)
                            .ok_or_else(|| GltfError::InvalidSkin("missing joints".into()))?
                            .into_u16()
                            .collect();
                        let values: Vec<_> = reader
                            .read_weights(set)
                            .ok_or_else(|| GltfError::InvalidSkin("missing weights".into()))?
                            .into_f32()
                            .collect();
                        if joints.len() != count || values.len() != count {
                            return Err(GltfError::InvalidSkin("attribute lengths differ".into()));
                        }
                        for ((weights, joints), values) in
                            weights.iter_mut().zip(joints).zip(values)
                        {
                            for (joint, weight) in joints.into_iter().zip(values) {
                                if !weight.is_finite()
                                    || weight < 0.0
                                    || joint as usize >= skin.joints().len()
                                {
                                    return Err(GltfError::InvalidSkin(
                                        "invalid joint influence".into(),
                                    ));
                                }
                                if weight > 0.0 {
                                    weights.push((joint as usize, weight));
                                }
                            }
                        }
                    }
                    for weights in &mut weights {
                        let total: f32 = weights.iter().map(|(_, weight)| *weight).sum();
                        if !total.is_finite() || total <= 0.0 {
                            return Err(GltfError::InvalidSkin(
                                "vertex has no joint weights".into(),
                            ));
                        }
                        for (_, weight) in weights {
                            *weight /= total;
                        }
                    }
                    influences.extend(weights);
                }
            }
            let mut materials = MeshMaterialSlots::default();
            for (face, key) in Face::ALL.into_iter().zip(material_keys) {
                materials.set(
                    face,
                    import_material(
                        key.and_then(|index| self.asset.document().materials().nth(index)),
                        self.asset.images(),
                        self.workspace,
                        &mut self.texture_cache,
                    )?,
                );
            }
            let handle =
                MeshHandle::from_gltf(self.asset.clone(), Mesh::new(vertices, indices), materials);
            if let Some(skin) = node.skin() {
                let joints: Vec<_> = skin
                    .joints()
                    .map(|joint| self.paths[joint.index()].clone())
                    .collect();
                if joints.iter().any(Vec::is_empty) {
                    return Err(GltfError::InvalidSkin(
                        "joint is outside the selected scene".into(),
                    ));
                }
                let inverse_bind: Vec<_> = skin
                    .reader(|buffer| Some(self.asset.buffers()[buffer.index()].0.as_slice()))
                    .read_inverse_bind_matrices()
                    .map(|matrices| {
                        matrices
                            .map(|matrix| Mat4::from_cols_array_2d(&matrix))
                            .collect()
                    })
                    .unwrap_or_else(|| vec![Mat4::IDENTITY; joints.len()]);
                if inverse_bind.len() != joints.len()
                    || inverse_bind.iter().any(|matrix| !matrix.is_finite())
                {
                    return Err(GltfError::InvalidSkin(
                        "invalid inverse bind matrices".into(),
                    ));
                }
                self.skins.push(SkinBinding {
                    mesh: self.paths[node.index()].clone(),
                    source: handle.clone(),
                    joints,
                    inverse_bind,
                    influences,
                });
            }
            Box::new(
                MeshPart::new(handle)
                    .with_name(name)
                    .with_pivot(transform)
                    .with_can_collide(false),
            )
        } else {
            Box::new(Rig::new().with_name(name).with_pivot(transform))
        };
        for child in node.children() {
            instance.add_child_box(self.node(child, transform)?);
        }
        Ok(instance)
    }
}

fn resolve<'a>(mut root: &'a dyn Instance, path: &[String]) -> Option<&'a dyn Instance> {
    for name in path {
        root = root
            .children()
            .iter()
            .find(|child| child.name() == name)?
            .as_ref();
    }
    Some(root)
}

fn resolve_mut<'a>(root: &'a mut dyn Instance, path: &[String]) -> Option<&'a mut dyn Instance> {
    let Some((name, rest)) = path.split_first() else {
        return Some(root);
    };
    let child = root.children_mut().find(|child| child.name() == name)?;
    resolve_mut(child, rest)
}

#[cfg(all(feature = "physics", feature = "default-player-rig"))]
pub(crate) fn add_soft_tissue(
    rig: &mut Rig,
    parent: &[String],
    name: &str,
    center: Vec3,
    radius: Vec3,
    strength: f32,
) -> Vec<String> {
    let parent_pose = resolve(rig, parent)
        .unwrap()
        .spatial()
        .unwrap()
        .world_pivot();
    let mesh_pose = resolve(rig, &rig.skins[0].mesh)
        .unwrap()
        .spatial()
        .unwrap()
        .world_pivot();
    let pose = mesh_pose * Mat4::from_translation(center);
    let mut path = parent.to_vec();
    path.push(name.to_owned());
    resolve_mut(rig, parent)
        .unwrap()
        .add_child_box(Box::new(Rig::new().with_name(name).with_pivot(pose)));
    for skin in Arc::make_mut(&mut rig.skins) {
        let Some(source) = skin.joints.iter().position(|joint| joint == parent) else {
            continue;
        };
        let joint = skin.joints.len();
        skin.joints.push(path.clone());
        // Splitting only the source bone's weight preserves its exact animation when at rest.
        skin.inverse_bind
            .push(pose.inverse() * parent_pose * skin.inverse_bind[source]);
        for (vertex, influences) in skin.source.mesh().vertices.iter().zip(&mut skin.influences) {
            let distance = ((Vec3::from_array(vertex.position) - center) / radius).length_squared();
            let falloff = (1.0 - distance).max(0.0).powi(2) * strength;
            if falloff == 0.0 {
                continue;
            }
            if let Some((_, weight)) = influences.iter_mut().find(|(index, _)| *index == source) {
                let transferred = *weight * falloff;
                *weight -= transferred;
                if transferred > 0.0 {
                    influences.push((joint, transferred));
                }
            }
        }
    }
    path
}

pub(super) fn update_skins<T: RigExt + ?Sized>(rig: &mut T) {
    let root = rig.as_instance();
    let updates: Vec<_> = rig
        .rig()
        .skins
        .iter()
        .filter_map(|skin| {
            let mesh_instance = resolve(root, &skin.mesh)?;
            if !mesh_instance
                .downcast_ref::<MeshPart>()?
                .mesh_handle()
                .same_asset(&skin.source)
            {
                return None;
            }
            let inverse = mesh_instance.spatial()?.world_pivot().inverse();
            if !inverse.is_finite() {
                return None;
            }
            let matrices: Vec<_> = skin
                .joints
                .iter()
                .zip(&skin.inverse_bind)
                .map(|(path, bind)| {
                    Some(inverse * resolve(root, path)?.spatial()?.world_pivot() * *bind)
                })
                .collect::<Option<_>>()?;
            let mut mesh = skin.source.mesh().clone();
            for (vertex, influences) in mesh.vertices.iter_mut().zip(&skin.influences) {
                let matrix = influences
                    .iter()
                    .fold(Mat4::ZERO, |matrix, (joint, weight)| {
                        matrix + matrices[*joint] * *weight
                    });
                let normal_matrix = matrix.inverse().transpose();
                let normal = normal_matrix
                    .transform_vector3(Vec3::from_array(vertex.normal))
                    .normalize_or(Vec3::Y);
                let tangent = matrix
                    .transform_vector3(Vec3::from_array(vertex.tangent[..3].try_into().unwrap()));
                let tangent = (tangent - normal * normal.dot(tangent)).normalize_or(Vec3::X);
                vertex.position = matrix
                    .transform_point3(Vec3::from_array(vertex.position))
                    .to_array();
                vertex.normal = normal.to_array();
                vertex.tangent = [
                    tangent.x,
                    tangent.y,
                    tangent.z,
                    vertex.tangent[3]
                        * if matrix.determinant() < 0.0 {
                            -1.0
                        } else {
                            1.0
                        },
                ];
            }
            Some((skin.mesh.clone(), mesh))
        })
        .collect();
    for (path, mesh) in updates {
        if let Some(part) = resolve_mut(rig.as_instance_mut(), &path)
            .and_then(|instance| instance.downcast_mut::<MeshPart>())
        {
            part.deform(mesh);
        }
    }
}
