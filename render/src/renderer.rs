use crate::camera::Camera;
use crate::gpu::{create_depth_view, CameraUniform};
use crate::mesh::{is_chunk_in_view, GpuChunkMesh};
use crate::types::ChunkMesh;
use glyphon::{
    Attrs, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea, TextAtlas,
    TextRenderer, Viewport,
};
use rustc_hash::FxHashMap;
use std::{fmt, mem, sync::Arc, time::Instant};
use tracing::debug;
use voxel_core::{
    ChunkCoord, FrameBudget, FrameMetrics, RenderConfig, WorkPhase,
};
use voxel_world::World;
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

const SHADER: &str = include_str!("shader.wgsl");
const CROSSHAIR_SHADER: &str = include_str!("crosshair.wgsl");

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    crosshair_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    chunk_meshes: FxHashMap<ChunkCoord, GpuChunkMesh>,

    // Text rendering
    font_system: FontSystem,
    swash_cache: SwashCache,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,
    viewport: Viewport,

    // GPU Profiling
    query_set: Option<wgpu::QuerySet>,
    query_buffer: Option<wgpu::Buffer>,
    staging_query_buffer: Option<wgpu::Buffer>,
    timestamp_period: f32,
}

impl fmt::Debug for Renderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Renderer")
            .field("chunk_meshes", &self.chunk_meshes.len())
            .field("timestamp_period", &self.timestamp_period)
            .finish()
    }
}

impl Renderer {
    pub async fn new(window: Arc<Window>, render_config: &RenderConfig) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .map_err(|error| format!("failed to create surface: {error}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "failed to request adapter".to_string())?;

        let features = adapter.features();
        let timestamp_supported = features.contains(wgpu::Features::TIMESTAMP_QUERY);
        let timestamp_inside_supported = features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        
        let mut required_features = wgpu::Features::empty();
        if timestamp_supported {
            required_features |= wgpu::Features::TIMESTAMP_QUERY;
        }
        if timestamp_inside_supported {
            required_features |= wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        }

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("voxel-device"),
                    required_features,
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|error| format!("failed to request device: {error}"))?;

        let timestamp_enabled = timestamp_supported && timestamp_inside_supported;
        let timestamp_period = queue.get_timestamp_period();

        let query_set = timestamp_enabled.then(|| {
            device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("timestamp-query-set"),
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            })
        });

        let query_buffer = timestamp_enabled.then(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timestamp-query-buffer"),
                size: 16, // 2 * 8 bytes
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        });

        let staging_query_buffer = timestamp_enabled.then(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("staging-query-buffer"),
                size: 16,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });

        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);
        let present_mode = if render_config.vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        let alpha_mode = capabilities.alpha_modes[0];

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: render_config.max_frames_in_flight as u32,
        };
        surface.configure(&device, &config);

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera-buffer"),
            size: mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera-bind-group-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera-bind-group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxel-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("voxel-pipeline-layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("voxel-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<crate::gpu::GpuVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: mem::size_of::<[f32; 3]>() as u64,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let crosshair_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crosshair-shader"),
            source: wgpu::ShaderSource::Wgsl(CROSSHAIR_SHADER.into()),
        });

        let crosshair_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crosshair-pipeline-layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let crosshair_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("crosshair-pipeline"),
            layout: Some(&crosshair_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &crosshair_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &crosshair_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let depth_view = create_depth_view(&device, &config);

        // Glyphon initialization
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(&device);
        let mut text_atlas = TextAtlas::new(&device, &queue, &cache, format);
        let text_renderer =
            TextRenderer::new(&mut text_atlas, &device, wgpu::MultisampleState::default(), None);
        let viewport = Viewport::new(&device, &cache);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            crosshair_pipeline,
            camera_buffer,
            camera_bind_group,
            depth_view,
            chunk_meshes: FxHashMap::default(),
            font_system,
            swash_cache,
            text_atlas,
            text_renderer,
            viewport,
            query_set,
            query_buffer,
            staging_query_buffer,
            timestamp_period,
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, &self.config);
        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.config.width,
                height: self.config.height,
            },
        );
    }

    pub fn surface_size(&self) -> PhysicalSize<u32> {
        PhysicalSize::new(self.config.width, self.config.height)
    }

    pub fn sync_world(&mut self, world: &mut World, camera: &Camera, metrics: &mut FrameMetrics) {
        self.chunk_meshes
            .retain(|coord, _| world.loaded_chunk(*coord).is_some() && is_chunk_in_view(camera, *coord));

        let mut budget = FrameBudget::new(world.streaming.max_mesh_jobs_per_frame);
        let mut to_upload = Vec::new();

        for (coord, chunk) in world.loaded_chunks_iter() {
            if !is_chunk_in_view(camera, *coord) {
                continue;
            }

            let is_meshing = world.manager.state(*coord) == Some(voxel_world::ChunkState::Meshing);
            let not_in_gpu = !self.chunk_meshes.contains_key(coord);

            if !is_meshing && !not_in_gpu {
                continue;
            }

            if !budget.try_take(1) {
                break;
            }

            to_upload.push((*coord, chunk.clone()));
        }

        for (coord, chunk) in to_upload {
            let mesh_started = Instant::now();
            let mesh = ChunkMesh::build(&chunk);
            metrics.record_phase(WorkPhase::Mesh, mesh_started.elapsed());

            let upload_started = Instant::now();
            if let Some(gpu_mesh) = GpuChunkMesh::from_chunk_mesh(&self.device, &mesh) {
                self.chunk_meshes.insert(coord, gpu_mesh);
            } else {
                self.chunk_meshes.remove(&coord);
            }
            world.mark_chunk_resident(coord);
            metrics.record_phase(WorkPhase::Upload, upload_started.elapsed());
        }

        debug!(cached_meshes = self.chunk_meshes.len(), "renderer world sync complete");
    }

    pub fn render(&mut self, camera: &Camera, debug_text: Option<&str>) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform = CameraUniform {
            view_projection: camera.view_projection().to_cols_array_2d(),
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("voxel-render-encoder"),
            });

        if let Some(query_set) = &self.query_set {
            encoder.write_timestamp(query_set, 0);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("voxel-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.52,
                            g: 0.73,
                            b: 0.91,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);

            for mesh in self.chunk_meshes.values() {
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }

            pass.set_pipeline(&self.crosshair_pipeline);
            pass.draw(0..12, 0..1);
        }

        if let Some(query_set) = &self.query_set {
            encoder.write_timestamp(query_set, 1);
        }

        if let (Some(query_set), Some(query_buffer)) = (&self.query_set, &self.query_buffer) {
            encoder.resolve_query_set(query_set, 0..2, query_buffer, 0);
            if let Some(staging) = &self.staging_query_buffer {
                encoder.copy_buffer_to_buffer(query_buffer, 0, staging, 0, 16);
            }
        }

        if let Some(text) = debug_text {
            let text_renderer = &mut self.text_renderer;
            let font_system = &mut self.font_system;
            let atlas = &mut self.text_atlas;

            let mut buffer = glyphon::Buffer::new(font_system, Metrics::new(20.0, 25.0));
            buffer.set_size(font_system, Some(self.config.width as f32), Some(self.config.height as f32));
            buffer.set_text(font_system, text, Attrs::new().family(Family::Monospace), Shaping::Advanced);
            buffer.shape_until_scroll(font_system, false);

            text_renderer
                .prepare(
                    &self.device,
                    &self.queue,
                    font_system,
                    atlas,
                    &self.viewport,
                    [TextArea {
                        buffer: &buffer,
                        left: 10.0,
                        top: 10.0,
                        scale: 1.0,
                        bounds: glyphon::TextBounds {
                            left: 0,
                            top: 0,
                            right: self.config.width as i32,
                            bottom: self.config.height as i32,
                        },
                        default_color: Color::rgb(255, 255, 255),
                        custom_glyphs: &[],
                    }],
                    &mut self.swash_cache,
                )
                .unwrap();

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("text-render-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });

                text_renderer.render(atlas, &self.viewport, &mut pass).unwrap();
            }
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    pub fn retrieve_gpu_time(&self) -> f32 {
        let Some(buffer) = &self.staging_query_buffer else {
            return 0.0;
        };

        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());

        self.device.poll(wgpu::Maintain::Wait);

        if let Ok(Ok(())) = receiver.recv() {
            let data = slice.get_mapped_range();
            let timestamps: &[u64] = bytemuck::cast_slice(&data);
            let diff = timestamps[1].wrapping_sub(timestamps[0]);
            drop(data);
            buffer.unmap();
            (diff as f32 * self.timestamp_period) / 1_000_000.0
        } else {
            0.0
        }
    }

    pub fn uploaded_meshes(&self) -> usize {
        self.chunk_meshes.len()
    }
}