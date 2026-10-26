#![cfg(not(target_family = "wasm"))]

use terrarium::*;

#[path = "../benches/render_benchmark/workload.rs"]
mod workload;

#[test]
fn caller_encoder_renders_and_resizes_output() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write as _;

    let _ = writeln!(std::io::stderr(), "render encoder: creating workspace");
    let mut workspace = Workspace::new();
    workspace.lighting.clear_skybox();
    let _ = writeln!(std::io::stderr(), "render encoder: creating instance");
    // Renderer caches retain device resources, so their native instance must stay alive too.
    static INSTANCE: std::sync::OnceLock<wgpu::Instance> = std::sync::OnceLock::new();
    let instance = INSTANCE.get_or_init(|| {
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env())
    });
    let _ = writeln!(std::io::stderr(), "render encoder: requesting adapter");
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    let _ = writeln!(
        std::io::stderr(),
        "render encoder adapter: {:?}",
        adapter.get_info()
    );
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let _ = writeln!(std::io::stderr(), "render encoder: device ready");
    let mut renderer = Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm, [8, 8])?;
    let _ = writeln!(std::io::stderr(), "render encoder: renderer ready");
    renderer.with_clear_color(wgpu::Color::RED);

    for size in [[8, 8], [32, 16]] {
        let _ = writeln!(std::io::stderr(), "render encoder: encoding {size:?}");
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.encode(&workspace.render_scene(), size, &mut encoder)?;
        let _ = writeln!(std::io::stderr(), "render encoder: submitting {size:?}");
        queue.submit(Some(encoder.finish()));
        let _ = writeln!(std::io::stderr(), "render encoder: reading {size:?}");
        let pixels = renderer.read_pixels()?;
        let _ = writeln!(std::io::stderr(), "render encoder: read {size:?}");
        assert_eq!(pixels.len(), (size[0] * size[1]) as usize);
        assert!(pixels.iter().all(|&pixel| pixel == [255, 0, 0, 255]));
    }
    let _ = writeln!(std::io::stderr(), "render encoder: complete");
    Ok(())
}

#[test]
fn render_workload_animates_textured_shapes_and_replaces_blocks() -> Result<(), RendererError> {
    let mut workload = workload::RenderWorkload::new(64, [256, 144])?;
    let workspace = workload.workspace();
    let original = workspace
        .get_all::<Part>()
        .map(|part| (part.id(), part.pivot()))
        .collect::<Vec<_>>();
    assert!((1_065..=2_065).contains(&original.len()));
    for shape in [
        PartShape::Ball,
        PartShape::Cylinder,
        PartShape::Wedge,
        PartShape::CornerWedge,
    ] {
        assert!(
            workspace
                .get_all::<Part>()
                .any(|part| part.shape() == shape)
        );
    }
    let textures = workspace
        .get_all::<Part>()
        .flat_map(|part| Face::ALL.map(|face| part.material_slot(face).textures().base_color))
        .flatten()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(textures.len(), 6);
    let camera = workspace.camera().pivot();

    for _ in 0..31 {
        workload.advance();
    }
    let workspace = workload.workspace();
    assert_ne!(workspace.camera().pivot(), camera);
    assert!(original.iter().any(|&(id, pivot)| {
        workspace
            .get::<Part>(id)
            .is_some_and(|part| part.pivot() != pivot)
    }));
    assert!(
        workspace
            .get_all::<Part>()
            .any(|part| part.transparency() > 0.0)
    );
    let removed = original
        .iter()
        .filter(|&&(id, _)| workspace.get::<Part>(id).is_none())
        .count();
    assert!((1_000..=2_000).contains(&removed));
    assert!((1_065..=2_065).contains(&workspace.get_all::<Part>().count()));
    Ok(())
}
