//! Brush strokes in world space, and clipping them against a selection circle.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn dist(self, other: Self) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub points: Vec<Point>,
    pub color: [u8; 4],
    /// Full width in world units.
    pub width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    pub center: Point,
    pub radius: f32,
}

pub fn inside(point: Point, circle: Circle) -> bool {
    point.dist(circle.center) <= circle.radius
}

/// Keep the parts of each stroke that lie strictly outside `circle`.
pub fn drop_inside(strokes: &[Stroke], circle: Circle) -> Vec<Stroke> {
    if circle.radius <= f32::EPSILON {
        return strokes.to_vec();
    }
    strokes
        .iter()
        .flat_map(|stroke| clip_stroke(stroke, circle))
        .collect()
}

fn clip_stroke(stroke: &Stroke, circle: Circle) -> Vec<Stroke> {
    let points = &stroke.points;
    if points.is_empty() {
        return Vec::new();
    }
    if points.len() == 1 {
        return if inside(points[0], circle) {
            Vec::new()
        } else {
            vec![stroke.clone()]
        };
    }

    let mut kept = Vec::new();
    let mut current = Vec::new();
    for pair in points.windows(2) {
        let outside = outside_spans(pair[0], pair[1], circle);
        if outside.is_empty() {
            flush(stroke, &mut current, &mut kept);
            continue;
        }
        for (index, (start, end)) in outside.into_iter().enumerate() {
            if index > 0 {
                flush(stroke, &mut current, &mut kept);
            }
            if current.last().is_none_or(|last| last.dist(start) > 0.01) {
                if !current.is_empty() {
                    flush(stroke, &mut current, &mut kept);
                }
                current.push(start);
            }
            current.push(end);
        }
    }
    flush(stroke, &mut current, &mut kept);
    kept
}

fn flush(stroke: &Stroke, current: &mut Vec<Point>, kept: &mut Vec<Stroke>) {
    if current.is_empty() {
        return;
    }
    kept.push(Stroke {
        points: std::mem::take(current),
        color: stroke.color,
        width: stroke.width,
    });
}

fn outside_spans(a: Point, b: Point, circle: Circle) -> Vec<(Point, Point)> {
    let mut cuts = vec![0.0, 1.0];
    for t in intersection_ts(a, b, circle) {
        if !cuts.iter().any(|cut| (cut - t).abs() < 1.0e-4) {
            cuts.push(t);
        }
    }
    cuts.sort_by(|left, right| left.total_cmp(right));

    let mut spans = Vec::new();
    for pair in cuts.windows(2) {
        let (t0, t1) = (pair[0], pair[1]);
        if t1 - t0 < 1.0e-4 {
            continue;
        }
        let mid = lerp(a, b, (t0 + t1) * 0.5);
        if !inside(mid, circle) {
            spans.push((lerp(a, b, t0), lerp(a, b, t1)));
        }
    }
    spans
}

fn intersection_ts(a: Point, b: Point, circle: Circle) -> Vec<f32> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let fx = a.x - circle.center.x;
    let fy = a.y - circle.center.y;
    let qa = dx * dx + dy * dy;
    if qa < 1.0e-12 {
        return Vec::new();
    }
    let qb = 2.0 * (fx * dx + fy * dy);
    let qc = fx * fx + fy * fy - circle.radius * circle.radius;
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return Vec::new();
    }
    let root = disc.sqrt();
    [(-qb - root) / (2.0 * qa), (-qb + root) / (2.0 * qa)]
        .into_iter()
        .filter(|t| (0.0..=1.0).contains(t))
        .collect()
}

fn lerp(a: Point, b: Point, t: f32) -> Point {
    Point {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(points: &[(f32, f32)]) -> Stroke {
        Stroke {
            points: points.iter().map(|(x, y)| Point { x: *x, y: *y }).collect(),
            color: [0, 0, 0, 255],
            width: 2.0,
        }
    }

    fn circle() -> Circle {
        Circle {
            center: Point { x: 0.0, y: 0.0 },
            radius: 10.0,
        }
    }

    #[test]
    fn keeps_strokes_outside_the_circle() {
        let kept = drop_inside(&[stroke(&[(-30.0, 0.0), (-20.0, 0.0)])], circle());
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].points.len(), 2);
    }

    #[test]
    fn drops_strokes_fully_inside() {
        let kept = drop_inside(&[stroke(&[(-2.0, 0.0), (2.0, 0.0)])], circle());
        assert!(kept.is_empty());
    }

    #[test]
    fn splits_a_stroke_that_passes_through() {
        let kept = drop_inside(&[stroke(&[(-30.0, 0.0), (30.0, 0.0)])], circle());
        assert_eq!(kept.len(), 2);
        assert!(kept[0].points[0].x < -10.0);
        assert!(kept[1].points.last().unwrap().x > 10.0);
    }

    #[test]
    fn trims_a_stroke_that_ends_inside() {
        let kept = drop_inside(&[stroke(&[(-30.0, 0.0), (0.0, 0.0)])], circle());
        assert_eq!(kept.len(), 1);
        let end = *kept[0].points.last().unwrap();
        assert!((end.dist(Point { x: 0.0, y: 0.0 }) - 10.0).abs() < 0.05);
    }
}
