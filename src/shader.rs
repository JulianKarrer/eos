use std::time::{Duration, Instant};

use cosmic::iced::wgpu::{BlendState, PipelineCompilationOptions};
use cosmic::iced::window::RedrawRequest;
use cosmic::iced_wgpu::graphics::Viewport;
use cosmic::iced::advanced::Shell;
use cosmic::iced::event::Status;
use cosmic::iced::mouse;
use cosmic::iced::mouse::Cursor;
use crate::config::Config;
use crate::iced::wgpu;
use crate::{get_term_bg_colour, Message};
use cosmic::iced::widget::shader::Event;
use cosmic::iced::widget::shader;
use cosmic::iced::Rectangle;

/// Milliseconds until next redraw of the fragment shader is requested
pub const FRAME_TIME:u64 = 33;

#[derive(Debug, Clone, Copy)]
struct Uniforms {
    time: f32,
    delta_time: Instant,
    cpu_util: f32,
    cpu_max: f32,
    cpu_freq: f32,
    bg: [f32;4],
}

#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct UniformsCRepr {
    resolution: [f32;2],
    top_left: [f32;2],
    time: f32,
    cpu_util: f32,
    cpu_max: f32,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

impl UniformsCRepr{
    /// Get the size of the structure in bytes. Used to create a uniform buffer
    fn size_in_bytes()-> usize {
        std::mem::size_of::<UniformsCRepr>() + std::mem::align_of::<UniformsCRepr>()
        // 48
    }
}

struct FragmentShaderPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
}

impl FragmentShaderPipeline {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, queue: &wgpu::Queue) -> Self {
        // create shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("FragmentShaderPipeline shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "shader.wgsl"
            ))),
        });

        // load texture
        let image_data = include_bytes!("../res/textures/earth_lights.jpg");
        let image = image::load_from_memory(image_data)
            .expect("Failed to load texture")
            .to_rgba8();
        let dimensions = image.dimensions();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("texture"),
            size: wgpu::Extent3d {
                width: dimensions.0,
                height: dimensions.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

         // upload texture data
         queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            image.as_raw(),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            wgpu::Extent3d {
                width: dimensions.0,
                height: dimensions.1,
                depth_or_array_layers: 1,
            },
        );

        // uniforms
        let uniform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                // uniforms entry
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // texture entry
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // sampler entry
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
            label: Some("uniform_bind_group_layout"),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform_buffer"),
            size: UniformsCRepr::size_in_bytes() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("uniform_bind_group"),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("FragmentShaderPipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState ::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });


        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group, 
        }
    }

    fn update(&mut self, queue: &wgpu::Queue, uniforms: &UniformsCRepr) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));
    }

    fn render(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        viewport: Rectangle<u32>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fill color test"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_viewport(
            viewport.x as f32,
            viewport.y as f32,
            viewport.width as f32,
            viewport.height as f32,
            0.0,
            1.0,
        );
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);

        pass.draw(0..3, 0..1);
    }
}



#[derive(Debug)]
pub struct FragmentShaderPrimitive {
    uniforms: Uniforms,
}

impl FragmentShaderPrimitive {
    fn new(uniforms: Uniforms) -> Self {
        Self { uniforms }
    }
}

impl shader::Primitive for FragmentShaderPrimitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        bounds: &cosmic::iced::Rectangle,
        _viewport: &Viewport,
    ) {
        if !storage.has::<FragmentShaderPipeline>() {
            storage.store(FragmentShaderPipeline::new(device, format, queue));
        }

        let pipeline = storage.get_mut::<FragmentShaderPipeline>().unwrap();
        let [r,g,b,a] = self.uniforms.bg;
        pipeline.update(
            queue,
            &UniformsCRepr {
                resolution: [bounds.width, bounds.height],
                top_left: [bounds.x, bounds.y],
                time: self.uniforms.time,
                r,g,b,a,
                cpu_util: self.uniforms.cpu_util,
                cpu_max: self.uniforms.cpu_max,
            },
        );
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let pipeline = storage.get::<FragmentShaderPipeline>().unwrap();
        pipeline.render(target, encoder, *clip_bounds);
    }
}



#[derive(Debug)]
pub struct FragmentShaderProgram {
    uniforms: Uniforms
}

impl FragmentShaderProgram{
    pub fn new(config:&Config)->Self{
        Self { 
            uniforms: Uniforms{ 
                time: 0., 
                delta_time: Instant::now(),
                bg: get_term_bg_colour(config),
                cpu_util: 0.,
                cpu_freq: 0.,
                cpu_max: 0.,
            } 
        }
    }

    /// To be called from `ResourceMonitor` at least once per visual update tick
    pub fn update_uniforms_tick(&mut self, cpu_util:f32, cpu_max:f32, cpu_freq:f32){
        self.uniforms.cpu_util = cpu_util;
        self.uniforms.cpu_max = cpu_max;
        self.uniforms.cpu_freq = cpu_freq;
        self.uniforms.time +=  self.uniforms.delta_time.elapsed().as_secs_f32() 
            * (self.uniforms.cpu_freq.clamp(0.0, 1.0).powi(2) * 0.5 + 0.5);
        self.uniforms.delta_time = Instant::now();
    }

    /// To be called when the background colour of the terminal theme changes
    pub fn update_bg(&mut self, config:&Config){
        self.uniforms.bg = get_term_bg_colour(config);
    }
}

impl shader::Program<Message> for FragmentShaderProgram {
    // type State = SomeEnum;
    type State = ();
    type Primitive = FragmentShaderPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        FragmentShaderPrimitive::new(self.uniforms)
    }

    fn update(
        &self,
        _state: &mut Self::State,
        _event: Event,
        _bounds: Rectangle,
        _cursor: Cursor,
        shell: &mut Shell<'_, Message>,
    ) -> (Status, Option<Message>) {
        shell.request_redraw(RedrawRequest::At(
            Instant::now()+Duration::from_millis(FRAME_TIME)
        ));
        // shell.request_redraw(RedrawRequest::NextFrame);
        (Status::Ignored, None)
    }
}

