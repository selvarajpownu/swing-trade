//! Company names for common NSE symbols.
//!
//! Used for demo display names and as a second lookup query when a bare
//! ticker is ambiguous upstream (INDmoney's search for `ACE` returns other
//! companies, while "Action Construction Equipment" resolves correctly).

const NAMES: &[(&str, &str)] = &[
    ("ACE", "Action Construction Equipment"),
    ("AJAXENGG", "Ajax Engineering"),
    ("ANTHEM", "Anthem Biosciences"),
    ("ASKAUTOLTD", "ASK Automotive"),
    ("BSE", "BSE"),
    ("CAMS", "Computer Age Management Services"),
    ("COFORGE", "Coforge"),
    ("HDFCAMC", "HDFC Asset Management Company"),
    ("IEX", "Indian Energy Exchange"),
    ("LUPIN", "Lupin"),
    ("MCX", "Multi Commodity Exchange of India"),
    ("OFSS", "Oracle Financial Services Software"),
    ("PERSISTENT", "Persistent Systems"),
    ("TIPSMUSIC", "Tips Music"),
];

/// Company name for a known symbol (case-insensitive).
pub(crate) fn company_name(symbol: &str) -> Option<&'static str> {
    NAMES
        .iter()
        .find(|(s, _)| s.eq_ignore_ascii_case(symbol))
        .map(|(_, n)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_known_symbols_case_insensitively() {
        assert_eq!(company_name("ace"), Some("Action Construction Equipment"));
        assert_eq!(
            company_name("MCX"),
            Some("Multi Commodity Exchange of India")
        );
        assert_eq!(company_name("UNKNOWN"), None);
    }
}
