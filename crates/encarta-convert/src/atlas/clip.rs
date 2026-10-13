//! Clipping of rings and lines to an axis-aligned rectangle (tile plus buffer).

pub type P = (f64, f64);

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// Sutherland–Hodgman clipping of a closed ring (no repeated closing point). The result may
/// contain zero-width spikes along the rectangle edges where the ring leaves and re-enters;
/// they do not change the filled area.
pub fn clip_ring(ring: &[P], r: &Rect) -> Vec<P> {
    let mut pts: Vec<P> = ring.to_vec();
    for edge in 0..4 {
        if pts.is_empty() {
            break;
        }
        let inside = |p: &P| match edge {
            0 => p.0 >= r.x0,
            1 => p.0 <= r.x1,
            2 => p.1 >= r.y0,
            _ => p.1 <= r.y1,
        };
        let cross = |a: &P, b: &P| -> P {
            match edge {
                0 | 1 => {
                    let x = if edge == 0 { r.x0 } else { r.x1 };
                    let t = (x - a.0) / (b.0 - a.0);
                    (x, a.1 + t * (b.1 - a.1))
                }
                _ => {
                    let y = if edge == 2 { r.y0 } else { r.y1 };
                    let t = (y - a.1) / (b.1 - a.1);
                    (a.0 + t * (b.0 - a.0), y)
                }
            }
        };
        let mut out = Vec::with_capacity(pts.len() + 4);
        for i in 0..pts.len() {
            let cur = pts[i];
            let prev = pts[(i + pts.len() - 1) % pts.len()];
            match (inside(&prev), inside(&cur)) {
                (true, true) => out.push(cur),
                (true, false) => out.push(cross(&prev, &cur)),
                (false, true) => {
                    out.push(cross(&prev, &cur));
                    out.push(cur);
                }
                (false, false) => {}
            }
        }
        pts = out;
    }
    pts
}

/// Liang–Barsky clipping of a polyline; returns the pieces inside the rectangle.
pub fn clip_line(line: &[P], r: &Rect) -> Vec<Vec<P>> {
    let mut out: Vec<Vec<P>> = Vec::new();
    let mut cur: Vec<P> = Vec::new();
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        let mut keep = true;
        for (p, q) in [(-dx, a.0 - r.x0), (dx, r.x1 - a.0), (-dy, a.1 - r.y0), (dy, r.y1 - a.1)] {
            if p == 0.0 {
                if q < 0.0 {
                    keep = false;
                    break;
                }
            } else {
                let t = q / p;
                if p < 0.0 {
                    t0 = t0.max(t);
                } else {
                    t1 = t1.min(t);
                }
                if t0 > t1 {
                    keep = false;
                    break;
                }
            }
        }
        if !keep {
            if cur.len() >= 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
            continue;
        }
        let pa = (a.0 + t0 * dx, a.1 + t0 * dy);
        let pb = (a.0 + t1 * dx, a.1 + t1 * dy);
        if cur.is_empty() {
            cur.push(pa);
        } else if t0 > 0.0 {
            if cur.len() >= 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur = vec![pa];
        }
        cur.push(pb);
        if t1 < 1.0 {
            if cur.len() >= 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// Even-odd point-in-ring test.
pub fn point_in_ring(p: P, ring: &[P]) -> bool {
    let mut c = false;
    let n = ring.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (ring[i], ring[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            c = !c;
        }
        j = i;
    }
    c
}

/// Twice the signed area (shoelace).
pub fn area2(ring: &[P]) -> f64 {
    let n = ring.len();
    let mut s = 0.0;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        s += a.0 * b.1 - b.0 * a.1;
    }
    s
}

/// A point inside the ring: the centroid if it is inside, else the middle of the widest
/// horizontal span through the centroid's row (or the first vertex for degenerate rings).
pub fn interior_point(ring: &[P]) -> P {
    if ring.len() < 3 {
        return ring.first().copied().unwrap_or((0.0, 0.0));
    }
    let a = area2(ring);
    let (mut cx, mut cy) = (0.0, 0.0);
    for i in 0..ring.len() {
        let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
        let f = p.0 * q.1 - q.0 * p.1;
        cx += (p.0 + q.0) * f;
        cy += (p.1 + q.1) * f;
    }
    if a.abs() > 0.0 {
        let c = (cx / (3.0 * a), cy / (3.0 * a));
        if point_in_ring(c, ring) {
            return c;
        }
        let y = c.1;
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..ring.len() {
            let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
            if (p.1 > y) != (q.1 > y) {
                xs.push(p.0 + (y - p.1) * (q.0 - p.0) / (q.1 - p.1));
            }
        }
        xs.sort_by(f64::total_cmp);
        if let Some(best) = xs.as_chunks::<2>().0.iter().max_by(|a, b| (a[1] - a[0]).total_cmp(&(b[1] - b[0]))) {
            return ((best[0] + best[1]) / 2.0, y);
        }
    }
    ring[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_clipping() {
        let r = Rect { x0: 0.0, y0: 0.0, x1: 10.0, y1: 10.0 };
        let sq = [(-5.0, -5.0), (5.0, -5.0), (5.0, 5.0), (-5.0, 5.0)];
        let c = clip_ring(&sq, &r);
        assert!((area2(&c).abs() / 2.0 - 25.0).abs() < 1e-9);
        let outside = [(20.0, 20.0), (30.0, 20.0), (30.0, 30.0)];
        assert!(clip_ring(&outside, &r).is_empty());
        let inside = [(1.0, 1.0), (2.0, 1.0), (2.0, 2.0)];
        assert_eq!(clip_ring(&inside, &r), inside.to_vec());
    }

    #[test]
    fn line_clipping() {
        let r = Rect { x0: 0.0, y0: 0.0, x1: 10.0, y1: 10.0 };
        // Enters, leaves, re-enters.
        let l = [(-5.0, 5.0), (5.0, 5.0), (15.0, 5.0), (15.0, 8.0), (5.0, 8.0)];
        let parts = clip_line(&l, &r);
        assert_eq!(parts, vec![vec![(0.0, 5.0), (5.0, 5.0), (10.0, 5.0)], vec![(10.0, 8.0), (5.0, 8.0)]]);
        assert!(clip_line(&[(20.0, 0.0), (30.0, 0.0)], &r).is_empty());
    }

    #[test]
    fn interior_of_concave_ring() {
        // A "C" shape whose centroid lies in the notch.
        let c = [(0.0, 0.0), (10.0, 0.0), (10.0, 2.0), (2.0, 2.0), (2.0, 8.0), (10.0, 8.0), (10.0, 10.0), (0.0, 10.0)];
        let p = interior_point(&c);
        assert!(point_in_ring(p, &c), "{p:?}");
    }
}
