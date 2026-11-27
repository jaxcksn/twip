//! DrawingML units

use std::ops::{Add, Neg, Sub};

/// English metric units in a point.
pub const EMU_PER_PT: i64 = 12700;

/// A length in English metric units: 914,400 to the inch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Emu(pub i64);

impl Emu {
    /// `pt` points, to the nearest unit.
    pub fn from_pt(pt: f64) -> Emu {
        Emu((pt * EMU_PER_PT as f64).round() as i64)
    }

    /// In points.
    pub fn to_pt(self) -> f64 {
        self.0 as f64 / EMU_PER_PT as f64
    }

    /// In twips, rounded half away from zero.
    pub fn to_twips_rounded(self) -> i64 {
        (self.0 * 1440 + 457_200 * self.0.signum()) / 914_400
    }

    /// In twips, truncated toward zero.
    pub fn to_twips_truncated(self) -> i64 {
        self.0 / 635
    }

    /// `ST_Coordinate`'s bound: any sum of a few coordinates from the file stays within `i64`.
    pub const MAX: i64 = 27_273_042_316_900;

    /// A coordinate attribute's value (`ST_Coordinate`): units, or a number with `mm`, `cm`, `in`, `pt`, `pc` or `pi`,
    /// bounded to [`Emu::MAX`].
    pub fn parse(s: &str) -> Option<Emu> {
        let s = s.trim();
        let bound = |v: i64| Emu(v.clamp(-Self::MAX, Self::MAX));
        if let Ok(v) = s.parse::<i64>() {
            return Some(bound(v));
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
        Some(bound((v * per).round() as i64))
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

/// An angle in 60,000ths of a degree, clockwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Angle(pub i32);

impl Angle {
    /// A full turn.
    pub const FULL: i32 = 21_600_000;

    /// `deg` degrees, to the nearest unit.
    pub fn from_degrees(deg: f64) -> Angle {
        Angle((deg * 60000.0).round() as i32)
    }

    /// In degrees.
    pub fn degrees(self) -> f64 {
        f64::from(self.0) / 60000.0
    }

    /// In radians.
    pub fn radians(self) -> f64 {
        self.degrees().to_radians()
    }

    /// The same angle within one turn, from 0 up to [`Angle::FULL`].
    pub fn normalized(self) -> Angle {
        Angle(self.0.rem_euclid(Self::FULL))
    }

    /// An angle attribute's value (`ST_Angle`), bounded to `i32`.
    pub fn parse(s: &str) -> Option<Angle> {
        s.trim().parse::<i64>().ok().map(|v| Angle(v.clamp(i32::MIN.into(), i32::MAX.into()) as i32))
    }
}

/// A percentage in 1,000ths of a percent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pct(pub i32);

impl Pct {
    /// 100%.
    pub const ONE: Pct = Pct(100_000);

    /// As a fraction: 1 is 100%.
    pub fn fraction(self) -> f64 {
        f64::from(self.0) / 100_000.0
    }

    /// The fraction `f` (1 is 100%), to the nearest unit.
    pub fn from_fraction(f: f64) -> Pct {
        Pct((f * 100_000.0).round() as i32)
    }

    /// A percentage attribute's value (`ST_Percentage`): units, or a number of percent with `%`, bounded to `i32`.
    pub fn parse(s: &str) -> Option<Pct> {
        let s = s.trim();
        if let Some(p) = s.strip_suffix('%') {
            return p.trim().parse::<f64>().ok().map(|v| Pct((v * 1000.0).round() as i32));
        }
        s.parse::<i64>().ok().map(|v| Pct(v.clamp(i32::MIN.into(), i32::MAX.into()) as i32))
    }
}

/// A percentage in 10ths of a percent.
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
        assert_eq!(Emu(318).to_twips_rounded(), 1);
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
        // Coordinates past ST_Coordinate's, so that their sums stay within i64.
        assert_eq!(Emu::parse("9223372036854775807"), Some(Emu(Emu::MAX)));
        assert_eq!(Emu::parse("-100000000000000in"), Some(Emu(-Emu::MAX)));
        assert_eq!(Pct::parse("50%"), Some(Pct(50000)));
        assert_eq!(Pct::parse("-25000"), Some(Pct(-25000)));
        assert_eq!(Angle::parse("-5400000").map(Angle::normalized), Some(Angle(16_200_000)));
        assert_eq!(Angle(5_400_000).degrees(), 90.0);
    }
}
