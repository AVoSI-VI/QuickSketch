//! Infinite canvas: pan, zoom, and a single round brush.

use egui::{
    Color32, CursorIcon, Key, Painter, PointerButton, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2,
};

use crate::stroke::{Circle, Point, Stroke as Ink};

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
        self.zoom = (self.zoom * factor).clamp(0.08, 24.0);
        let after = self.world_to_screen(before);
        self.offset += screen.to_vec2() - after.to_vec2();
    }
}

pub struct Canvas {
    view: View,
    strokes: Vec<Ink>,
    active: Option<Ink>,
    circle_origin: Option<Point>,
    panning: bool,
    pub brush_size: f32,
    pub brush_color: Color32,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            view: View::default(),
            strokes: Vec::new(),
            active: None,
            circle_origin: None,
            panning: false,
            brush_size: 4.0,
            brush_color: Color32::BLACK,
        }
    }
}

pub enum Tool {
    Brush,
    Circle(Option<Circle>),
}

impl Canvas {
    pub fn brush_ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Brush");
            ui.add(egui::Slider::new(&mut self.brush_size, 1.0..=64.0).text("size"));
            ui.color_edit_button_srgba(&mut self.brush_color);
        });
    }

    pub fn commit(&mut self) {
        if let Some(stroke) = self.active.take()
            && !stroke.points.is_empty()
        {
            self.strokes.push(stroke);
        }
    }

    pub fn strokes_mut(&mut self) -> &mut Vec<Ink> {
        &mut self.strokes
    }

    /// Returns a new selection circle when the user drags one.
    pub fn show(&mut self, ui: &mut Ui, tool: &Tool) -> Option<Circle> {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        self.apply_view(&response, ui, rect);
        let drawn = self.apply_pointer(ui, rect, tool);

        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(246, 244, 240));
        draw_grid(&painter, rect, &self.view);
        for stroke in &self.strokes {
            paint_ink(&painter, &self.view, stroke);
        }
        if let Some(stroke) = &self.active {
            paint_ink(&painter, &self.view, stroke);
        }
        let selection = match tool {
            Tool::Circle(existing) => drawn.or(*existing),
            Tool::Brush => None,
        };
        if let Some(circle) = selection {
            paint_circle(&painter, &self.view, circle);
        }
        ui.painter().text(
            rect.left_top() + Vec2::new(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            "Left-drag draws. Middle-drag or left Ctrl + left-drag pans. Scroll zooms.",
            egui::FontId::proportional(12.0),
            Color32::from_black_alpha(140),
        );
        selection
    }

    fn apply_view(&mut self, response: &Response, ui: &Ui, rect: Rect) {
        let (middle_down, middle_pressed, ctrl_left, primary_down, delta, pos) =
            ui.input(|input| {
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
        if (middle_pressed && over) || (ctrl_pan && over && !self.panning) {
            self.panning = true;
        }
        if !middle_down && !ctrl_pan {
            self.panning = false;
        }
        if self.panning {
            self.view.offset += delta;
        }
        let scroll = ui.input(|input| input.smooth_scroll_delta().y);
        if scroll != 0.0
            && response.hovered()
            && let Some(pos) = response.hover_pos()
        {
            ui.input_mut(|input| input.smooth_scroll_delta = Vec2::ZERO);
            self.view.zoom_at(pos, (scroll * 0.002).exp());
        }
        let cursor = if self.panning {
            CursorIcon::Grabbing
        } else {
            CursorIcon::Crosshair
        };
        let _ = response.clone().on_hover_cursor(cursor);
    }

    fn apply_pointer(&mut self, ui: &Ui, rect: Rect, tool: &Tool) -> Option<Circle> {
        let primary = PointerButton::Primary;
        let (pressed, down, released, pos) = ui.input(|input| {
            (
                input.pointer.button_pressed(primary),
                input.pointer.button_down(primary),
                input.pointer.button_released(primary),
                input.pointer.latest_pos(),
            )
        });
        if self.panning {
            self.circle_origin = None;
            self.commit();
            return None;
        }
        let Some(pos) = pos else {
            if released {
                self.commit();
                self.circle_origin = None;
            }
            return None;
        };
        let world = self.view.screen_to_world(pos);
        let over = rect.contains(pos);
        match tool {
            Tool::Brush => {
                self.draw_at(world, over, pressed, down, released);
                None
            }
            Tool::Circle(_) => self.drag_circle(world, over, pressed, down, released),
        }
    }

    fn draw_at(&mut self, world: Point, over: bool, pressed: bool, down: bool, released: bool) {
        if pressed && over {
            self.active = Some(Ink {
                points: vec![world],
                color: color_bytes(self.brush_color),
                width: self.brush_size / self.view.zoom,
            });
        } else if down && let Some(stroke) = &mut self.active {
            push_point(stroke, world, self.view.zoom);
        }
        if released && let Some(stroke) = &mut self.active {
            push_point(stroke, world, self.view.zoom);
        }
        if released {
            self.commit();
        }
    }

    fn drag_circle(
        &mut self,
        world: Point,
        over: bool,
        pressed: bool,
        down: bool,
        released: bool,
    ) -> Option<Circle> {
        if pressed && over {
            self.circle_origin = Some(world);
        }
        let circle = if (down || released) && self.circle_origin.is_some() {
            self.circle_from(world)
        } else {
            None
        };
        if released {
            self.circle_origin = None;
        }
        circle
    }

    fn circle_from(&self, world: Point) -> Option<Circle> {
        let origin = self.circle_origin?;
        Some(Circle {
            center: origin,
            radius: origin.dist(world).max(1.0),
        })
    }
}

fn push_point(stroke: &mut Ink, world: Point, zoom: f32) {
    let far = stroke
        .points
        .last()
        .is_none_or(|last| last.dist(world) >= 0.4 / zoom);
    if far {
        stroke.points.push(world);
    }
}

fn color_bytes(color: Color32) -> [u8; 4] {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    [r, g, b, a]
}

fn draw_grid(painter: &Painter, rect: Rect, view: &View) {
    let mut step = 64.0;
    while step * view.zoom < 12.0 {
        step *= 2.0;
    }
    while step * view.zoom > 160.0 && step > 8.0 {
        step *= 0.5;
    }
    let top_left = view.screen_to_world(rect.min);
    let bottom_right = view.screen_to_world(rect.max);
    let stroke = Stroke::new(1.0, Color32::from_black_alpha(20));
    let mut x = (top_left.x / step).floor() * step;
    while x < bottom_right.x {
        let a = view.world_to_screen(Point { x, y: top_left.y });
        let b = view.world_to_screen(Point {
            x,
            y: bottom_right.y,
        });
        painter.line_segment([a, b], stroke);
        x += step;
    }
    let mut y = (top_left.y / step).floor() * step;
    while y < bottom_right.y {
        let a = view.world_to_screen(Point { x: top_left.x, y });
        let b = view.world_to_screen(Point {
            x: bottom_right.x,
            y,
        });
        painter.line_segment([a, b], stroke);
        y += step;
    }
}

fn paint_ink(painter: &Painter, view: &View, stroke: &Ink) {
    let color = Color32::from_rgba_unmultiplied(
        stroke.color[0],
        stroke.color[1],
        stroke.color[2],
        stroke.color[3],
    );
    let width = (stroke.width * view.zoom).max(1.0);
    let paint = Stroke::new(width, color);
    for pair in stroke.points.windows(2) {
        painter.line_segment(
            [view.world_to_screen(pair[0]), view.world_to_screen(pair[1])],
            paint,
        );
        painter.circle_filled(view.world_to_screen(pair[0]), width * 0.5, color);
    }
    if let Some(last) = stroke.points.last() {
        painter.circle_filled(view.world_to_screen(*last), width * 0.5, color);
    }
}

fn paint_circle(painter: &Painter, view: &View, circle: Circle) {
    let center = view.world_to_screen(circle.center);
    let radius = circle.radius * view.zoom;
    painter.circle_filled(
        center,
        radius,
        Color32::from_rgba_unmultiplied(30, 120, 220, 36),
    );
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(2.0, Color32::from_rgb(30, 120, 220)),
    );
}
