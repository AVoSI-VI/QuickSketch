//! Shared pan and zoom. `screen = world * zoom + offset`.

use egui::{CursorIcon, Key, PointerButton, Pos2, Rect, Response, Ui, Vec2};

use crate::stroke::Point;

const MIN_ZOOM: f32 = 0.08;
const MAX_ZOOM: f32 = 24.0;

#[derive(Clone)]
pub struct View {
    /// `screen = world * zoom + offset`
    offset: Vec2,
    zoom: f32,
}

impl Default for View {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl View {
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Fit `image` inside `rect`, centered. The zoom stays inside the shared clamp.
    pub fn fit(rect: Rect, image: Vec2) -> Self {
        let raw = if image.x > 0.0 && image.y > 0.0 {
            (rect.width() / image.x).min(rect.height() / image.y)
        } else {
            1.0
        };
        let zoom = raw.clamp(MIN_ZOOM, MAX_ZOOM);
        let drawn = image * zoom;
        Self {
            offset: rect.min.to_vec2() + (rect.size() - drawn) * 0.5,
            zoom,
        }
    }

    pub fn image_rect(&self, image: Vec2) -> Rect {
        Rect::from_min_size(self.offset.to_pos2(), image * self.zoom)
    }

    pub fn world_to_screen(&self, point: Point) -> Pos2 {
        Pos2::new(
            point.x * self.zoom + self.offset.x,
            point.y * self.zoom + self.offset.y,
        )
    }

    pub fn screen_to_world(&self, pos: Pos2) -> Point {
        Point {
            x: (pos.x - self.offset.x) / self.zoom,
            y: (pos.y - self.offset.y) / self.zoom,
        }
    }

    fn zoom_at(&mut self, screen: Pos2, factor: f32) {
        let before = self.screen_to_world(screen);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let after = self.world_to_screen(before);
        self.offset += screen.to_vec2() - after.to_vec2();
    }
}

#[derive(Clone, Default)]
pub struct Nav {
    pub panning: bool,
    pub touch_gesture: bool,
}

/// Middle-drag, left Ctrl + left-drag, and two-finger drag pan.
/// Scroll and pinch zoom toward the pointer. The cursor is grabbing while panning.
/// `blocked` skips a two-finger gesture when a popup covers the touch.
pub fn apply(
    view: &mut View,
    nav: &mut Nav,
    response: &Response,
    ui: &Ui,
    rect: Rect,
    blocked: impl Fn(&Ui, Pos2) -> bool,
) {
    let (middle_down, middle_pressed, ctrl_left, primary_down, delta, pos) = ui.input(|input| {
        (
            input.pointer.button_down(PointerButton::Middle),
            input.pointer.button_pressed(PointerButton::Middle),
            input.key_down(Key::ControlLeft),
            input.pointer.button_down(PointerButton::Primary),
            input.pointer.delta(),
            input.pointer.latest_pos(),
        )
    });
    let over = pos.is_some_and(|pos| rect.contains(pos));
    let ctrl_pan = ctrl_left && primary_down;
    if (middle_pressed && over) || (ctrl_pan && over && !nav.panning) {
        nav.panning = true;
    }
    if !middle_down && !ctrl_pan {
        nav.panning = false;
    }
    if nav.panning {
        view.offset += delta;
    }
    apply_touch(view, nav, ui, rect, &blocked);
    let scroll = ui.input(|input| input.smooth_scroll_delta().y);
    if scroll != 0.0
        && response.hovered()
        && let Some(pos) = response.hover_pos()
    {
        ui.input_mut(|input| input.smooth_scroll_delta = Vec2::ZERO);
        view.zoom_at(pos, (scroll * 0.002).exp());
    }
    if nav.panning {
        let _ = response.clone().on_hover_cursor(CursorIcon::Grabbing);
    }
}

fn apply_touch(
    view: &mut View,
    nav: &mut Nav,
    ui: &Ui,
    rect: Rect,
    blocked: &impl Fn(&Ui, Pos2) -> bool,
) {
    let touch = ui.input(|input| input.multi_touch());
    let Some(touch) = touch else {
        nav.touch_gesture = false;
        return;
    };
    let on_rect = rect.contains(touch.center_pos) && !blocked(ui, touch.center_pos);
    nav.touch_gesture = on_rect;
    if !on_rect {
        return;
    }
    if touch.num_touches == 2 {
        if touch.zoom_delta.is_finite() && touch.zoom_delta != 1.0 {
            view.zoom_at(touch.center_pos, touch.zoom_delta);
        }
        view.offset += touch.translation_delta;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        let mut view = View::default();
        let screen = Pos2::new(100.0, 80.0);
        let before = view.screen_to_world(screen);
        view.zoom_at(screen, 2.0);
        let after = view.screen_to_world(screen);
        assert!((before.x - after.x).abs() < 1e-4);
        assert!((before.y - after.y).abs() < 1e-4);
        assert!((view.zoom() - 2.0).abs() < 1e-4);
    }

    #[test]
    fn fit_clamps_a_huge_image_and_centers_it() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(100.0, 100.0));
        let image = Vec2::new(10_000.0, 10_000.0);
        let view = View::fit(rect, image);
        assert!((view.zoom() - MIN_ZOOM).abs() < 1e-5);
        let painted = view.image_rect(image);
        assert!((painted.center().x - rect.center().x).abs() < 1e-3);
        assert!((painted.center().y - rect.center().y).abs() < 1e-3);
    }
}
