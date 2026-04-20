use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use rustc_hash::FxHashMap;
use std::{mem, sync::Arc, time::Instant};
use tracing::debug;
use voxel_core::{
    BlockCoord, CameraTransform, ChunkCoord, FrameBudget, FrameMetrics, RenderConfig, WorkPhase,
    CHUNK_HEIGHT, CHUNK_SIZE_X, CHUNK_SIZE_Z,
};
use voxel_world::{BlockId, Chunk, World, AIR, DIRT, GRASS, STONE};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

const SHADER: &str = include_str!("shader.wgsl");

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Face {
    Left,
    Right,
    Bottom,
    Top,
    Back,
    Front,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaceVertex {
    pub position: [i32; 3],
    pub block_id: BlockId,
    pub face: Face,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkMesh {
    pub chunk: ChunkCoord,
    pub vertices: Vec<FaceVertex>,
    pub visible_faces: u32,
}

impl ChunkMesh {
    pub fn build(chunk: &Chunk) -> Self {
        let mut vertices = Vec::new();

        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_SIZE_Z {
                for x in 0..CHUNK_SIZE_X {
                    let block_id = chunk.storage.get(x, y, z);
                    if block_id == AIR {
                        continue;
                    }

                    let world = chunk.coord.world_origin();
                    let wx = world.x + x;
                    let wz = world.z + z;

                    push_face_if_visible(chunk, &mut vertices, wx, y, wz, x - 1, y, z, block_id, Face::Left);
                    push_face_if_visible(chunk, &mut vertices, wx, y, wz, x + 1, y, z, block_id, Face::Right);
                    push_face_if_visible(chunk, &mut vertices, wx, y, wz, x, y - 1, z, block_id, Face::Bottom);
                    push_face_if_visible(chunk, &mut vertices, wx, y, wz, x, y + 1, z, block_id, Face::Top);
                    push_face_if_visible(chunk, &mut vertices, wx, y, wz, x, y, z - 1, block_id, Face::Back);
                    push_face_if_visible(chunk, &mut vertices, wx, y, wz, x, y, z + 1, block_id, Face::Front);
                }
            }
        }

        Self {
            chunk: chunk.coord,
            visible_faces: vertices.len() as u32,
            vertices,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub transform: CameraTransform,
    pub aspect_ratio: f32,
    pub fov_degrees: f32,
    pub near_plane: f32,
    pub far_plane: f32,
    pub view_distance: i32,
}

impl Camera {
    pub fn from_transform(transform: CameraTransform, config: &RenderConfig, size: PhysicalSize<u32>, view_distance: i32) -> Self {
        Self {
            transform,
            aspect_ratio: (size.width.max(1) as f32) / (size.height.max(1) as f32),
            fov_degrees: config.fov_degrees,
            near_plane: config.near_plane,
            far_plane: config.far_plane,
            view_distance,
        }
    }

    pub fn view_projection(self) -> Mat4 {
        let view = self.transform.view_matrix();
        let projection = Mat4::perspective_rh_gl(
            self.fov_degrees.to_radians(),
            self.aspect_ratio,
            self.near_plane,
            self.far_plane,
        );
        projection * view
    }
}

#[derive(Debug)]
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    chunk_meshes: FxHashMap<ChunkCoord, GpuChunkMesh>,
}

#[derive(Debug)]
struct GpuChunkMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuVertex {
    position: [f32; 3],
    color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_projection: [[f32; 4]; 4],
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
            .map_err(|error| format!("failed to request adapter: {error}"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("voxel-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::default(),
            })
            .await
            .map_err(|error| format!("failed to request device: {error}"))?;

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
                    array_stride: mem::size_of::<GpuVertex>() as u64,
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

        let depth_view = create_depth_view(&device, &config);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            camera_buffer,
            camera_bind_group,
            depth_view,
            chunk_meshes: FxHashMap::default(),
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

            if self.chunk_meshes.contains_key(coord) {
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
                world.mark_chunk_resident(coord);
            }
            metrics.record_phase(WorkPhase::Upload, upload_started.elapsed());
        }

        debug!(cached_meshes = self.chunk_meshes.len(), "renderer world sync complete");
    }

    pub fn render(&mut self, camera: &Camera) -> Result<(), wgpu::SurfaceError> {
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
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    pub fn uploaded_meshes(&self) -> usize {
        self.chunk_meshes.len()
    }
}

impl GpuChunkMesh {
    fn from_chunk_mesh(device: &wgpu::Device, mesh: &ChunkMesh) -> Option<Self> {
        if mesh.vertices.is_empty() {
            return None;
        }

        let mut vertices = Vec::with_capacity(mesh.vertices.len() * 4);
        let mut indices = Vec::with_capacity(mesh.vertices.len() * 6);

        for (face_index, face) in mesh.vertices.iter().enumerate() {
            let base_index = (face_index * 4) as u32;
            let face_vertices = face_quad_vertices(*face);
            vertices.extend_from_slice(&face_vertices);
            indices.extend_from_slice(&[
                base_index,
                base_index + 1,
                base_index + 2,
                base_index,
                base_index + 2,
                base_index + 3,
            ]);
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("chunk-vertex-buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("chunk-index-buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Some(Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
        })
    }
}

fn create_depth_view(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth-texture"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn face_quad_vertices(face: FaceVertex) -> [GpuVertex; 4] {
    let [x, y, z] = face.position;
    let x = x as f32;
    let y = y as f32;
    let z = z as f32;
    let color = face_color(face.block_id, face.face);

    match face.face {
        Face::Left => [
            GpuVertex { position: [x, y, z], color },
            GpuVertex { position: [x, y, z + 1.0], color },
            GpuVertex { position: [x, y + 1.0, z + 1.0], color },
            GpuVertex { position: [x, y + 1.0, z], color },
        ],
        Face::Right => [
            GpuVertex { position: [x + 1.0, y, z + 1.0], color },
            GpuVertex { position: [x + 1.0, y, z], color },
            GpuVertex { position: [x + 1.0, y + 1.0, z], color },
            GpuVertex { position: [x + 1.0, y + 1.0, z + 1.0], color },
        ],
        Face::Bottom => [
            GpuVertex { position: [x, y, z + 1.0], color },
            GpuVertex { position: [x, y, z], color },
            GpuVertex { position: [x + 1.0, y, z], color },
            GpuVertex { position: [x + 1.0, y, z + 1.0], color },
        ],
        Face::Top => [
            GpuVertex { position: [x, y + 1.0, z], color },
            GpuVertex { position: [x, y + 1.0, z + 1.0], color },
            GpuVertex { position: [x + 1.0, y + 1.0, z + 1.0], color },
            GpuVertex { position: [x + 1.0, y + 1.0, z], color },
        ],
        Face::Back => [
            GpuVertex { position: [x + 1.0, y, z], color },
            GpuVertex { position: [x, y, z], color },
            GpuVertex { position: [x, y + 1.0, z], color },
            GpuVertex { position: [x + 1.0, y + 1.0, z], color },
        ],
        Face::Front => [
            GpuVertex { position: [x, y, z + 1.0], color },
            GpuVertex { position: [x + 1.0, y, z + 1.0], color },
            GpuVertex { position: [x + 1.0, y + 1.0, z + 1.0], color },
            GpuVertex { position: [x, y + 1.0, z + 1.0], color },
        ],
    }
}

fn face_color(block_id: BlockId, face: Face) -> [f32; 3] {
    let base = match block_id {
        GRASS => [0.29, 0.66, 0.18],
        DIRT => [0.51, 0.34, 0.18],
        STONE => [0.52, 0.52, 0.56],
        _ => [0.85, 0.25, 0.85],
    };

    let shade = match face {
        Face::Top => 1.1,
        Face::Bottom => 0.55,
        Face::Left | Face::Right => 0.85,
        Face::Back | Face::Front => 0.95,
    };

    [base[0] * shade, base[1] * shade, base[2] * shade]
}

fn push_face_if_visible(
    chunk: &Chunk,
    vertices: &mut Vec<FaceVertex>,
    world_x: i32,
    world_y: i32,
    world_z: i32,
    neighbor_x: i32,
    neighbor_y: i32,
    neighbor_z: i32,
    block_id: BlockId,
    face: Face,
) {
    if chunk.storage.get(neighbor_x, neighbor_y, neighbor_z) != AIR {
        return;
    }

    vertices.push(FaceVertex {
        position: [world_x, world_y, world_z],
        block_id,
        face,
    });
}

pub fn is_chunk_in_view(camera: &Camera, chunk: ChunkCoord) -> bool {
    let center = chunk.world_origin();
    let offset = Vec3::new(
        center.x as f32 + (CHUNK_SIZE_X / 2) as f32,
        (CHUNK_HEIGHT / 2) as f32,
        center.z as f32 + (CHUNK_SIZE_Z / 2) as f32,
    ) - camera.transform.position;

    offset.x * offset.x + offset.z * offset.z <= (camera.view_distance * camera.view_distance) as f32
}

pub fn sample_chunk_surface(world: &World, coord: ChunkCoord) -> Option<BlockCoord> {
    let chunk = world.loaded_chunk(coord)?;
    let origin = coord.world_origin();

    for y in (0..CHUNK_HEIGHT).rev() {
        for z in 0..CHUNK_SIZE_Z {
            for x in 0..CHUNK_SIZE_X {
                if chunk.storage.get(x, y, z) != AIR {
                    return Some(BlockCoord::new(origin.x + x, y, origin.z + z));
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxel_world::{Chunk, STONE};

    #[test]
    fn isolated_block_emits_six_faces() {
        let mut chunk = Chunk::new(ChunkCoord::new(0, 0));
        chunk.storage.set(1, 1, 1, STONE);

        let mesh = ChunkMesh::build(&chunk);
        assert_eq!(mesh.visible_faces, 6);
    }

    #[test]
    fn adjacent_blocks_cull_internal_faces() {
        let mut chunk = Chunk::new(ChunkCoord::new(0, 0));
        chunk.storage.set(1, 1, 1, STONE);
        chunk.storage.set(2, 1, 1, STONE);

        let mesh = ChunkMesh::build(&chunk);
        assert_eq!(mesh.visible_faces, 10);
    }

    #[test]
    fn gpu_conversion_emits_two_triangles_per_face() {
        let face = FaceVertex {
            position: [0, 0, 0],
            block_id: STONE,
            face: Face::Front,
        };

        let vertices = face_quad_vertices(face);
        assert_eq!(vertices.len(), 4);
    }
}
