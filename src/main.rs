mod render;

use {
    render::Backend,
    thiserror::Error,
    winit::{
        dpi,
        event::Event,
        event_loop::{ControlFlow, EventLoop},
        window::{Window, WindowBuilder},
    },
};

pub trait HandleEvent {
    fn handle(&mut self, event: Event<()>, flow: &mut ControlFlow);
}

#[derive(Debug, Error)]
enum AppError {
    #[error("Unable to create window: {0}")]
    WindowError(#[from] winit::error::OsError),
    #[error("Could not create backend: {0}")]
    BackendError(#[from] render::BackendError),
}

struct App {
    backend: Backend,

    // DO NOT REORDER THIS -- Safety of Backend::new depends on it
    _window: Window,
}

impl App {
    async fn new(event_loop: &EventLoop<()>) -> Result<Self, AppError> {
        let window = WindowBuilder::new()
            .with_title("Tic Tac GPU")
            .with_resizable(false)
            .with_inner_size(dpi::LogicalSize::new(400, 400))
            .build(&event_loop)?;
        // SAFETY: window is in the same struct as the backend and the window gets dropped after
        // the backend
        let backend = unsafe { Backend::new(&window) }.await?;

        Ok(Self {
            backend,
            _window: window,
        })
    }
}

impl HandleEvent for App {
    fn handle(&mut self, event: Event<()>, flow: &mut ControlFlow) {
        // Just forward, maybe it wants to do something with it as well
        self.backend.handle(event, flow);
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let event_loop = EventLoop::new();

    let mut app = App::new(&event_loop).await.unwrap_or_else(|e| {
        log::error!("{}", e);
        std::process::exit(1)
    });
    event_loop.run(move |event, _, flow| app.handle(event, flow));
}
