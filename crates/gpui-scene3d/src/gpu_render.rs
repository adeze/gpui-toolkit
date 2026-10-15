//! GPU-resident surface rendering through wgpu.
//!
//! A [`GpuScene`] uploads every actor's prepared chunks (positions,
//! normals, scalars, indices) and colour LUT once. Afterwards, drawing a
//! frame only rewrites the 288-byte per-actor uniform block (camera and
//! clip planes): geometry never returns to the CPU, which is the whole
//! point on Metal / CUDA-class adapters. Hosts present the target texture
//! themselves (surface or GPUI custom draw); [`GpuRenderer::read_rgba`]
//! exists only for headless QA and movie export.

// Rust guideline compliant 2026-02-21

use crate::camera::{Camera, multiply};
use crate::prepare::prepare_chunks;
use crate::scene::Scene;
use crate::shaders::{SURFACE_WGSL, scene_uniform_bytes};
use crate::{Frame, SceneError};
use wgpu::util::DeviceExt;

/// Failures reported by the GPU renderer.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GpuRenderError {
    /// No GPU adapter is available on this machine.
    #[error("no wgpu adapter available")]
    NoAdapter,
    /// The device could not be created.
    #[error("wgpu device request failed: {0}")]
    Device(String),
    /// Scene preparation failed.
    #[error(transparent)]
    Scene(#[from] SceneError),
}

struct GpuChunkBuffers {
    vertices: [wgpu::Buffer; 3],
    index: wgpu::Buffer,
    index_count: u32,
    origin: [f64; 3],
}

struct GpuActor {
    chunks: Vec<GpuChunkBuffers>,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    scalar_range: [f32; 2],
    opacity: f32,
    clip_planes: Vec<crate::scene::ClipPlane>,
    transparent: bool,
    distance: f32,
}

/// A scene whose geometry lives entirely in GPU buffers.
pub struct GpuScene {
    actors: Vec<GpuActor>,
    background: [f32; 4],
    /// Total bytes of uploaded geometry (positions, normals, scalars,
    /// indices) — proof of residency for diagnostics.
    pub geometry_bytes: u64,
}

/// An offscreen render target owned by the renderer.
pub struct GpuTarget {
    color: wgpu::Texture,
    depth: wgpu::Texture,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// wgpu surface renderer: one device, pipelines per colour format.
pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline_opaque: wgpu::RenderPipeline,
    pipeline_transparent: wgpu::RenderPipeline,
    bind_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    format: wgpu::TextureFormat,
}

impl GpuRenderer {
    /// Creates a renderer for `format` (the surface or target format).
    ///
    /// # Errors
    ///
    /// Returns [`GpuRenderError::NoAdapter`] when no adapter exists.
    pub fn new(format: wgpu::TextureFormat) -> Result<Self, GpuRenderError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            ..Default::default()
        }))
        .map_err(|_| GpuRenderError::NoAdapter)?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gpui-scene3d renderer"),
            ..Default::default()
        }))
        .map_err(|e| GpuRenderError::Device(e.to_string()))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpui-scene3d surface"),
            source: wgpu::ShaderSource::Wgsl(SURFACE_WGSL.into()),
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpui-scene3d uniforms"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D1,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpui-scene3d pipeline"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let vertex_layouts = [
            wgpu::VertexBufferLayout {
                array_stride: 12,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                }],
            },
            wgpu::VertexBufferLayout {
                array_stride: 12,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 1,
                }],
            },
            wgpu::VertexBufferLayout {
                array_stride: 4,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 0,
                    shader_location: 2,
                }],
            },
        ];
        let make_pipeline = |depth_write: bool, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("gpui-scene3d surface pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &vertex_layouts,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gpui-scene3d lut sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Ok(Self {
            pipeline_opaque: make_pipeline(true, None),
            pipeline_transparent: make_pipeline(false, Some(wgpu::BlendState::ALPHA_BLENDING)),
            device,
            queue,
            bind_layout,
            sampler,
            format,
        })
    }

    /// The underlying device, for hosts sharing the wgpu context.
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Uploads a scene's geometry and LUTs; called once per scene change.
    ///
    /// # Errors
    ///
    /// Returns [`SceneError`] when an actor references a missing field.
    pub fn upload(&self, scene: &Scene) -> Result<GpuScene, GpuRenderError> {
        let mut actors = Vec::new();
        let mut geometry_bytes = 0u64;
        for actor in scene.actors.iter().filter(|a| a.visible) {
            let chunks = prepare_chunks(actor, crate::prepare::DEFAULT_MAX_TRIANGLES)?;
            let mut gpu_chunks = Vec::with_capacity(chunks.len());
            for chunk in &chunks {
                geometry_bytes += chunk.byte_len();
                let make = |data: &[f32], label: &str| {
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(label),
                            contents: bytemuck::cast_slice(data),
                            usage: wgpu::BufferUsages::VERTEX,
                        })
                };
                gpu_chunks.push(GpuChunkBuffers {
                    vertices: [
                        make(&chunk.positions, "positions"),
                        make(&chunk.normals, "normals"),
                        make(&chunk.scalars, "scalars"),
                    ],
                    index: self
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("indices"),
                            contents: bytemuck::cast_slice(&chunk.indices),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                    index_count: chunk.indices.len() as u32,
                    origin: chunk.origin,
                });
            }
            // Colour LUT: the actor's transfer function, or a flat ramp.
            let (lut_bytes, scalar_range) =
                if let (Some(transfer), Some(_)) = (&actor.transfer, actor.scalars()) {
                    (
                        transfer.lut_bytes(),
                        [transfer.range[0] as f32, transfer.range[1] as f32],
                    )
                } else {
                    let mut bytes = Vec::with_capacity(256 * 4);
                    for _ in 0..256 {
                        bytes.extend_from_slice(&[
                            (actor.color[0] * 255.0) as u8,
                            (actor.color[1] * 255.0) as u8,
                            (actor.color[2] * 255.0) as u8,
                            (actor.color[3] * 255.0) as u8,
                        ]);
                    }
                    (bytes, [0.0, 1.0])
                };
            let lut = self.device.create_texture_with_data(
                &self.queue,
                &wgpu::TextureDescriptor {
                    label: Some("gpui-scene3d lut"),
                    size: wgpu::Extent3d {
                        width: 256,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D1,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::wgt::TextureDataOrder::LayerMajor,
                &lut_bytes,
            );
            let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gpui-scene3d uniforms"),
                size: 288,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gpui-scene3d actor"),
                layout: &self.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &lut.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            actors.push(GpuActor {
                chunks: gpu_chunks,
                uniform,
                bind_group,
                scalar_range,
                opacity: actor.opacity,
                clip_planes: actor.clip_planes.clone(),
                transparent: actor.is_transparent(),
                distance: 0.0,
            });
        }
        Ok(GpuScene {
            actors,
            background: scene.background,
            geometry_bytes,
        })
    }

    /// Creates an offscreen colour+depth target.
    #[must_use]
    pub fn create_target(&self, width: u32, height: u32) -> GpuTarget {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpui-scene3d target"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpui-scene3d depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        GpuTarget {
            color,
            depth,
            width,
            height,
        }
    }

    /// Draws an uploaded scene into a target. Only uniform buffers are
    /// rewritten; geometry buffers are untouched (GPU-resident).
    pub fn render(&self, scene: &mut GpuScene, camera: &Camera, target: &GpuTarget) {
        let aspect = target.width as f32 / target.height.max(1) as f32;
        let view_projection = camera.view_projection(aspect);
        let eye = camera.eye();
        for actor in &mut scene.actors {
            // Distance for transparent sorting: chunk origins are all the
            // dataset centre, so use the first chunk's origin.
            if let Some(first) = actor.chunks.first() {
                let o = first.origin;
                actor.distance = ((o[0] as f32 - eye[0]).powi(2)
                    + (o[1] as f32 - eye[1]).powi(2)
                    + (o[2] as f32 - eye[2]).powi(2))
                .sqrt();
            }
        }
        for actor in &scene.actors {
            if let Some(first) = actor.chunks.first() {
                let translation = [
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                    first.origin[0] as f32,
                    first.origin[1] as f32,
                    first.origin[2] as f32,
                    1.0,
                ];
                let mvp = multiply(view_projection, translation);
                let bytes = scene_uniform_bytes(
                    &mvp,
                    actor.scalar_range,
                    actor.opacity,
                    &actor.clip_planes,
                );
                self.queue.write_buffer(&actor.uniform, 0, &bytes);
            }
        }
        let mut order: Vec<usize> = (0..scene.actors.len()).collect();
        order.sort_by_key(|&i| {
            let actor = &scene.actors[i];
            if actor.transparent {
                // Opaque first, then transparent back-to-front.
                (1, -ordered_float(actor.distance))
            } else {
                (0, 0)
            }
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpui-scene3d frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gpui-scene3d pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.color.create_view(&Default::default()),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(scene.background[0]),
                            g: f64::from(scene.background[1]),
                            b: f64::from(scene.background[2]),
                            a: f64::from(scene.background[3]),
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth.create_view(&Default::default()),
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
            for &index in &order {
                let actor = &scene.actors[index];
                pass.set_pipeline(if actor.transparent {
                    &self.pipeline_transparent
                } else {
                    &self.pipeline_opaque
                });
                pass.set_bind_group(0, &actor.bind_group, &[]);
                for chunk in &actor.chunks {
                    for (slot, buffer) in chunk.vertices.iter().enumerate() {
                        pass.set_vertex_buffer(slot as u32, buffer.slice(..));
                    }
                    pass.set_index_buffer(chunk.index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..chunk.index_count, 0, 0..1);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// Reads a target back to a CPU frame (QA and movie export only).
    ///
    /// # Errors
    ///
    /// Returns [`GpuRenderError::Device`] when the readback fails.
    pub fn read_rgba(&self, target: &GpuTarget) -> Result<Frame, GpuRenderError> {
        let bytes_per_row = target.width * 4;
        let padded = bytes_per_row.div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpui-scene3d readback"),
            size: u64::from(padded * target.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpui-scene3d readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(target.height),
                },
            },
            wgpu::Extent3d {
                width: target.width,
                height: target.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(10)),
            })
            .map_err(|e| GpuRenderError::Device(e.to_string()))?;
        receiver
            .recv()
            .map_err(|e| GpuRenderError::Device(e.to_string()))?
            .map_err(|e| GpuRenderError::Device(e.to_string()))?;
        let data = slice.get_mapped_range();
        let mut rgba = Vec::with_capacity((target.width * target.height * 4) as usize);
        for row in 0..target.height as usize {
            let start = row * padded as usize;
            rgba.extend_from_slice(&data[start..start + bytes_per_row as usize]);
        }
        drop(data);
        buffer.unmap();
        // Swizzle BGRA targets to RGBA.
        if self.format == wgpu::TextureFormat::Bgra8Unorm
            || self.format == wgpu::TextureFormat::Bgra8UnormSrgb
        {
            for px in rgba.as_chunks_mut::<4>().0 {
                px.swap(0, 2);
            }
        }
        Ok(Frame {
            width: target.width,
            height: target.height,
            rgba,
        })
    }
}

fn ordered_float(value: f32) -> i64 {
    // Reverse-sort key helper: distances are finite scene scales.
    (value * 1000.0) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Actor, TransferFunction};
    use gpui_mesh::{DataSet, Field};

    #[test]
    fn gpu_render_matches_scene_and_updates_by_uniform_only() {
        let Ok(renderer) = GpuRenderer::new(wgpu::TextureFormat::Rgba8Unorm) else {
            eprintln!("gpu render test skipped: no adapter");
            return;
        };
        let dataset = DataSet::structured_grid("g", [6, 6, 6], [-1.0; 3], [0.4; 3])
            .unwrap()
            .with_field(Field::scalar(
                "p",
                (0..216).map(|i| i as f64 / 216.0).collect::<Vec<_>>(),
            ));
        let scene = Scene::new().with_actor(
            Actor::new(dataset).coloured_by("p", TransferFunction::viridis([0.0, 1.0])),
        );
        let camera = Camera::fitted(scene.bounds().unwrap());
        let mut gpu_scene = renderer.upload(&scene).unwrap();
        assert!(gpu_scene.geometry_bytes > 0);
        let target = renderer.create_target(96, 96);
        renderer.render(&mut gpu_scene, &camera, &target);
        let frame = renderer.read_rgba(&target).unwrap();
        let centre = frame.rgba[((48 * 96 + 48) * 4) as usize..][..3].to_vec();
        assert_ne!(centre, vec![20, 23, 28], "centre shows the surface");

        // Orbit and re-render: same buffers, different picture.
        let mut orbited = camera;
        orbited.orbit(1.2, 0.3);
        renderer.render(&mut gpu_scene, &orbited, &target);
        let frame2 = renderer.read_rgba(&target).unwrap();
        let changed = frame
            .rgba
            .iter()
            .zip(frame2.rgba.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed > 1_000, "orbit changes the frame ({changed})");
    }
}
