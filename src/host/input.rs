//! Replays the client's mouse and keyboard on this machine.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;

use anyhow::Result;
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};

use crate::proto::{InputEvent, MouseButton};

/// Runs on its own thread (the injection APIs are not `Send`). Ends when
/// the sender is dropped, releasing anything still held down.
/// `screen` holds the current host screen size as `(width << 32) | height`.
pub fn run(rx: Receiver<InputEvent>, screen: Arc<AtomicU64>) -> Result<()> {
    let settings = Settings {
        linux_delay: 0,
        ..Settings::default()
    };
    let mut enigo =
        Enigo::new(&settings).map_err(|e| anyhow::anyhow!("cannot simulate input: {e}"))?;
    let mut held_keys: HashSet<u16> = HashSet::new();
    let mut held_buttons: HashSet<Button> = HashSet::new();

    for ev in rx {
        let r = match ev {
            InputEvent::MouseMove { x, y } => {
                let size = screen.load(Ordering::Relaxed);
                let (w, h) = ((size >> 32) as i64, (size & 0xFFFF_FFFF) as i64);
                let px = (x as i64 * (w - 1).max(0) / 65535) as i32;
                let py = (y as i64 * (h - 1).max(0) / 65535) as i32;
                enigo
                    .move_mouse(px, py, Coordinate::Abs)
                    .map_err(|e| e.to_string())
            }
            InputEvent::MouseButton { button, down } => {
                let b = map_button(button);
                if down {
                    held_buttons.insert(b)
                } else {
                    held_buttons.remove(&b)
                };
                enigo.button(b, dir(down)).map_err(|e| e.to_string())
            }
            InputEvent::Scroll { dx, dy } => {
                let mut r = Ok(());
                if dy != 0 {
                    r = enigo.scroll(dy, Axis::Vertical).map_err(|e| e.to_string());
                }
                if dx != 0 {
                    r = r.and(
                        enigo
                            .scroll(dx, Axis::Horizontal)
                            .map_err(|e| e.to_string()),
                    );
                }
                r
            }
            InputEvent::Key { code, down } => {
                let was_held = if down {
                    !held_keys.insert(code)
                } else {
                    !held_keys.remove(&code)
                };
                // X11 auto-repeats held keys by itself; Windows needs the
                // client's repeats.
                if down && was_held && cfg!(target_os = "linux") {
                    continue;
                }
                key(&mut enigo, code, down)
            }
        };
        if let Err(e) = r {
            tracing::debug!("input event failed: {e}");
        }
    }

    // Client gone: never leave a key or button stuck down.
    for code in held_keys {
        let _ = key(&mut enigo, code, false);
    }
    for b in held_buttons {
        let _ = enigo.button(b, Direction::Release);
    }
    Ok(())
}

fn dir(down: bool) -> Direction {
    if down {
        Direction::Press
    } else {
        Direction::Release
    }
}

fn map_button(b: MouseButton) -> Button {
    match b {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
        MouseButton::Back => Button::Back,
        MouseButton::Forward => Button::Forward,
    }
}

#[cfg(target_os = "linux")]
fn key(enigo: &mut Enigo, code: u16, down: bool) -> Result<(), String> {
    // X11 keycodes are evdev codes + 8.
    enigo.raw(code + 8, dir(down)).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn key(enigo: &mut Enigo, code: u16, down: bool) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;

    if code == 119 {
        // Pause has no usable scancode; send the virtual key instead.
        return enigo
            .key(enigo::Key::Pause, dir(down))
            .map_err(|e| e.to_string());
    }
    let Some(scan) = crate::keymap::evdev_to_set1(code) else {
        return Err(format!("no Windows scancode for key {code}"));
    };
    let mut flags = KEYEVENTF_SCANCODE;
    if scan & 0xFF00 == 0xE000 {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scan & 0xFF,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    if sent == 1 {
        Ok(())
    } else {
        Err("SendInput was blocked (is a window of a higher-privilege app focused?)".into())
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn key(enigo: &mut Enigo, code: u16, down: bool) -> Result<(), String> {
    enigo.raw(code, dir(down)).map_err(|e| e.to_string())
}
