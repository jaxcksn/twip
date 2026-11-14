//! Layout units

pub const LU_PER_PT: f64 = 4096.0;

/// One pixel of the 600-dpi device Word's PDF export lays text out on, in layout units (0.12pt).
pub const DEVICE_PX: f64 = LU_PER_PT * 72.0 / 600.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lu(pub i32);

impl Lu {
    pub fn from_font(v: f64, size: f64, upem: f64) -> Lu {
        let hp = 2.0 * size;
        Lu(((v * hp * 2048.0 + (upem / 2.0).floor()) / upem).floor() as i32)
    }

    pub fn round_half_up(pt: f64) -> Lu {
        Lu((pt * LU_PER_PT + 0.5).floor() as i32)
    }

    pub fn to_pt(self) -> f64 {
        f64::from(self.0) / LU_PER_PT
    }

    /// The nearest whole number of device pixels, half up.
    pub fn device_px(self) -> i64 {
        (i64::from(self.0) * 600 + 147456).div_euclid(294912)
    }

    pub const MAX: Lu = Lu(i32::MAX);

    pub fn holds(self, pt: f64) -> bool {
        Lu::round_half_up(pt) <= self
    }
}

#[cfg(test)]
mod tests {
    use super::Lu;

    #[test]
    fn font_metrics_round_half_up_per_run() {
        assert_eq!(Lu::from_font(1854.0, 11.0, 2048.0), Lu(40788));
        assert_eq!(Lu::from_font(2048.0, 10.5, 2048.0), Lu(43008));
        assert_eq!(Lu::from_font(220.0, 11.0, 256.0), Lu(38720));
        assert_eq!(Lu::from_font(905.0, 12.0, 1000.0), Lu(44483));
        assert_eq!(Lu::from_font(1001.0, 12.0, 3000.0), Lu(16400));
        assert_eq!(Lu(4096).to_pt(), 1.0);
    }
}
