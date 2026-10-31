//! The layout unit (UNIT-4): 1/4096 pt, the grid every vertical quantity and fit test is on.

/// Layout units per point.
pub const LU_PER_PT: f64 = 4096.0;

/// A length in layout units, 1/4096 pt (294912 per inch, 204.8 per twip).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lu(pub i32);

impl Lu {
    /// A font-unit metric `v` of a face with `upem` units per em, at `size` points: `(v × hp × 2048 + upem div 2) div
    /// upem` with `hp` the size in half-points (UNIT-4).
    pub fn from_font(v: f64, size: f64, upem: f64) -> Lu {
        let hp = 2.0 * size;
        Lu(((v * hp * 2048.0 + (upem / 2.0).floor()) / upem).floor() as i32)
    }

    /// `pt` points to the nearest layout unit, halves up.
    pub fn round_half_up(pt: f64) -> Lu {
        Lu((pt * LU_PER_PT + 0.5).floor() as i32)
    }

    /// Exact: every layout unit is a whole number of 1/4096 pt.
    pub fn to_pt(self) -> f64 {
        f64::from(self.0) / LU_PER_PT
    }
}

#[cfg(test)]
mod tests {
    use super::Lu;

    #[test]
    fn font_metrics_round_half_up_per_run() {
        // Arial (upem 2048) win ascent 1854 at 11 pt (hp 22): 1854 × 22 = 40788 lu exactly.
        assert_eq!(Lu::from_font(1854.0, 11.0, 2048.0), Lu(40788));
        // A full em at 10.5 pt (hp 21) is 21 × 2048 lu.
        assert_eq!(Lu::from_font(2048.0, 10.5, 2048.0), Lu(43008));
        // MS Gothic (upem 256) 220 at 11 pt: (220 × 22 × 2048 + 128) div 256 = 38720.
        assert_eq!(Lu::from_font(220.0, 11.0, 256.0), Lu(38720));
        // Remainders round half up: (905 × 24 × 2048 + 500) div 1000 = 44483 (44482.56 exactly).
        assert_eq!(Lu::from_font(905.0, 12.0, 1000.0), Lu(44483));
        // 1001 × 24 × 2048 / 3000 = 16400.38: down.
        assert_eq!(Lu::from_font(1001.0, 12.0, 3000.0), Lu(16400));
        assert_eq!(Lu(4096).to_pt(), 1.0);
    }
}
