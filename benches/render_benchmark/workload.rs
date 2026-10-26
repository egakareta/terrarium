use terrarium::{glam::*, *};

const ANIMATED_PARTS_RATIO: usize = 4;
const ANIMATION_POOL_CHANGE_INTERVAL: usize = 30;
const CAMERA_DISTANCE_MIN_SCALE: f32 = 0.05;
const CAMERA_DISTANCE_MAX_SCALE: f32 = 0.90;
const CAMERA_ZOOM_SPEED: f32 = 0.45;
const DYNAMIC_LAYER_CHANGE_INTERVAL: usize = 30;
const DYNAMIC_LAYER_MIN_BLOCKS: usize = 1_000;
const DYNAMIC_LAYER_MAX_BLOCKS: usize = 2_000;
const DYNAMIC_LAYER_HEIGHT: f32 = 1.35;
const TEXTURED_PART_DIVISOR: usize = 5;

pub(super) struct RenderWorkload {
    workspace: Workspace,
    base_camera_pivot: Mat4,
    part_ids: Vec<InstanceId>,
    base_parts: Vec<Part>,
    upper_layer_extent: f32,
    upper_layer_ids: Vec<InstanceId>,
    benchmark_materials: Vec<Material>,
    random_state: u64,
    animation_frame: usize,
    animation_pool_start: usize,
}

impl RenderWorkload {
    pub(super) fn new(parts: usize, size: [u32; 2]) -> Result<Self, RendererError> {
        let mut workspace = Workspace::new();
        let (part_ids, upper_layer_extent, benchmark_materials) =
            create_benchmark_workspace(&mut workspace, parts, size[0], size[1])?;
        let base_camera_pivot = workspace.camera().pivot();
        let base_parts = part_ids
            .iter()
            .map(|&id| {
                workspace
                    .get::<Part>(id)
                    .expect("newly created benchmark part")
                    .clone()
            })
            .collect();
        let mut workload = Self {
            workspace,
            base_camera_pivot,
            part_ids,
            base_parts,
            upper_layer_extent,
            upper_layer_ids: Vec::new(),
            benchmark_materials,
            random_state: 0x4d59_5df4_d0f3_3173,
            animation_frame: 0,
            animation_pool_start: 0,
        };
        workload.replace_upper_layer();
        Ok(workload)
    }

    pub(super) fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub(super) fn advance(&mut self) {
        let part_count = self.part_ids.len();
        let animated_count = part_count / ANIMATED_PARTS_RATIO
            + usize::from(!part_count.is_multiple_of(ANIMATED_PARTS_RATIO));

        if self.animation_frame != 0
            && self
                .animation_frame
                .is_multiple_of(ANIMATION_POOL_CHANGE_INTERVAL)
        {
            for offset in 0..animated_count {
                let index = (self.animation_pool_start + offset) % part_count;
                self.reset_part(index);
            }
            self.animation_pool_start = (self.animation_pool_start + animated_count) % part_count;
        }

        if self.animation_frame != 0
            && self
                .animation_frame
                .is_multiple_of(DYNAMIC_LAYER_CHANGE_INTERVAL)
        {
            self.replace_upper_layer();
        }

        let time = self.animation_frame as f32 * 0.02;
        let camera_scale = (CAMERA_DISTANCE_MIN_SCALE + CAMERA_DISTANCE_MAX_SCALE) * 0.5
            + (CAMERA_DISTANCE_MAX_SCALE - CAMERA_DISTANCE_MIN_SCALE)
                * 0.5
                * (time * CAMERA_ZOOM_SPEED).sin();
        let (_, camera_rotation, camera_position) =
            self.base_camera_pivot.to_scale_rotation_translation();
        self.workspace
            .camera_mut()
            .with_pose(Mat4::from_rotation_translation(
                camera_rotation,
                camera_position * camera_scale,
            ));

        for offset in 0..animated_count {
            let index = (self.animation_pool_start + offset) % part_count;
            let id = self.part_ids[index];
            let base = &self.base_parts[index];
            let motion = time * 0.8 + index as f32 * 0.013;
            let pulse = (time * 1.4 + index as f32 * 0.021).sin() * 0.18;
            let color_shift = time * 1.7 + index as f32 * 0.017;

            if let Some(part) = self.workspace.get_mut::<Part>(id) {
                let (_, base_rotation, base_position) =
                    base.pivot().to_scale_rotation_translation();
                let position = base_position
                    + Vec3::new(
                        motion.sin() * 0.32,
                        (motion * 1.7).cos() * 0.16,
                        motion.cos() * 0.32,
                    );
                let rotation = base_rotation
                    * Quat::from_euler(
                        EulerRot::XYZ,
                        (motion.sin() * 12.0).to_radians(),
                        (motion.cos() * 18.0).to_radians(),
                        ((motion * 0.7).sin() * 10.0).to_radians(),
                    );
                part.with_pose(Mat4::from_rotation_translation(rotation, position))
                    .with_size(
                        base.pivot().to_scale_rotation_translation().0
                            * Vec3::new(1.0 + pulse, 1.0 + pulse * 0.6, 1.0 - pulse * 0.35),
                    )
                    .with_transparency(time.sin().max(0.0))
                    .with_color(Color3::new(
                        (base.color().r + color_shift.sin() * 0.18).clamp(0.0, 1.0),
                        (base.color().g + (color_shift + 2.1).sin() * 0.18).clamp(0.0, 1.0),
                        (base.color().b + (color_shift + 4.2).sin() * 0.18).clamp(0.0, 1.0),
                    ));
            }
        }
        self.animation_frame += 1;
    }

    fn replace_upper_layer(&mut self) {
        let previous_layer = std::mem::take(&mut self.upper_layer_ids);
        for id in previous_layer {
            if let Some(instance) = self.workspace.view_mut::<dyn Instance>(id) {
                instance.destroy();
            }
        }

        let block_count = DYNAMIC_LAYER_MIN_BLOCKS
            + (self.next_random()
                % (DYNAMIC_LAYER_MAX_BLOCKS - DYNAMIC_LAYER_MIN_BLOCKS + 1) as u64)
                as usize;
        self.upper_layer_ids.reserve(block_count);

        for _ in 0..block_count {
            let rotation = Quat::from_rotation_y(self.random_f32() * std::f32::consts::TAU);
            let position = Vec3::new(
                (self.random_f32() - 0.5) * self.upper_layer_extent,
                DYNAMIC_LAYER_HEIGHT + self.random_f32() * 0.1,
                (self.random_f32() - 0.5) * self.upper_layer_extent,
            );
            let color = Color3::new(
                0.30 + self.random_f32() * 0.22,
                0.34 + self.random_f32() * 0.22,
                0.44 + self.random_f32() * 0.22,
            );
            let mut block = Part::new()
                .with_shape(PartShape::Block)
                .with_pose(Mat4::from_rotation_translation(rotation, position))
                .with_size(Vec3::splat(0.82))
                .with_color(color);
            if self.random_f32() < 1.0 / TEXTURED_PART_DIVISOR as f32 {
                let slot = Face::ALL[(self.next_random() % Face::ALL.len() as u64) as usize];
                let material_index =
                    (self.next_random() % self.benchmark_materials.len() as u64) as usize;
                let material = self.benchmark_materials[material_index];
                block = block.with_material_slot(slot, material);
            }
            self.upper_layer_ids
                .push(block.set_parent(&mut self.workspace));
        }
    }

    fn next_random(&mut self) -> u64 {
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 7;
        self.random_state ^= self.random_state << 17;
        self.random_state
    }

    fn random_f32(&mut self) -> f32 {
        self.next_random() as f32 / u64::MAX as f32
    }

    fn reset_part(&mut self, index: usize) {
        let id = self.part_ids[index];
        let base = &self.base_parts[index];
        if let Some(part) = self.workspace.get_mut::<Part>(id) {
            part.with_pivot(base.pivot()).with_color(base.color());
        }
    }
}

fn create_benchmark_workspace(
    workspace: &mut Workspace,
    part_count: usize,
    width: u32,
    height: u32,
) -> Result<(Vec<InstanceId>, f32, Vec<Material>), RendererError> {
    let side = (part_count as f64).sqrt().ceil() as usize;
    let spacing = 1.2;
    let extent = side as f32 * spacing;
    let camera_distance = extent * 1.25 + 5.0;
    let benchmark_materials = load_benchmark_materials(workspace)?;
    *workspace.camera_mut() = Camera::new(
        Vec3::new(0.0, camera_distance * 0.72, camera_distance),
        Vec3::ZERO,
        width as f32 / height.max(1) as f32,
    );
    workspace
        .camera_mut()
        .with_zfar(camera_distance * 4.0 + extent);
    let mut part_ids = Vec::with_capacity(part_count);

    for index in 0..part_count {
        let column = index % side;
        let row = index / side;
        let mut part = Part::new()
            .with_shape(match index % 5 {
                0 => PartShape::Block,
                1 => PartShape::Ball,
                2 => PartShape::Cylinder,
                3 => PartShape::Wedge,
                _ => PartShape::CornerWedge,
            })
            .with_pose(Mat4::from_rotation_translation(
                Quat::from_rotation_y(((index % 360) as f32).to_radians()),
                Vec3::new(
                    (column as f32 - side as f32 * 0.5) * spacing,
                    0.35 + (index % 7) as f32 * 0.06,
                    (row as f32 - side as f32 * 0.5) * spacing,
                ),
            ))
            .with_size(Vec3::splat(0.82))
            .with_color(Color3::new(
                0.24 + (index % 5) as f32 * 0.12,
                0.32 + (index % 3) as f32 * 0.16,
                0.40 + (index % 4) as f32 * 0.11,
            ));
        if index.is_multiple_of(TEXTURED_PART_DIVISOR) {
            let slot = Face::ALL[index % Face::ALL.len()];
            let material =
                benchmark_materials[(index / TEXTURED_PART_DIVISOR) % benchmark_materials.len()];
            part = part.with_material_slot(slot, material);
        }
        part_ids.push(part.set_parent(workspace));
    }

    let floor = Part::new()
        .with_pose(Mat4::from_translation(Vec3::new(0.0, -0.05, 0.0)))
        .with_size(Vec3::new(
            extent + spacing * 2.0,
            0.1,
            extent + spacing * 2.0,
        ))
        .with_color(Color3::new(0.08, 0.10, 0.14));
    floor.set_parent(workspace);

    Ok((part_ids, extent, benchmark_materials))
}

/// Loads the six textured materials used by the workload.
fn load_benchmark_materials(workspace: &mut Workspace) -> Result<Vec<Material>, RendererError> {
    let cobblestone = workspace.add_texture(Texture::from_bytes(
        include_bytes!("../../assets/cobblestone.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let dirt = workspace.add_texture(Texture::from_bytes(
        include_bytes!("../../assets/dirt.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let grass_top = workspace.add_texture(Texture::from_bytes(
        include_bytes!("../../assets/grass_top.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let grass_side = workspace.add_texture(Texture::from_bytes(
        include_bytes!("../../assets/grass_side.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let castle_brick = workspace.add_texture(Texture::from_bytes(
        include_bytes!("../../assets/castle_brick.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let festival_lantern = workspace.add_texture(Texture::from_bytes(
        include_bytes!("../../assets/festival_lantern.png"),
        TextureColorSpace::Srgb,
    )?)?;

    let cobblestone_material = Material::textured(cobblestone).with_roughness(0.82);

    let dirt_material = Material::textured(dirt).with_roughness(0.9);

    let grass_top_material = Material::textured(grass_top)
        .with_filter(TextureFilter::Nearest)
        .with_roughness(0.82);

    let grass_side_material = Material::textured(grass_side)
        .with_filter(TextureFilter::Nearest)
        .with_roughness(0.82);

    let castle_brick_material = Material::textured(castle_brick)
        .with_roughness(0.75)
        .with_metallic(0.05);

    let lantern_material = Material::textured(festival_lantern)
        .with_roughness(0.6)
        .with_emissive([0.55, 0.32, 0.12]);

    Ok(vec![
        cobblestone_material,
        dirt_material,
        grass_top_material,
        grass_side_material,
        castle_brick_material,
        lantern_material,
    ])
}
