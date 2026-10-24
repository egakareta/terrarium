#![cfg(not(target_family = "wasm"))]

use terrarium::*;

#[test]
fn caller_encoder_renders_and_resizes_output() -> Result<(), Box<dyn std::error::Error>> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let mut renderer = Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm, [8, 8])?;
    let mut workspace = Workspace::new();
    workspace.lighting.clear_skybox();
    renderer.with_clear_color(wgpu::Color::RED);

    for size in [[8, 8], [32, 16]] {
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.encode(&workspace.render_scene(), size, &mut encoder)?;
        queue.submit(Some(encoder.finish()));
        let pixels = renderer.read_pixels()?;
        assert_eq!(pixels.len(), (size[0] * size[1]) as usize);
        assert!(pixels.iter().all(|&pixel| pixel == [255, 0, 0, 255]));
    }
    Ok(())
}
