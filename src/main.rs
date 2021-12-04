mod render;

use {
    render::Backend,
    thiserror::Error,
    winit::{
        dpi,
        event::{Event, MouseButton, WindowEvent},
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Cell {
    Cross,
    Ring,
    Empty,
}

struct App {
    board: [Cell; 9],
    backend: Backend,

    // DO NOT REORDER THIS -- Safety of Backend::new depends on it
    window: Window,
}

impl App {
    async fn new(event_loop: &EventLoop<()>) -> Result<Self, AppError> {
        let window = WindowBuilder::new()
            .with_title("Tic Tac GPU")
            .with_resizable(false)
            .with_inner_size(dpi::LogicalSize::new(400, 400))
            .build(event_loop)?;
        // SAFETY: window is in the same struct as the backend and the window gets dropped after
        // the backend
        let backend = unsafe { Backend::new(&window) }.await?;

        Ok(Self {
            board: [Cell::Empty; 9],
            backend,
            window,
        })
    }

    fn mark_field(&mut self, index: usize, with: Cell) {
        self.board[index] = with;
        // Don't forget to tell the backend! It has to update it's internal structure then
        self.backend.update_instances(&self.board);
    }
}

impl HandleEvent for App {
    fn handle(&mut self, event: Event<()>, flow: &mut ControlFlow) {
        match event {
            Event::WindowEvent { ref event, .. } => match event {
                &WindowEvent::MouseInput {
                    button: MouseButton::Left,
                    ..
                } => {
                    self.mark_field(2, Cell::Ring);
                    self.mark_field(7, Cell::Cross);
                    // Not triggering would cause the backend not to know when it should redraw,
                    // and so it would be drawn on the next required redraw, such as the window
                    // being visible again or switching workspaces.
                    self.window.request_redraw();
                }
                _ => (),
            },
            _ => (),
        }
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
