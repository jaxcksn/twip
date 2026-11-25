//! Line style values: `ST_Underline`

use crate::document::line::Underline;

/// The underline a `w:u/@w:val` names: single for a value the format doesn't define.
pub fn underline(v: &str) -> Underline {
    use Underline::*;
    match v {
        "none" => None,
        "words" => Words,
        "double" => Double,
        "thick" => Thick,
        "dotted" => Dotted,
        "dottedHeavy" => DottedHeavy,
        "dash" => Dash,
        "dashedHeavy" => DashedHeavy,
        "dashLong" => DashLong,
        "dashLongHeavy" => DashLongHeavy,
        "dotDash" => DotDash,
        "dashDotHeavy" => DashDotHeavy,
        "dotDotDash" => DotDotDash,
        "dashDotDotHeavy" => DashDotDotHeavy,
        "wave" => Wave,
        "wavyHeavy" => WavyHeavy,
        "wavyDouble" => WavyDouble,
        _ => Single,
    }
}

/// The `w:u/@w:val` of `u`.
pub fn underline_val(u: Underline) -> &'static str {
    use Underline::*;
    match u {
        None => "none",
        Single => "single",
        Words => "words",
        Double => "double",
        Thick => "thick",
        Dotted => "dotted",
        DottedHeavy => "dottedHeavy",
        Dash => "dash",
        DashedHeavy => "dashedHeavy",
        DashLong => "dashLong",
        DashLongHeavy => "dashLongHeavy",
        DotDash => "dotDash",
        DashDotHeavy => "dashDotHeavy",
        DotDotDash => "dotDotDash",
        DashDotDotHeavy => "dashDotDotHeavy",
        Wave => "wave",
        WavyHeavy => "wavyHeavy",
        WavyDouble => "wavyDouble",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `ST_Underline` value.
    const UNDERLINES: [&str; 18] = [
        "single",
        "words",
        "double",
        "thick",
        "dotted",
        "dottedHeavy",
        "dash",
        "dashedHeavy",
        "dashLong",
        "dashLongHeavy",
        "dotDash",
        "dashDotHeavy",
        "dotDotDash",
        "dashDotDotHeavy",
        "wave",
        "wavyHeavy",
        "wavyDouble",
        "none",
    ];

    #[test]
    fn every_underline_round_trips() {
        let read: std::collections::HashSet<Underline> = UNDERLINES.iter().map(|v| underline(v)).collect();
        assert_eq!(read.len(), UNDERLINES.len(), "each value is its own underline");
        for v in UNDERLINES {
            assert_eq!(underline_val(underline(v)), v);
        }
        assert_eq!(underline("bogus"), Underline::Single);
    }
}
