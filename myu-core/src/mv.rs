use crate::{ParseMvError, Sq, format_sq, parse_sq};

/// A single move: placement at a square.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mv {
    /// The square where the piece is placed.
    pub sq: Sq,
    /// Number of yugos formed by this move (0 if unknown or not encoded).
    pub yugos_formed: u8,
}

impl Mv {
    /// Create a move without yugo information.
    #[inline]
    pub fn new(sq: Sq) -> Self {
        Self { sq, yugos_formed: 0 }
    }

    /// Create a move with yugo count.
    #[inline]
    pub fn with_yugos(sq: Sq, yugos_formed: u8) -> Self {
        Self { sq, yugos_formed }
    }
}

/// Move format for parsing and stringification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MvFormat {
    /// Plain square notation: "d4"
    #[default]
    Plain,
    /// Yugo count in parentheses: "d6 (1 yugo)" or "d4" if 0
    YugosParens,
    /// Yugo count as plus suffix: "d6+" or "d4" if 0
    YugosPlus,
}

/// Parse a move in the given format.
pub fn parse_mv(s: &str, format: MvFormat) -> Result<Mv, ParseMvError> {
    match format {
        MvFormat::Plain => {
            let sq = parse_sq(s)?;
            Ok(Mv::new(sq))
        }
        MvFormat::YugosParens => parse_mv_parens(s),
        MvFormat::YugosPlus => parse_mv_plus(s),
    }
}

/// Parse a move, auto-detecting the format.
pub fn parse_mv_auto(s: &str) -> Result<Mv, ParseMvError> {
    // Try to detect format by looking at the content
    if s.contains('(') {
        parse_mv_parens(s)
    } else if s.ends_with('+') {
        parse_mv_plus(s)
    } else {
        let sq = parse_sq(s)?;
        Ok(Mv::new(sq))
    }
}

fn parse_mv_parens(s: &str) -> Result<Mv, ParseMvError> {
    // Format: "d6 (1 yugo)" or "d6 (2 yugos)" or just "d6"
    if let Some(paren_start) = s.find('(') {
        let sq_part = s[..paren_start].trim();
        let sq = parse_sq(sq_part)?;

        let rest = &s[paren_start + 1..];
        let paren_end = rest.find(')').ok_or(ParseMvError::BadYugoFormat)?;
        let inner = rest[..paren_end].trim();

        // Parse "N yugo" or "N yugos"
        let parts: Vec<&str> = inner.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(ParseMvError::BadYugoFormat);
        }
        let count: u8 = parts[0].parse().map_err(|_| ParseMvError::BadYugoFormat)?;
        if !parts[1].starts_with("yugo") {
            return Err(ParseMvError::BadYugoFormat);
        }
        Ok(Mv::with_yugos(sq, count))
    } else {
        let sq = parse_sq(s)?;
        Ok(Mv::new(sq))
    }
}

fn parse_mv_plus(s: &str) -> Result<Mv, ParseMvError> {
    // Format: "d6+" or "d6++" or just "d6"
    let plus_count = s.bytes().rev().take_while(|&b| b == b'+').count() as u8;
    let sq_part = &s[..s.len() - plus_count as usize];
    let sq = parse_sq(sq_part)?;
    Ok(Mv::with_yugos(sq, plus_count))
}

/// Format a move in the given format.
pub fn format_mv(mv: Mv, format: MvFormat) -> String {
    let sq_str = format_sq(mv.sq);
    match format {
        MvFormat::Plain => sq_str,
        MvFormat::YugosParens => {
            if mv.yugos_formed == 0 {
                sq_str
            } else if mv.yugos_formed == 1 {
                format!("{sq_str} (1 yugo)")
            } else {
                format!("{sq_str} ({} yugos)", mv.yugos_formed)
            }
        }
        MvFormat::YugosPlus => {
            if mv.yugos_formed == 0 {
                sq_str
            } else {
                format!("{sq_str}{}", "+".repeat(mv.yugos_formed as usize))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plain() {
        let mv = parse_mv("d4", MvFormat::Plain).unwrap();
        assert_eq!(mv.sq, parse_sq("d4").unwrap());
        assert_eq!(mv.yugos_formed, 0);
    }

    #[test]
    fn test_parse_parens() {
        let mv = parse_mv("d6 (1 yugo)", MvFormat::YugosParens).unwrap();
        assert_eq!(format_sq(mv.sq), "d6");
        assert_eq!(mv.yugos_formed, 1);

        let mv = parse_mv("e4 (2 yugos)", MvFormat::YugosParens).unwrap();
        assert_eq!(format_sq(mv.sq), "e4");
        assert_eq!(mv.yugos_formed, 2);

        let mv = parse_mv("a1", MvFormat::YugosParens).unwrap();
        assert_eq!(format_sq(mv.sq), "a1");
        assert_eq!(mv.yugos_formed, 0);
    }

    #[test]
    fn test_parse_plus() {
        let mv = parse_mv("d6+", MvFormat::YugosPlus).unwrap();
        assert_eq!(format_sq(mv.sq), "d6");
        assert_eq!(mv.yugos_formed, 1);

        let mv = parse_mv("e4++", MvFormat::YugosPlus).unwrap();
        assert_eq!(format_sq(mv.sq), "e4");
        assert_eq!(mv.yugos_formed, 2);

        let mv = parse_mv("a1", MvFormat::YugosPlus).unwrap();
        assert_eq!(format_sq(mv.sq), "a1");
        assert_eq!(mv.yugos_formed, 0);
    }

    #[test]
    fn test_parse_auto() {
        let mv = parse_mv_auto("d4").unwrap();
        assert_eq!(mv.yugos_formed, 0);

        let mv = parse_mv_auto("d6 (1 yugo)").unwrap();
        assert_eq!(mv.yugos_formed, 1);

        let mv = parse_mv_auto("e4++").unwrap();
        assert_eq!(mv.yugos_formed, 2);
    }

    #[test]
    fn test_format_plain() {
        let mv = Mv::with_yugos(parse_sq("d6").unwrap(), 1);
        assert_eq!(format_mv(mv, MvFormat::Plain), "d6");
    }

    #[test]
    fn test_format_parens() {
        let mv = Mv::new(parse_sq("d4").unwrap());
        assert_eq!(format_mv(mv, MvFormat::YugosParens), "d4");

        let mv = Mv::with_yugos(parse_sq("d6").unwrap(), 1);
        assert_eq!(format_mv(mv, MvFormat::YugosParens), "d6 (1 yugo)");

        let mv = Mv::with_yugos(parse_sq("e4").unwrap(), 2);
        assert_eq!(format_mv(mv, MvFormat::YugosParens), "e4 (2 yugos)");
    }

    #[test]
    fn test_format_plus() {
        let mv = Mv::new(parse_sq("d4").unwrap());
        assert_eq!(format_mv(mv, MvFormat::YugosPlus), "d4");

        let mv = Mv::with_yugos(parse_sq("d6").unwrap(), 1);
        assert_eq!(format_mv(mv, MvFormat::YugosPlus), "d6+");

        let mv = Mv::with_yugos(parse_sq("e4").unwrap(), 3);
        assert_eq!(format_mv(mv, MvFormat::YugosPlus), "e4+++");
    }
}
