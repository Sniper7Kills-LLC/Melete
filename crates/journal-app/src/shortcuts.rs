use gtk4::gdk::ModifierType;
use gtk4::prelude::*;
use gtk4::{ApplicationWindow, DrawingArea, EventControllerKey};

use crate::state::SharedState;

pub fn attach_keyboard_shortcuts(
    window: &ApplicationWindow,
    state: SharedState,
    canvas: DrawingArea,
) {
    let key_ctrl = EventControllerKey::new();
    key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);

    {
        let state = state.clone();
        let canvas = canvas.clone();
        let window = window.clone();
        key_ctrl.connect_key_pressed(move |_ctrl, keyval, _code, mods| {
            let ctrl = mods.contains(ModifierType::CONTROL_MASK);
            let shift = mods.contains(ModifierType::SHIFT_MASK);

            match keyval {
                gtk4::gdk::Key::z | gtk4::gdk::Key::Z if ctrl && shift => {
                    crate::input::redo(&state, &canvas);
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::z | gtk4::gdk::Key::Z if ctrl => {
                    crate::input::undo(&state, &canvas);
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::y | gtk4::gdk::Key::Y if ctrl => {
                    crate::input::redo(&state, &canvas);
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::plus | gtk4::gdk::Key::equal if ctrl => {
                    let (sw, sh) = {
                        let s = state.borrow();
                        s.transform.screen_size()
                    };
                    state.borrow_mut().transform.zoom_at((sw * 0.5, sh * 0.5), 1.2);
                    canvas.queue_draw();
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::minus if ctrl => {
                    let (sw, sh) = {
                        let s = state.borrow();
                        s.transform.screen_size()
                    };
                    state.borrow_mut().transform.zoom_at((sw * 0.5, sh * 0.5), 1.0 / 1.2);
                    canvas.queue_draw();
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::_0 if ctrl => {
                    let page_rect = state.borrow().page_rect;
                    let mut s = state.borrow_mut();
                    crate::state::fit_viewport_to_page_pub(&mut s.transform, page_rect);
                    drop(s);
                    canvas.queue_draw();
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::s if ctrl => {
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::e | gtk4::gdk::Key::E if ctrl => {
                    crate::state::toggle_tool_eraser(&state);
                    canvas.queue_draw();
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::b | gtk4::gdk::Key::B if !ctrl => {
                    crate::state::set_tool_pen(&state);
                    canvas.queue_draw();
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::h | gtk4::gdk::Key::H if !ctrl => {
                    crate::state::set_tool_highlighter(&state);
                    canvas.queue_draw();
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::v | gtk4::gdk::Key::V if !ctrl => {
                    crate::state::set_tool_selection(&state);
                    canvas.queue_draw();
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::Delete => {
                    crate::input::delete_selection(&state, &canvas);
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::Escape => {
                    crate::state::clear_selection(&state);
                    canvas.queue_draw();
                    return glib::Propagation::Stop;
                }
                gtk4::gdk::Key::F11 => {
                    let is_full = window.is_fullscreen();
                    if is_full {
                        window.unfullscreen();
                    } else {
                        window.fullscreen();
                    }
                    return glib::Propagation::Stop;
                }
                _ => {}
            }
            glib::Propagation::Proceed
        });
    }

    window.add_controller(key_ctrl);
}

use gtk4::glib;
