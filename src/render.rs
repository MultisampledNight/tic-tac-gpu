use {
    std::f32::consts::PI,
    thiserror::Error,
    ultraviolet::{rotor::Rotor2, vec::Vec2},
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
        match source {
            wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost => Self::SurfaceOutdated,
            e => Self::SurfaceTextureError(e),
        }
    }
}

/// All the information you need to know about a frame in order to render on it.
///
/// Has two different lifetimes as one time handles are expected to persist over several frames
/// such as a pipeline or a device, and one time things are really thought only for this frame,
/// such as a view on the next surface contents or a command encoder. Not that it would make a
/// usable difference.
struct Frame<'persist, 'frame> {
    encoder: &'frame mut wgpu::CommandEncoder,
    target_view: &'frame wgpu::TextureView,
    pipeline: &'persist wgpu::RenderPipeline,
}

pub struct Backend {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    pipeline: wgpu::RenderPipeline,

    cross: Shape,
    ring: Shape,

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

        Ok(Self {
            cross: Shape::cross(&device),
            ring: Shape::ring(&device),
            adapter,
            device,
            queue,
            surface,
            pipeline,
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

    fn clear_background(color: wgpu::Color, frame: &mut Frame<'_, '_>) {
        frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: frame.target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });
        // dropping is enough for the clear command to be recorded
    }

    fn draw(&mut self) -> Result<(), BackendDrawError> {
        // We first have to tell the surface we want to have a fresh new frame to render to.
        let next_frame_surface = self.surface.get_current_texture()?;

        // You can see a view as an actual "view" on the texture. It's possible to see something
        // from a different angle or at another daylight. Here you have much less options though.
        let next_frame_view =
            next_frame_surface
                .texture
                .create_view(&wgpu::TextureViewDescriptor {
                    label: None,
                    // might seem pointless, but I want to ensure the format is Some
                    format: Some(self.surface.get_preferred_format(&self.adapter).unwrap()),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    ..wgpu::TextureViewDescriptor::default()
                });

        // A command encoder is comparable to a recorder: You say some things and these things can
        // be heared in the same order later on. Same with the command encoder, just that it
        // doesn't record voice but rather render *commands* (also compute commands, but I
        // currently don't care about these and they are for more specific purposes) for the GPU to
        // execute.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Almost finished with setting up stuff, we pack all "rendering" stuff of interest into a
        // nice toolbox we can hand around to anything that wants to draw something.
        let mut next_frame = Frame {
            encoder: &mut encoder,
            target_view: &next_frame_view,
            pipeline: &self.pipeline,
        };

        // Now that we finished the setup stuff, let's actually draw stuff.
        Self::clear_background(
            wgpu::Color {
                r: 0.04,
                g: 0.09,
                b: 0.09,
                a: 1.0,
            },
            &mut next_frame,
        );
        self.cross.draw(&mut next_frame);
        self.ring.draw(&mut next_frame);

        // Now that we're done recording what we want to do for now, we have to tell the
        // CommandEncoder to stop recording and place our resulting CommandBuffer on the conveyor
        // belt to the GPU.
        self.queue.submit(std::iter::once(encoder.finish()));

        // And finally, tell the surface texture for the next frame we're done with drawing to it,
        // it can "present" itself to the world now.
        next_frame_surface.present();
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
        &[$(
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
    fn new(device: &wgpu::Device, vertices: &[Vertex], indices: &[u16]) -> Self {
        // Buffers in general are comparable to dynamically sized arrays, like vec![3, 12, 5, 2]
        // would be. But they are a bit more complicated, by that I mean you can control how a
        // buffer is allowed to be used, or change how it's data is to be interpreted (which is...
        // quite rare, but can happen).
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(indices),
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
    fn draw(&self, frame: &mut Frame<'_, '_>) {
        // Render passes are like one thing to do when rendering stuff on the screen. They take one
        // "shape" (vertex buffers + one index buffer) , instance them as needed, and are then
        // given to the encoder to take care of it.
        // Note that the render pass is written into the encoder when dropping it, so we don't need
        // to consume it or anything.
        let mut render_pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: frame.target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // TODO is that really ideal?
                        load: wgpu::LoadOp::Load,
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

        render_pass.set_pipeline(frame.pipeline);
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
            &[
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

    /// Creates a new ring-like shape with 48 vertices.
    #[rustfmt::skip]
    fn ring(device: &wgpu::Device) -> Shape {
        const CIRCLE_VERTEX_COUNT: u32 = 24;

        fn wrap_at_max(x: u32) -> u32 {
            x % (CIRCLE_VERTEX_COUNT * 2)
        }

        let mut vertices = Vec::with_capacity((CIRCLE_VERTEX_COUNT * 2) as usize);
        let mut indices = Vec::with_capacity((CIRCLE_VERTEX_COUNT * 6) as usize);

        // We configure the rotor once, then rotate the vector with it again and again and again...
        // ...until we've completed a circle movement and catched all the vertices we wanted to
        // have. Now let's go and push their DVs to make a perfect build. /hj
        let rotor = Rotor2::from_angle(PI * 2.0 / CIRCLE_VERTEX_COUNT as f32);
        let mut vector = Vec2::new(1.0, 0.0);

        for i in (0..CIRCLE_VERTEX_COUNT).into_iter().map(|x| x * 2) {
            vertices.push(Vertex { position: [vector.x * 0.15, vector.y * 0.15], color: [0.76, 0.3, 1.0, 1.0] });
            vertices.push(Vertex { position: [vector.x * 0.25, vector.y * 0.25], color: [0.76, 0.3, 1.0, 1.0] });

            // Might seem confusing, but let me explain:
            //
            //  3        1
            //   +------+
            //   |     / \
            //   +----+   \
            //  2    0 \   \
            //
            // (note the direction, we're going counter-clockwise, important for clipping)
            // In one loop iteration, we want to note down such a quad between the current vertex
            // pair and the next one. This can be accomplished by a triangle between 0, 1 and 2,
            // and one between 2, 1, 3. We need to wrap 2 and 3 at CIRCLE_VERTEX_COUNT though, as
            // we're constantly referring to the next pair: What if i is currently
            // CIRCLE_VERTEX_COUNT - 2?
            for x in [
                i, i + 1, wrap_at_max(i + 2),
                wrap_at_max(i + 2), i + 1, wrap_at_max(i + 3),
            ] {
                indices.push(x as u16);
            }

            rotor.rotate_vec(&mut vector);
        }

        Self::new(device, &vertices, &indices)
    }
}
