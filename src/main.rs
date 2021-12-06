mod render;

use {
    rand::{distributions::Standard, prelude::*},
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

impl Cell {
    // Returns whether this cell is empty, false if it is used by any faction.
    fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Faction {
    Cross,
    Ring,
}

impl Faction {
    // Determines whether this faction makes the first turn. Ring is the one for that.
    fn goes_first(&self) -> bool {
        match self {
            Self::Cross => false,
            Self::Ring => true,
        }
    }

    // Returns the opposite faction, e.g. cross for ring and ring for cross.
    fn opposite(&self) -> Self {
        match self {
            Self::Cross => Self::Ring,
            Self::Ring => Self::Cross,
        }
    }
}

impl Distribution<Faction> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Faction {
        // exact mapping doesn't matter
        match rng.gen() {
            false => Faction::Cross,
            true => Faction::Ring,
        }
    }
}

impl From<Faction> for Cell {
    fn from(faction: Faction) -> Self {
        match faction {
            Faction::Cross => Cell::Cross,
            Faction::Ring => Cell::Ring,
        }
    }
}

struct App {
    selected_field: (u8, u8),
    board: [Cell; 9],
    game_over: bool,
    // we need only one sido to hold which faction it belongs to, the AI will then just be the
    // other one
    user_faction: Faction,

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

        let user_faction = thread_rng().gen();

        let mut app = Self {
            selected_field: (1, 1),
            board: [Cell::Empty; 9],
            game_over: false,
            user_faction,
            backend,
            window,
        };

        if !user_faction.goes_first() {
            app.ai_turn();
        }

        Ok(app)
    }

    fn mark_field(&mut self, index: usize, with: Cell) {
        self.board[index] = with;
        // Don't forget to tell the backend! It has to update it's internal structure then
        self.backend.update_instances(&self.board);
    }

    fn ai_turn(&mut self) {
        let selected_field = loop {
            let attempt = thread_rng().gen_range(0..9);
            // check if the field is empty at all
            if self.board[attempt].is_empty() {
                break attempt;
            }
        };
        self.mark_field(selected_field, self.user_faction.opposite().into());
    }

    fn check_game_over(&mut self) {
        let mut game_over = false;

        // check first if there is any empty field left, else the game is over anyways
        if !self.board.iter().any(Cell::is_empty) {
            game_over = true;
        } else {
            for i in 0..3 {
                if (
                    // horizontal
                    !self.board[3 * i].is_empty()
                        && self.board[3 * i] == self.board[3 * i + 1]
                        && self.board[3 * i] == self.board[3 * i + 2]
                ) || (
                    // vertical
                    !self.board[i].is_empty()
                        && self.board[i] == self.board[i + 3]
                        && self.board[i] == self.board[i + 6]
                ) {
                    game_over = true;
                }
            }

            // crossed
            if (!self.board[0].is_empty()
                && self.board[0] == self.board[4]
                && self.board[0] == self.board[8])
                || (!self.board[2].is_empty()
                    && self.board[2] == self.board[4]
                    && self.board[2] == self.board[6])
            {
                game_over = true;
            }
        }

        if game_over {
            self.game_over = true;
            self.backend.set_background(wgpu::Color {
                r: 0.3,
                g: 0.35,
                b: 0.35,
                a: 1.0,
            });
        }
    }

    fn reset(&mut self) {
        // TODO eventually the app should be more self-contained and all the game stuff into it's
        // own struct which is resettable by ::new()ing and the app more like a manager, but it is
        // what it is
        self.board = [Cell::Empty; 9];
        self.game_over = false;
        self.backend.set_background(wgpu::Color {
            r: 0.04,
            g: 0.09,
            b: 0.09,
            a: 1.0,
        });

        self.user_faction = thread_rng().gen();
        if !self.user_faction.goes_first() {
            self.ai_turn();
        }
    }
}

impl HandleEvent for App {
    fn handle(&mut self, event: Event<()>, flow: &mut ControlFlow) {
        match event {
            Event::WindowEvent { ref event, .. } => match event {
                WindowEvent::CursorMoved { position, .. } => {
                    let window_size = self.window.inner_size();

                    // simple bounds checking, sometimes on X I've seen some mouse event coming
                    // from out of the actual window size
                    if !(position.x < 0.0
                        || position.x >= window_size.width as f64
                        || position.y < 0.0
                        || position.y >= window_size.width as f64)
                    {
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
                }
                WindowEvent::MouseInput {
                    button: MouseButton::Left,
                    state: ElementState::Released,
                    ..
                } => {
                    if !self.game_over {
                        // basically 2d to 1d index conversion, but we know already the width of one
                        // line is 3
                        let field_index = self.selected_field.0 * 3 + self.selected_field.1;

                        // check first if the cell is free at all, we shouldn't overwrite an used one
                        if self.board[usize::from(field_index)].is_empty() {
                            self.mark_field(usize::from(field_index), self.user_faction.into());
                            self.check_game_over();

                            if !self.game_over {
                                self.ai_turn();
                                self.check_game_over();
                            }

                            // Not triggering would cause the backend not to know when it should redraw,
                            // and so it would be drawn on the next required redraw, such as the window
                            // being visible again or switching workspaces.
                            self.window.request_redraw();
                        }
                    } else {
                        self.reset();
                        self.window.request_redraw();
                    }
                }
                _ => (),
            },
            _ => (),
        }
        // Just forward, maybe it wants to do something with it as well (such as... re-rendering if
        // needed)
        self.backend.handle(event, flow);
    }
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new();

    let mut app = pollster::block_on(App::new(&event_loop)).unwrap_or_else(|e| {
        log::error!("{}", e);
        std::process::exit(1)
    });
    event_loop.run(move |event, _, flow| app.handle(event, flow));
}
