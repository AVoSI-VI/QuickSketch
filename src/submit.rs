//! Circle selection, PNG export, and clearing the ink inside that circle.

use std::fs;
use std::path::Path;

use image::{Rgba, RgbaImage};

use crate::persist::{self, Submission};
use crate::stroke::{self, Circle, Point, Stroke};

#[derive(Default)]
pub struct SubmitPhase {
    category_id: Option<String>,
    circle: Option<Circle>,
}

impl SubmitPhase {
    pub fn is_placing(&self) -> bool {
        self.category_id.is_some()
    }

    pub fn circle(&self) -> Option<Circle> {
        self.circle
    }

    pub fn can_finish(&self) -> bool {
        self.circle.is_some_and(|circle| circle.radius >= 4.0)
    }

    pub fn begin(&mut self, category_id: String) {
        self.category_id = Some(category_id);
        self.circle = None;
    }

    pub fn cancel(&mut self) {
        *self = Self::default();
    }

    pub fn set_circle(&mut self, circle: Circle) {
        if self.category_id.is_some() {
            self.circle = Some(circle);
        }
    }

    pub fn finish(
        &mut self,
        root: &Path,
        strokes: &mut Vec<Stroke>,
        submissions: &mut Vec<Submission>,
    ) -> Result<String, String> {
        let category_id = self
            .category_id
            .clone()
            .ok_or("Start a submission first.")?;
        let circle = self
            .circle
            .ok_or("Drag a circle around the sketch first.")?;
        if circle.radius < 4.0 {
            return Err("The circle is too small.".into());
        }
        let image = rasterize(strokes, circle);
        let dir = persist::sketch_dir(root, &category_id);
        fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let file_name = unique_png_name(&dir);
        let path = dir.join(&file_name);
        image.save(&path).map_err(|err| err.to_string())?;
        *strokes = stroke::drop_inside(strokes, circle);
        submissions.push(Submission {
            category_id,
            file_name,
            created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        });
        let saved = path.display().to_string();
        *self = Self::default();
        Ok(saved)
    }
}

fn unique_png_name(dir: &Path) -> String {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%3f");
    let mut name = format!("{stamp}.png");
    let mut index = 2;
    while dir.join(&name).exists() {
        name = format!("{stamp}-{index}.png");
        index += 1;
    }
    name
}

pub fn rasterize(strokes: &[Stroke], circle: Circle) -> RgbaImage {
    let side = export_side(circle.radius);
    let mut image = RgbaImage::from_pixel(side, side, Rgba([0, 0, 0, 0]));
    let scale = side as f32 / (circle.radius * 2.0);
    for stroke in strokes {
        if stroke.points.len() == 1 {
            paint_disk(
                &mut image,
                stroke.points[0],
                stroke.width * 0.5,
                stroke.color,
                circle,
                scale,
            );
        }
        for pair in stroke.points.windows(2) {
            paint_segment(
                &mut image,
                pair[0],
                pair[1],
                stroke.width * 0.5,
                stroke.color,
                circle,
                scale,
            );
        }
    }
    image
}

fn export_side(radius: f32) -> u32 {
    let wanted = (radius * 2.0 * 2.0).ceil() as u32;
    wanted.clamp(64, 1536)
}

fn paint_disk(
    image: &mut RgbaImage,
    center: Point,
    radius: f32,
    color: [u8; 4],
    circle: Circle,
    scale: f32,
) {
    paint_segment(image, center, center, radius, color, circle, scale);
}

fn paint_segment(
    image: &mut RgbaImage,
    a: Point,
    b: Point,
    radius: f32,
    color: [u8; 4],
    circle: Circle,
    scale: f32,
) {
    let side = image.width();
    let (ax, ay) = world_to_px(a, circle, scale, side);
    let (bx, by) = world_to_px(b, circle, scale, side);
    let pad = radius * scale + 1.0;
    let min_x = (ax.min(bx) - pad).floor().max(0.0) as u32;
    let max_x = (ax.max(bx) + pad).ceil().min(side as f32 - 1.0) as u32;
    let min_y = (ay.min(by) - pad).floor().max(0.0) as u32;
    let max_y = (ay.max(by) + pad).ceil().min(image.height() as f32 - 1.0) as u32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let world = px_to_world(x, y, circle, scale, side);
            if world.dist(circle.center) > circle.radius {
                continue;
            }
            if dist_to_segment(world, a, b) > radius {
                continue;
            }
            let blended = over(*image.get_pixel(x, y), color);
            image.put_pixel(x, y, blended);
        }
    }
}

fn world_to_px(point: Point, circle: Circle, scale: f32, side: u32) -> (f32, f32) {
    let half = side as f32 * 0.5;
    (
        (point.x - circle.center.x) * scale + half,
        (point.y - circle.center.y) * scale + half,
    )
}

fn px_to_world(x: u32, y: u32, circle: Circle, scale: f32, side: u32) -> Point {
    let half = side as f32 * 0.5;
    Point {
        x: circle.center.x + (x as f32 + 0.5 - half) / scale,
        y: circle.center.y + (y as f32 + 0.5 - half) / scale,
    }
}

fn dist_to_segment(point: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len2 = dx * dx + dy * dy;
    if len2 < 1.0e-12 {
        return point.dist(a);
    }
    let t = ((point.x - a.x) * dx + (point.y - a.y) * dy) / len2;
    let t = t.clamp(0.0, 1.0);
    point.dist(Point {
        x: a.x + dx * t,
        y: a.y + dy * t,
    })
}

fn over(dst: Rgba<u8>, src: [u8; 4]) -> Rgba<u8> {
    let sa = src[3] as f32 / 255.0;
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return Rgba([0, 0, 0, 0]);
    }
    let mut out = [0u8; 4];
    for channel in 0..3 {
        let s = src[channel] as f32 / 255.0;
        let d = dst[channel] as f32 / 255.0;
        let mixed = (s * sa + d * da * (1.0 - sa)) / out_a;
        out[channel] = (mixed * 255.0).round() as u8;
    }
    out[3] = (out_a * 255.0).round() as u8;
    Rgba(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_paints_the_stroke_and_leaves_the_outside_clear() {
        let circle = Circle {
            center: Point { x: 0.0, y: 0.0 },
            radius: 10.0,
        };
        let stroke = Stroke {
            points: vec![Point { x: -20.0, y: 0.0 }, Point { x: 20.0, y: 0.0 }],
            color: [0, 0, 0, 255],
            width: 6.0,
        };
        let image = rasterize(&[stroke], circle);
        let mid = image.width() / 2;
        assert_eq!(image.get_pixel(mid, mid)[3], 255);
        assert_eq!(image.get_pixel(0, 0)[3], 0);
    }

    #[test]
    fn finish_saves_and_clears_only_the_inside() {
        let root = std::env::temp_dir().join(format!("artproj-submit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut phase = SubmitPhase::default();
        phase.begin("c1".into());
        phase.set_circle(Circle {
            center: Point { x: 0.0, y: 0.0 },
            radius: 10.0,
        });
        let mut strokes = vec![
            Stroke {
                points: vec![Point { x: -2.0, y: 0.0 }, Point { x: 2.0, y: 0.0 }],
                color: [0, 0, 0, 255],
                width: 4.0,
            },
            Stroke {
                points: vec![Point { x: 40.0, y: 40.0 }, Point { x: 50.0, y: 40.0 }],
                color: [0, 0, 0, 255],
                width: 4.0,
            },
        ];
        let mut submissions = Vec::new();
        let saved = phase.finish(&root, &mut strokes, &mut submissions).unwrap();
        assert!(Path::new(&saved).is_file());
        assert_eq!(submissions.len(), 1);
        assert!(phase.circle().is_none());
        assert_eq!(strokes.len(), 1);
        assert!(strokes[0].points[0].x > 10.0);
        let _ = fs::remove_dir_all(&root);
    }
}
