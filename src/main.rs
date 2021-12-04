mod render;

use {
    render::Backend,
    thiserror::Error,
    winit::{
        dpi,
        event::{ElementState, Event, MouseButton, WindowEvent},
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
    selected_field: (u8, u8),
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
            selected_field: (1, 1),
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
                WindowEvent::CursorMoved { position, .. } => {
                    let window_size = self.window.inner_size();

                    // even though it's name might not make that clear, these components now range
                    // from 0 to 3
                    let grid_pos = (
                        (position.x * 3.0 / f64::from(window_size.width)) as u8,
                        (position.y * 3.0 / f64::from(window_size.height)) as u8,
                    );
                    // winit thinks in y+ down, but wgpu by default y+ up, so invert
                    // (this causes our grid to be thought in the wgpu dimension)
                    let inverted = (grid_pos.0, 2 - grid_pos.1);

                    self.selected_field = inverted;
                }
                WindowEvent::MouseInput {
                    button: MouseButton::Left,
                    state: ElementState::Released,
                    ..
                } => {
                    // basically 2d to 1d index conversion, but we know already the width of one
                    // line is 3
                    let field_index = self.selected_field.0 * 3 + self.selected_field.1;
                    self.mark_field(usize::from(field_index), Cell::Cross);

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
