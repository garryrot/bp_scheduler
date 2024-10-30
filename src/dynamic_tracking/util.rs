
// todo: limit_speed_by_shortening_distance
pub fn limit_speed(from_pos: f64, to_pos: f64, duration_ms: u32, min_duration_full_range: u32) -> f64 {
    if duration_ms < min_duration_full_range { // self.settings.stroke_min_ms
        let max_dist = duration_ms as f64 / min_duration_full_range as f64;
        let mut dist = to_pos - from_pos;
        if dist < 0.0 && dist < -max_dist {
            dist = -max_dist;
        }
        if dist > 0.0 && dist > max_dist {
            dist = max_dist;
        }
        from_pos + dist
    } else {
        to_pos
    }
}

#[cfg(test)]
mod tests {
    use crate::dynamic_tracking::util::*;

    #[tokio::test]
    pub async fn max_distance_test() {
        assert_eq!(limit_speed(0.0, 1.0, 200, 200), 1.0, "returns actual target if speed in range");
        assert_eq!(limit_speed(1.0, 0.0, 200, 200), 0.0, "works in reverse");
        assert_eq!(limit_speed(0.0, 1.0, 100, 200), 0.5, "moves 50% of the range if the speed is 2x to fast");
        assert_eq!(limit_speed(0.0, 1.0, 50, 200), 0.25, "moves 25% of the range if the speed is 4x too fast ");
        assert_eq!(limit_speed(0.75, 0.0, 100, 200), 0.25, "moves 75% of the range of the speed 25% to ");
    }
}
