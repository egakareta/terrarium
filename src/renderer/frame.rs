use super::*;

impl Renderer {
    /// Sets the color used to clear the color attachment before each frame.
    ///
    pub fn with_clear_color(&mut self, color: wgpu::Color) -> &mut Self {
        self.clear_color = color;
        self
    }

    /// Returns the average number of successfully rendered frames per second over the last
    /// measurement interval.
    pub fn fps(&self) -> f32 {
        self.fps
    }

    /// Returns the elapsed time in seconds since the previous call, capped at 100 milliseconds.
    pub fn delta_secs(&mut self) -> f32 {
        let now = Instant::now();
        let delta_seconds = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        delta_seconds.min(0.1)
    }

    /// Blocks until all GPU work submitted before this call has completed.
    ///
    /// This is intended for deterministic measurements and should not be used in a real-time
    /// render loop, where allowing multiple frames in flight is preferable.
    pub fn wait_for_gpu(&self) -> Result<(), RendererError> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map(|_| ())
            .map_err(RendererError::DevicePoll)
    }

    /// Reads an RGBA8 pixel from the most recently prepared eframe scene.
    ///
    /// Coordinates use a top-left origin.
    ///
    /// On web, this starts an asynchronous capture when one is not already in
    /// flight and returns the latest completed capture. The first call returns
    /// [`RendererError::PixelReadbackPending`].
    pub fn read_pixel(&self, x: u32, y: u32) -> Result<[u8; 4], RendererError> {
        if x >= self.width || y >= self.height {
            return Err(RendererError::InvalidPixel {
                x,
                y,
                width: self.width,
                height: self.height,
            });
        }

        let pixels = self.read_pixels()?;
        Ok(pixels[y as usize * self.width as usize + x as usize])
    }

    /// Reads all RGBA8 pixels from the most recently prepared eframe scene.
    ///
    /// Pixels are returned in row-major order with a top-left origin.
    ///
    /// On web, this starts an asynchronous capture when one is not already in
    /// flight and returns the latest completed capture. The first call returns
    /// [`RendererError::PixelReadbackPending`], and completed captures may be
    /// several frames old.
    pub fn read_pixels(&self) -> Result<Vec<[u8; 4]>, RendererError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.read_texture_pixels(
                &self.eframe_scene._texture,
                self.width,
                self.height,
                self.eframe_scene.format,
            )
        }

        #[cfg(target_arch = "wasm32")]
        {
            let (latest, error) = {
                let mut readback = self.web_pixel_readback.borrow_mut();
                (readback.latest.clone(), readback.error.take())
            };

            self.start_web_pixel_readback()?;

            if let Some(error) = error {
                return Err(RendererError::PixelReadback(error));
            }
            latest.ok_or(RendererError::PixelReadbackPending)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn read_texture_pixels(
        &self,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<Vec<[u8; 4]>, RendererError> {
        let (unpadded_bytes_per_row, padded_bytes_per_row, buffer_size) =
            pixel_readback_layout(width, height)?;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render target pixel readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render target pixel readback encoder"),
            });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row as u32),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let (sender, receiver) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })?;
        receiver
            .recv()
            .map_err(|error| RendererError::PixelReadback(error.to_string()))?
            .map_err(|error| RendererError::PixelReadback(error.to_string()))?;

        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|error| RendererError::PixelReadback(error.to_string()))?;
        let pixels = decode_pixel_readback(
            &mapped,
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
            format,
        );
        drop(mapped);
        buffer.unmap();
        Ok(pixels)
    }

    #[cfg(target_arch = "wasm32")]
    fn start_web_pixel_readback(&self) -> Result<(), RendererError> {
        let (unpadded_bytes_per_row, padded_bytes_per_row, buffer_size) =
            pixel_readback_layout(self.width, self.height)?;
        let generation = {
            let mut readback = self.web_pixel_readback.borrow_mut();
            if readback.in_flight_generation.is_some() {
                return Ok(());
            }
            let generation = readback.generation;
            readback.in_flight_generation = Some(generation);
            generation
        };

        let buffer = Rc::new(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("web render target pixel readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        }));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("web render target pixel readback encoder"),
            });
        encoder.copy_texture_to_buffer(
            self.eframe_scene._texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: buffer.as_ref(),
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row as u32),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let callback_buffer = Rc::clone(&buffer);
        let readback = Rc::clone(&self.web_pixel_readback);
        let width = self.width;
        let height = self.height;
        let format = self.eframe_scene.format;
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let pixels = match result {
                    Ok(()) => match callback_buffer.slice(..).get_mapped_range() {
                        Ok(mapped) => {
                            let pixels = decode_pixel_readback(
                                &mapped,
                                width,
                                height,
                                unpadded_bytes_per_row,
                                padded_bytes_per_row,
                                format,
                            );
                            drop(mapped);
                            callback_buffer.unmap();
                            Ok(pixels)
                        }
                        Err(error) => {
                            callback_buffer.unmap();
                            Err(error.to_string())
                        }
                    },
                    Err(error) => Err(error.to_string()),
                };

                let mut readback = readback.borrow_mut();
                if readback.in_flight_generation == Some(generation) {
                    readback.in_flight_generation = None;
                }
                if readback.generation != generation {
                    return;
                }
                match pixels {
                    Ok(pixels) => {
                        readback.latest = Some(pixels);
                        readback.error = None;
                    }
                    Err(error) => readback.error = Some(error),
                }
            });
        Ok(())
    }

    /// Resizes the eframe scene and depth buffer for a new non-zero size.
    ///
    /// Zero dimensions are ignored, which is useful while a window is
    /// minimized.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut readback = self.web_pixel_readback.borrow_mut();
            readback.generation = readback.generation.wrapping_add(1);
            readback.in_flight_generation = None;
            readback.latest = None;
            readback.error = None;
        }
        self.width = width;
        self.height = height;
        let (depth_texture, depth_view) = create_depth_texture(&self.device, width, height);
        self.depth_texture = depth_texture;
        self.depth_view = depth_view;
        self.scene_texture =
            create_eframe_scene_texture(&self.device, self.eframe_scene.format, width, height);
        self.scene_view = self
            .scene_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (outline_mask_texture, outline_mask_view) =
            create_outline_mask_texture(&self.device, width, height);
        self.outline_mask_texture = outline_mask_texture;
        self.outline_mask_view = outline_mask_view;
        self.outline_bind_group = create_outline_bind_group(
            &self.device,
            &self.outline_bind_group_layout,
            &self.scene_view,
            &self.eframe_scene.sampler,
            &self.outline_mask_view,
            &self.outline_uniform_buffer,
        );
        self.eframe_scene.resize(&self.device, width, height);
    }

    /// Prepares a workspace for drawing in an eframe WGPU paint callback.
    pub fn prepare_eframe_scene(
        &mut self,
        workspace: &Workspace,
        size: [u32; 2],
    ) -> Result<(), RendererError> {
        let width = size[0].max(1);
        let height = size[1].max(1);
        if self.width != width || self.height != height {
            self.resize(width, height);
        }
        self.prepare_scene(workspace)?;
        self.submit_scene();
        Ok(())
    }

    /// Draws the prepared workspace into an eframe WGPU render pass.
    pub fn paint_eframe_scene<'a>(&mut self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.eframe_scene.pipeline);
        pass.set_bind_group(0, &self.eframe_scene.bind_group, &[]);
        pass.draw(0..3, 0..1);
        self.record_frame();
    }

    pub(super) fn encode_shadow_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.shadow_pass_enabled {
            for (cascade, view) in self.shadow_layer_views.iter().enumerate() {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("directional shadow cascade pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                self.draw_shadow_scene(&mut pass, cascade, 1 << (cascade + 1));
            }
        }
        for (layer, view) in self
            .local_shadow_layer_views
            .iter()
            .take(self.local_shadow_layer_count)
            .enumerate()
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("local light shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.draw_shadow_scene(
                &mut pass,
                SHADOW_CASCADE_COUNT + layer,
                1 << (LOCAL_SHADOW_VISIBILITY_OFFSET + layer),
            );
        }
    }

    pub(super) fn encode_scene_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        clear_color: wgpu::Color,
    ) {
        let color_attachment = wgpu::RenderPassColorAttachment {
            view: color_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear_color),
                store: wgpu::StoreOp::Store,
            },
        };
        let depth_attachment = wgpu::RenderPassDepthStencilAttachment {
            view: depth_view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(0),
                store: wgpu::StoreOp::Store,
            }),
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene render pass"),
            color_attachments: &[Some(color_attachment)],
            depth_stencil_attachment: Some(depth_attachment),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.draw_scene(&mut pass);
    }

    pub(super) fn encode_outline_mask_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("screen-space outline mask pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.outline_mask_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.draw_outline_batches(
            &mut pass,
            OutlineMode::Toon,
            &self.toon_mask_pipeline,
            false,
        );
        self.draw_outline_batches(
            &mut pass,
            OutlineMode::Silhouette,
            &self.silhouette_mask_pipeline,
            false,
        );
    }

    pub(super) fn encode_stencil_outline_passes(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.outline_batches[OutlineMode::Stencil.index()].is_empty() {
            return;
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("stencil outline mask pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scene_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: None,
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_stencil_reference(1);
            self.draw_outline_batches(
                &mut pass,
                OutlineMode::Stencil,
                &self.stencil_mask_pipeline,
                false,
            );
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("stencil outline geometry pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.scene_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: None,
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_stencil_reference(1);
        self.draw_outline_batches(
            &mut pass,
            OutlineMode::Stencil,
            &self.stencil_outline_pipeline,
            true,
        );
    }

    pub(super) fn encode_outline_composite_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("outline composite pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.eframe_scene.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.clear_color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.outline_composite_pipeline);
        pass.set_bind_group(0, &self.outline_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    pub(super) fn submit_scene(&self) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("eframe scene command encoder"),
            });
        self.encode_shadow_pass(&mut encoder);
        self.encode_scene_pass(
            &mut encoder,
            &self.scene_view,
            &self.depth_view,
            self.clear_color,
        );
        self.encode_outline_mask_pass(&mut encoder);
        self.encode_stencil_outline_passes(&mut encoder);
        self.encode_outline_composite_pass(&mut encoder);
        self.queue.submit(Some(encoder.finish()));
    }

    pub(super) fn record_frame(&mut self) {
        self.frame_count += 1;
        let elapsed = self.fps_timer.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            self.fps = self.frame_count as f32 / elapsed;
            self.frame_count = 0;
            self.fps_timer = Instant::now();
        }
    }
}

fn pixel_readback_layout(width: u32, height: u32) -> Result<(u64, u64, u64), RendererError> {
    let unpadded_bytes_per_row = u64::from(width) * 4;
    let alignment = u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let padded_bytes_per_row = unpadded_bytes_per_row
        .div_ceil(alignment)
        .checked_mul(alignment)
        .ok_or_else(|| RendererError::PixelReadback("row size overflow".to_owned()))?;
    let buffer_size = padded_bytes_per_row
        .checked_mul(u64::from(height))
        .ok_or_else(|| RendererError::PixelReadback("buffer size overflow".to_owned()))?;
    Ok((unpadded_bytes_per_row, padded_bytes_per_row, buffer_size))
}

fn decode_pixel_readback(
    mapped: &[u8],
    width: u32,
    height: u32,
    unpadded_bytes_per_row: u64,
    padded_bytes_per_row: u64,
    format: wgpu::TextureFormat,
) -> Vec<[u8; 4]> {
    let row_bytes = unpadded_bytes_per_row as usize;
    let mut pixels = Vec::with_capacity(width as usize * height as usize);
    for row in mapped.chunks_exact(padded_bytes_per_row as usize) {
        for pixel in row[..row_bytes].chunks_exact(4) {
            let pixel: [u8; 4] = pixel.try_into().expect("one RGBA8 pixel is four bytes");
            pixels.push(match format {
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
                    [pixel[2], pixel[1], pixel[0], pixel[3]]
                }
                _ => pixel,
            });
        }
    }
    pixels
}
