//! Infinite canvas: pan, zoom, and a single round brush.

use egui::{Color32, CursorIcon, Painter, PointerButton, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::stroke::{Circle, Point, Stroke as Ink};
use crate::view::{self, Nav, View};

pub struct Canvas {
    view: View,
    strokes: Vec<Ink>,
    active: Option<Ink>,
    circle_origin: Option<Point>,
    nav: Nav,
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
            nav: Nav::default(),
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
    /// Returns true when the canvas was cleared.
    pub fn brush_ui(&mut self, ui: &mut Ui) -> bool {
        let mut cleared = false;
        ui.horizontal(|ui| {
            ui.label("Brush");
            ui.add(egui::Slider::new(&mut self.brush_size, 1.0..=64.0).text("size"));
            ui.color_edit_button_srgba(&mut self.brush_color);
            if ui.button("Clear canvas").clicked() {
                self.clear();
                cleared = true;
            }
        });
        cleared
    }

    pub fn clear(&mut self) {
        self.strokes.clear();
        self.active = None;
        self.circle_origin = None;
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
        view::apply(
            &mut self.view,
            &mut self.nav,
            &response,
            ui,
            rect,
            floating_menu_at,
        );
        if !self.nav.panning {
            let _ = response.clone().on_hover_cursor(CursorIcon::Crosshair);
        }
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
            "Left-drag draws. Middle-drag, left Ctrl + left-drag, or two fingers pan. Scroll or pinch zooms.",
            egui::FontId::proportional(12.0),
            Color32::from_black_alpha(140),
        );
        selection
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
        if self.nav.touch_gesture {
            self.active = None;
            self.circle_origin = None;
            return None;
        }
        if self.nav.panning {
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
        // Combo menus float above the canvas. A touch-drag that scrolls one
        // still reports the primary button as down inside this rect.
        if floating_menu_at(ui, pos) {
            if released {
                self.commit();
                self.circle_origin = None;
            }
            return None;
        }
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
                width: self.brush_size / self.view.zoom(),
            });
        } else if down && let Some(stroke) = &mut self.active {
            push_point(stroke, world, self.view.zoom());
        }
        if released && let Some(stroke) = &mut self.active {
            push_point(stroke, world, self.view.zoom());
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

/// True when a popup, such as a category menu, is the top layer at `pos`.
fn floating_menu_at(ui: &Ui, pos: Pos2) -> bool {
    ui.ctx()
        .layer_id_at(pos)
        .is_some_and(|layer| layer != ui.layer_id())
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
    while step * view.zoom() < 12.0 {
        step *= 2.0;
    }
    while step * view.zoom() > 160.0 && step > 8.0 {
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
    let width = (stroke.width * view.zoom()).max(1.0);
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
    let radius = circle.radius * view.zoom();
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
