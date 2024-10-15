pub struct Collision {
    pub outer_distance: f32,
    pub depth: f32,
    pub min_stroke: f32,
}

impl Collision {
    pub fn get_stroke_range(&self, a1: f32, a2: f32) -> (f64, f64) {
        let min = f32::min(a1, a2);
        let max = f32::max(a1, a2);

        if min >= self.outer_distance {
            // out of upper limits
            (1.0, 1.0)
        } else if max <= (self.outer_distance - self.depth) {
            (0.0, 0.0)
        } else {
            // any collision beyond max depth is ignored
            let ignored_depth = self.outer_distance - self.depth;

            // normalize positions to zero
            let zmin = f32::max(min - ignored_depth, 0.0);
            let zmax = f32::min(max - ignored_depth, self.depth);

            // normalize entire collision depth as 1.0 len
            let mut lower = zmin / self.depth;
            let mut upper = zmax / self.depth;

            // assure length is at least min_stroke
            if upper - lower < self.min_stroke {
                if lower - self.min_stroke < 0.0 {
                    upper = lower + self.min_stroke;
                } else {
                    lower = upper - self.min_stroke;
                }
            }
            (lower.into(), upper.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::dynamic_tracking::collision::*;
    use assert_float_eq::*;

    #[tokio::test]
    pub async fn get_stroke_range_intervall_correct() {
        let c = Collision {
            outer_distance: 10.0,
            depth: 6.0,
            min_stroke: 0.25,
        };

        assert_eq!(
            c.get_stroke_range(4.0, 4.0),
            (0.0, 0.0),
            "out of lower limits 1"
        );
        assert_eq!(
            c.get_stroke_range(-4.0, -7.0),
            (0.0, 0.0),
            "out of lower limits 1"
        );
        assert_eq!(
            c.get_stroke_range(10.0, 11.0),
            (1.0, 1.0),
            "out of upper limits"
        );
        assert_eq!(
            c.get_stroke_range(999.0, 999.0),
            (1.0, 1.0),
            "out of upper limits"
        );

        assert_eq!(c.get_stroke_range(4.0, 10.0), (0.0, 1.0), "full range");
        assert_eq!(
            c.get_stroke_range(7.0, 10.0),
            (0.5, 1.0),
            "half range upper"
        );
        assert_eq!(c.get_stroke_range(4.0, 7.0), (0.0, 0.5), "half range lower");
        assert_eq!(
            c.get_stroke_range(5.5, 8.5),
            (0.25, 0.75),
            "somewhere in the middle"
        );
    }

    #[tokio::test]
    pub async fn get_stroke_range_min_stroke() {
        let c = Collision {
            outer_distance: 10.0,
            depth: 10.0,
            min_stroke: 0.25,
        };
        assert_range_equal(c.get_stroke_range(9.0, 10.0), (0.75, 1.0)); // "upper end"
        assert_range_equal(c.get_stroke_range(7.0, 8.0), (0.55, 0.8)); // upper end middle
        assert_range_equal(c.get_stroke_range(0.0, 1.0), (0.0, 0.25)); // lower end
        assert_range_equal(c.get_stroke_range(1.0, 2.0), (0.1, 0.35)); // lower end middle
    }

    pub fn assert_range_equal(r1: (f64, f64), r2: (f64, f64)) {
        assert_float_relative_eq!(r1.0, r2.0, 0.001);
        assert_float_relative_eq!(r1.1, r2.1, 0.001);
    }
}
