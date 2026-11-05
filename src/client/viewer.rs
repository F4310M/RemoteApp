//! The viewer window: draws the remote screen and captures local input.

use std::collections::HashSet;
use std::num::NonZeroU32;
use std::sync::Arc;

use anyhow::Result;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use super::{Session, UiEvent};
use crate::proto::{ClientMsg, InputEvent, MouseButton};

pub fn run(event_loop: EventLoop<UiEvent>, session: Session) -> Result<()> {
    let mut app = Viewer {
        session,
        window: None,
        surface: None,
        cursor: None,
        held_keys: HashSet::new(),
        scroll_acc: (0.0, 0.0),
        closed: None,
    };
    event_loop.run_app(&mut app)?;
    if let Some(reason) = app.closed {
        println!("  {reason}");
    }
    Ok(())
}

type Surface = softbuffer::Surface<Arc<Window>, Arc<Window>>;

struct Viewer {
    session: Session,
    window: Option<Arc<Window>>,
    surface: Option<Surface>,
    cursor: Option<PhysicalPosition<f64>>,
    held_keys: HashSet<u16>,
    /// Fractional wheel movement not yet sent (touchpads scroll in pixels).
    scroll_acc: (f64, f64),
    closed: Option<String>,
}

/// Where the remote picture sits inside the window (letterboxed).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

fn placement(win: (usize, usize), pic: (usize, usize)) -> Placement {
    if pic.0 == 0 || pic.1 == 0 || win.0 == 0 || win.1 == 0 {
        return Placement {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
    }
    let scale = (win.0 as f64 / pic.0 as f64).min(win.1 as f64 / pic.1 as f64);
    let w = ((pic.0 as f64 * scale).round() as usize).clamp(1, win.0);
    let h = ((pic.1 as f64 * scale).round() as usize).clamp(1, win.1);
    Placement {
        x: (win.0 - w) / 2,
        y: (win.1 - h) / 2,
        w,
        h,
    }
}

/// Window pixel → host position in 0..=65535 on each axis.
fn to_remote(p: Placement, pos: PhysicalPosition<f64>) -> Option<(u16, u16)> {
    if p.w == 0 || p.h == 0 {
        return None;
    }
    let fx = ((pos.x - p.x as f64) / p.w as f64).clamp(0.0, 1.0);
    let fy = ((pos.y - p.y as f64) / p.h as f64).clamp(0.0, 1.0);
    Some(((fx * 65535.0).round() as u16, (fy * 65535.0).round() as u16))
}

impl Viewer {
    fn send(&self, ev: InputEvent) {
        let _ = self.session.input.send(ClientMsg::Input(ev));
    }

    fn current_placement(&self) -> Placement {
        let Some(w) = &self.window else {
            return placement((0, 0), (0, 0));
        };
        let size = w.inner_size();
        let pic = self.session.picture.lock().unwrap();
        let dims = if pic.width > 0 {
            (pic.width, pic.height)
        } else {
            (
                self.session.screen.0 as usize,
                self.session.screen.1 as usize,
            )
        };
        placement((size.width as usize, size.height as usize), dims)
    }

    fn release_all_keys(&mut self) {
        for code in std::mem::take(&mut self.held_keys) {
            self.send(InputEvent::Key { code, down: false });
        }
    }

    fn draw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let size = window.inner_size();
        let (Some(ww), Some(wh)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };
        if surface.resize(ww, wh).is_err() {
            return;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
            return;
        };
        let (ww, wh) = (size.width as usize, size.height as usize);
        let pic = self.session.picture.lock().unwrap();
        let p = placement((ww, wh), (pic.width, pic.height));
        buf.fill(0x0020_2020);
        if p.w > 0 {
            // Nearest-neighbour scaling; column lookup computed once per frame.
            let cols: Vec<usize> = (0..p.w).map(|x| x * pic.width / p.w).collect();
            for y in 0..p.h {
                let sy = y * pic.height / p.h;
                let src = &pic.pixels[sy * pic.width..(sy + 1) * pic.width];
                let dst = &mut buf[(p.y + y) * ww + p.x..(p.y + y) * ww + p.x + p.w];
                for (d, &sx) in dst.iter_mut().zip(&cols) {
                    *d = src[sx];
                }
            }
        }
        drop(pic);
        let _ = buf.present();
    }
}

impl ApplicationHandler<UiEvent> for Viewer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        // Open at the remote size, shrunk to fit a typical screen.
        let (sw, sh) = (
            self.session.screen.0.max(1) as f64,
            self.session.screen.1.max(1) as f64,
        );
        let scale = (1600.0 / sw).min(900.0 / sh).min(1.0);
        let attrs = Window::default_attributes()
            .with_title(format!("Magic Remote: {}", self.session.host_name))
            .with_inner_size(LogicalSize::new(sw * scale, sh * scale));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.closed = Some(format!("cannot open a window: {e}"));
                return event_loop.exit();
            }
        };
        let surface = softbuffer::Context::new(window.clone())
            .and_then(|ctx| Surface::new(&ctx, window.clone()));
        match surface {
            Ok(s) => self.surface = Some(s),
            Err(e) => {
                self.closed = Some(format!("cannot draw in the window: {e}"));
                return event_loop.exit();
            }
        }
        self.window = Some(window);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UiEvent) {
        match event {
            UiEvent::NewPicture => {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            UiEvent::Closed(reason) => {
                self.closed = Some(reason);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.release_all_keys();
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Resized(_) => {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::Focused(false) => self.release_all_keys(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some(position);
                if let Some((x, y)) = to_remote(self.current_placement(), position) {
                    self.send(InputEvent::MouseMove { x, y });
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    winit::event::MouseButton::Left => MouseButton::Left,
                    winit::event::MouseButton::Right => MouseButton::Right,
                    winit::event::MouseButton::Middle => MouseButton::Middle,
                    winit::event::MouseButton::Back => MouseButton::Back,
                    winit::event::MouseButton::Forward => MouseButton::Forward,
                    _ => return,
                };
                self.send(InputEvent::MouseButton {
                    button,
                    down: state == ElementState::Pressed,
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x as f64, y as f64),
                    MouseScrollDelta::PixelDelta(p) => (p.x / 40.0, p.y / 40.0),
                };
                self.scroll_acc.0 += dx;
                self.scroll_acc.1 += dy;
                let (sx, sy) = (self.scroll_acc.0.trunc(), self.scroll_acc.1.trunc());
                self.scroll_acc.0 -= sx;
                self.scroll_acc.1 -= sy;
                if sx != 0.0 || sy != 0.0 {
                    // winit: positive y = wheel away from you = scroll up.
                    self.send(InputEvent::Scroll {
                        dx: sx as i32,
                        dy: -sy as i32,
                    });
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                if is_synthetic {
                    return;
                }
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let Some(code) = crate::keymap::from_winit(code) else {
                    return;
                };
                let down = event.state == ElementState::Pressed;
                if down {
                    self.held_keys.insert(code);
                } else if !self.held_keys.remove(&code) {
                    return; // release of a key pressed before we had focus
                }
                self.send(InputEvent::Key { code, down });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterbox_and_mapping() {
        // 16:9 picture in a square window: bars top and bottom.
        let p = placement((1000, 1000), (1920, 1080));
        assert_eq!(
            p,
            Placement {
                x: 0,
                y: 218,
                w: 1000,
                h: 563
            }
        );
        assert_eq!(
            to_remote(p, PhysicalPosition::new(0.0, 218.0)),
            Some((0, 0))
        );
        assert_eq!(
            to_remote(p, PhysicalPosition::new(1000.0, 781.0)),
            Some((65535, 65535))
        );
        // Clicks on the bars clamp to the edge.
        assert_eq!(
            to_remote(p, PhysicalPosition::new(500.0, 0.0)).unwrap().1,
            0
        );
    }
}
