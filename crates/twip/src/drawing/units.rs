//! DrawingML units: EMU lengths, angles in 60000ths of a degree and percentages in 100000ths.

use std::ops::{Add, Neg, Sub};

/// EMU per point.
pub const EMU_PER_PT: i64 = 12700;

/// A length in English Metric Units: 914400 per inch, 12700 per point, 635 per twip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Emu(pub i64);

impl Emu {
    /// `pt` points, rounded to the nearest EMU.
    pub fn from_pt(pt: f64) -> Emu {
        Emu((pt * EMU_PER_PT as f64).round() as i64)
    }

    /// Points (exact for every EMU a document can hold).
    pub fn to_pt(self) -> f64 {
        self.0 as f64 / EMU_PER_PT as f64
    }

    /// Whole twips, rounded half away from zero: `trunc((e × 1440 + 457200 × sign(e)) / 914400)` (FLOAT-10).
    pub fn to_twips_rounded(self) -> i64 {
        (self.0 * 1440 + 457_200 * self.0.signum()) / 914_400
    }

    /// Whole twips, truncated toward zero (FLOAT-7).
    pub fn to_twips_truncated(self) -> i64 {
        self.0 / 635
    }

    /// Parses an `ST_Coordinate` attribute value (an integer; a universal measure such as `1in` or `12pt` too).
    pub fn parse(s: &str) -> Option<Emu> {
        let s = s.trim();
        if let Ok(v) = s.parse::<i64>() {
            return Some(Emu(v));
        }
        let (num, unit) = s.split_at(s.find(|c: char| c.is_ascii_alphabetic())?);
        let v: f64 = num.trim().parse().ok()?;
        let per = match unit {
            "mm" => 36_000.0,
            "cm" => 360_000.0,
            "in" => 914_400.0,
            "pt" => 12_700.0,
            "pc" | "pi" => 152_400.0,
            _ => return None,
        };
        Some(Emu((v * per).round() as i64))
    }
}

impl Add for Emu {
    type Output = Emu;
    fn add(self, o: Emu) -> Emu {
        Emu(self.0 + o.0)
    }
}

impl Sub for Emu {
    type Output = Emu;
    fn sub(self, o: Emu) -> Emu {
        Emu(self.0 - o.0)
    }
}

impl Neg for Emu {
    type Output = Emu;
    fn neg(self) -> Emu {
        Emu(-self.0)
    }
}

/// An angle in 60000ths of a degree (`ST_Angle`; 21600000 is a full turn). Signed: a sweep may be negative.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Angle(pub i32);

impl Angle {
    pub const FULL: i32 = 21_600_000;

    pub fn from_degrees(deg: f64) -> Angle {
        Angle((deg * 60000.0).round() as i32)
    }

    pub fn degrees(self) -> f64 {
        f64::from(self.0) / 60000.0
    }

    pub fn radians(self) -> f64 {
        self.degrees().to_radians()
    }

    /// The same direction in [0, 21600000) (a rotation; not a sweep).
    pub fn normalized(self) -> Angle {
        Angle(self.0.rem_euclid(Self::FULL))
    }

    pub fn parse(s: &str) -> Option<Angle> {
        s.trim().parse::<i64>().ok().map(|v| Angle(v.clamp(i32::MIN.into(), i32::MAX.into()) as i32))
    }
}

/// A fraction in 100000ths (`ST_Percentage`, `ST_PositiveFixedPercentage`: 100000 = 100 %). May be negative or above
/// 100 % where the schema allows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pct(pub i32);

impl Pct {
    pub const ONE: Pct = Pct(100_000);

    pub fn fraction(self) -> f64 {
        f64::from(self.0) / 100_000.0
    }

    pub fn from_fraction(f: f64) -> Pct {
        Pct((f * 100_000.0).round() as i32)
    }

    /// An integer, or a percentage string (`"50%"`, which the strict schema writes) as 100000ths.
    pub fn parse(s: &str) -> Option<Pct> {
        let s = s.trim();
        if let Some(p) = s.strip_suffix('%') {
            return p.trim().parse::<f64>().ok().map(|v| Pct((v * 1000.0).round() as i32));
        }
        s.parse::<i64>().ok().map(|v| Pct(v.clamp(i32::MIN.into(), i32::MAX.into()) as i32))
    }
}

/// A fraction in 1000ths (`ST_TextFontScalePercentOrPercentString`, VML fractions `f` are 65536ths: see vml.rs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pct1000(pub i32);

impl From<Pct1000> for Pct {
    fn from(p: Pct1000) -> Pct {
        Pct(p.0 * 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emu_to_twips_rounds_half_away_from_zero() {
        assert_eq!(Emu(317).to_twips_rounded(), 0);
        assert_eq!(Emu(318).to_twips_rounded(), 1); // 0.5008 twip
        assert_eq!(Emu(-318).to_twips_rounded(), -1);
        assert_eq!(Emu(144212).to_twips_rounded(), 227);
        assert_eq!(Emu(98492).to_twips_rounded(), 155);
        assert_eq!(Emu(634).to_twips_truncated(), 0);
        assert_eq!(Emu(-1270).to_twips_truncated(), -2);
    }

    #[test]
    fn units_parse_their_attribute_forms() {
        assert_eq!(Emu::parse("12700"), Some(Emu(12700)));
        assert_eq!(Emu::parse("1in"), Some(Emu(914400)));
        assert_eq!(Emu::parse("2.5pt"), Some(Emu(31750)));
        assert_eq!(Pct::parse("50%"), Some(Pct(50000)));
        assert_eq!(Pct::parse("-25000"), Some(Pct(-25000)));
        assert_eq!(Angle::parse("-5400000").map(Angle::normalized), Some(Angle(16_200_000)));
        assert_eq!(Angle(5_400_000).degrees(), 90.0);
    }
}
