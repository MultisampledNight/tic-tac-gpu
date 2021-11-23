use {
    thiserror::Error,
    wgpu::util::DeviceExt,
    winit::{
        dpi,
        event::{Event, WindowEvent},
        event_loop::ControlFlow,
        window::Window,
    },
};

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("Could not find any suitable GPU adapter")]
    NoSuitableAdapter,
    #[error("Could not request device: {0}")]
    RequestDeviceError(#[from] wgpu::RequestDeviceError),
}

#[derive(Debug, Error)]
enum BackendDrawError {
    #[error("Outdated or lost surface, needs to be reconfigured")]
    SurfaceOutdated,
    #[error(transparent)]
    SurfaceTextureError(wgpu::SurfaceError),
}

impl From<wgpu::SurfaceError> for BackendDrawError {
    fn from(source: wgpu::SurfaceError) -> Self {
        use wgpu::SurfaceError::*;
        match source {
            Outdated | Lost => Self::SurfaceOutdated,
            e => Self::SurfaceTextureError(e),
        }
    }
}

pub struct Backend {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    pipeline: wgpu::RenderPipeline,

    cross: Shape,

    window_size: dpi::PhysicalSize<u32>,
}

impl Backend {
    /// Creates a new backend for drawing stuff.
    ///
    /// # Safety
    ///
    /// The given [`winit::window::Window`] must live as long as the returned backend.
    pub async unsafe fn new(window: &Window) -> Result<Self, BackendError> {
        // The instance is the main starting point for everything in wgpu, there is no need to
        // "keep it alive" though (see the docs). We also need it only for surface and adapter
        // creation
        let instance = wgpu::Instance::new(wgpu::Backends::all());

        let surface = unsafe { instance.create_surface(window) }; // SAFETY: delegated to the caller

        // An adapter can be seen as a virtual handle to a physical graphics card or whatever that
        // might be
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or(BackendError::NoSuitableAdapter)?;

        let surface_format = surface.get_preferred_format(&adapter).unwrap(); // won't fail as no adapter can be found then

        // The device however refers to one specific API of a such graphics card. So if your card
        // supports, let's say, Vulkan and OpenGL ES, an adapter would refer to the card itself
        // while the device might refer to the Vulkan API of this card.
        //
        // And about the queue, you can imagine it as a conveyor belt which "slowly" flows towards
        // the GPU while trying to use space as useful as possible. That conveyor belt can contain
        // textures, cool buffers, but most importantly *sparkles* render commands *sparkles*.
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::downlevel_webgl2_defaults(),
                },
                None,
            )
            .await?;
        // Generates an underlying structure for the surface to be ready to be drawn onto. If you
        // don't do that, prepare for panics. I don't know why wgpu does not require this already
        // on setup though.
        let window_size = window.inner_size();
        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: window_size.width,
                height: window_size.height,
                present_mode: wgpu::PresentMode::Fifo,
            },
        );

        // Shaders are small programs running on the GPU. In normal applications, you usually only
        // use:
        //
        // - Vertex shaders: Run per every vertex, get all of their data, and transform it. The
        //                   fragment shader then gets the interpolated result.
        // - Fragment shaders: Run per every "fragment" and set the final color for it. A fragment
        //                     is basically a pixel, but it might be still hidden by something in
        //                     front of it.
        //                     DX calls them pixel shaders because of that, in case that helps.
        //
        // The only other shader types I know are compute and geometry shaders, but they are for
        // more special cases. uwu.
        let shader = device.create_shader_module(&wgpu::include_wgsl!("shader.wgsl"));

        // Render pipelines and their layout define one "way" of how to handle rendering. "Way" as
        // in, one run to the GPU, through the vertex shader, fragment shader, and all the other
        // magic things that transform a few buffers to a wonderful pixel surface. You can
        // have multiple of them with ease, which allows you to have different shaders, rendering
        // modes and antialiasing methods.
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vertex_main",
                buffers: &[
                    // A vertex buffer layout, as the name says, tells about how data in this buffer is to be
                    // interpreted. In this case we have two components, position and color, while the position
                    // is 2 f32 and the color 4 f32, following after each other.
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 0,
                                shader_location: 0,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: bytemuck::offset_of!(Vertex, color) as wgpu::BufferAddress,
                                shader_location: 1,
                            },
                        ],
                    },
                ],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                clamp_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fragment_main",
                targets: &[wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::all(),
                }],
            }),
        });

        let cross = Shape::cross(&device);

        Ok(Self {
            adapter,
            device,
            queue,
            surface,
            pipeline,
            cross,
            window_size,
        })
    }

    fn reconfigure_surface(&mut self) {
        // reconfiguring the surface is enough for the underlying structures to be recalculated
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.surface.get_preferred_format(&self.adapter).unwrap(),
                width: self.window_size.width,
                height: self.window_size.height,
                present_mode: wgpu::PresentMode::Fifo,
            },
        );
    }

    fn draw(&mut self) -> Result<(), BackendDrawError> {
        // We first have to tell the surface we want to have a fresh new frame to render to.
        let next_frame = self.surface.get_current_texture()?;

        // You can see a view as an actual "view" on the texture. It's possible to see something
        // from a different angle or at another daylight. Here you have much less options though.
        let next_frame_view = next_frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                label: None,
                // might seem pointless, but I want to ensure the format is Some
                format: Some(self.surface.get_preferred_format(&self.adapter).unwrap()),
                dimension: Some(wgpu::TextureViewDimension::D2),
                ..Default::default()
            });

        // A command encoder is comparable to a recorder: You say some things and these things can
        // be heared in the same order later on. Same with the command encoder, just that it
        // doesn't record voice but rather render *commands* (also compute commands, but I
        // currently don't care about these and they are for more specific purposes) for the GPU to
        // execute.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Now that we finished the setup stuff, let's actually draw stuff.
        self.cross
            .draw(&mut encoder, &self.pipeline, &next_frame_view);

        // Now that we're done recording what we want to do for now, we have to tell the
        // CommandEncoder to stop recording and place our resulting CommandBuffer on the conveyor
        // belt to the GPU.
        self.queue.submit(std::iter::once(encoder.finish()));

        // And finally, tell the surface texture for the next frame we're done with drawing to it,
        // it can "present" itself to the world now.
        next_frame.present();
        Ok(())
    }
}

impl super::HandleEvent for Backend {
    fn handle(&mut self, event: Event<()>, flow: &mut ControlFlow) {
        // handle only basic stuff such as quitting directly, forward everything else
        match event {
            // omitting window id checking since we only create one window
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *flow = ControlFlow::Exit,
                // window is unresizable, but who knows what great ideas a WM might have
                WindowEvent::Resized(new_inner_size) => {
                    self.window_size = new_inner_size;
                    self.reconfigure_surface();
                }
                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    self.window_size = *new_inner_size;
                    self.reconfigure_surface();
                }
                _ => (),
            },
            Event::RedrawRequested(_) => match self.draw() {
                Err(BackendDrawError::SurfaceOutdated) => self.reconfigure_surface(),
                Err(e) => {
                    log::error!("Error while drawing: {}", e);
                    *flow = ControlFlow::Exit;
                }
                _ => (),
            },
            _ => (),
        }
    }
}

#[repr(C)]
#[derive(Default, Debug, Copy, Clone)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

unsafe impl bytemuck::Zeroable for Vertex {}
unsafe impl bytemuck::Pod for Vertex {}

macro_rules! vertices {
    (color: { r: $r:expr, g: $g:expr, b: $b:expr $(,)? }, position: [ $( $x:expr, $y:expr $(,)? );+ $(;)? ]) => {
        vec![$(
            Vertex { position: [$x, $y], color: [$r, $g, $b, 1.0] },
        )*]
    };
}

#[derive(Debug)]
struct Shape {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

impl Shape {
    /// Allocates the given shape on the GPU. Has to be drawn to be seen.
    fn new(device: &wgpu::Device, vertices: Vec<Vertex>, indices: Vec<u16>) -> Self {
        // Buffers in general are comparable to dynamically sized arrays, like vec![3, 12, 5, 2]
        // would be. But they are a bit more complicated, by that I mean you can control how a
        // buffer is allowed to be used, or change how it's data is to be interpreted (which is...
        // quite rare, but can happen).
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertices: vertex_buffer,
            indices: index_buffer,
            index_count: indices.len() as u32,
        }
    }

    /// Draws this shape by creating a new render pass.
    ///
    /// The pipeline defines how the vertices contained by this shape are to be interpreted, e.g.
    /// if as lines, triangles, triangle strips...
    fn draw<'a>(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        target_view: &wgpu::TextureView,
    ) {
        // Render passes are like one thing to do when rendering stuff on the screen. They take one
        // "shape" (vertex buffers + one index buffer) , instance them as needed, and are then
        // given to the encoder to take care of it.
        // Note that the render pass is written into the encoder when dropping it, so we don't need
        // to consume it or anything.
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[wgpu::RenderPassColorAttachment {
                view: &target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.14,
                        b: 0.14,
                        a: 1.0,
                    }),
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });

        render_pass.set_pipeline(&pipeline);
        render_pass.set_vertex_buffer(0, self.vertices.slice(..));
        render_pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

/// Pre-defined shapes.
impl Shape {
    /// Creates a new cross-like shape.
    #[rustfmt::skip]
    fn cross(device: &wgpu::Device) -> Self {
        Self::new(
            device,
            vertices! {
                color: { r: 0.27, g: 0.87, b: 0.7 },
                position: [
                    -0.25, 0.25;
                    -0.2, 0.15;
                    -0.15, 0.2;

                    0.25, 0.25;
                    0.2, 0.15;
                    0.15, 0.2;

                    0.25, -0.25;
                    0.2, -0.15;
                    0.15, -0.2;

                    -0.25, -0.25;
                    -0.2, -0.15;
                    -0.15, -0.2;
                ]
            },
            vec![
                // corners
                1, 2, 0,
                3, 5, 4,
                6, 7, 8,
                9, 11, 10,

                // "bridges"
                1, 8, 7,
                7, 2, 1,

                5, 10, 11,
                11, 4, 5,
            ],
        )
    }
}
