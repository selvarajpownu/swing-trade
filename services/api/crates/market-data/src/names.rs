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

const US_NAMES: &[(&str, &str)] = &[
    ("AAPL", "Apple"),
    ("AMZN", "Amazon"),
    ("GOOGL", "Alphabet"),
    ("META", "Meta Platforms"),
    ("MSFT", "Microsoft"),
    ("NVDA", "NVIDIA"),
    ("TSLA", "Tesla"),
];

const MCX_NAMES: &[(&str, &str)] = &[
    ("COPPER", "Copper"),
    ("CRUDEOIL", "Crude Oil"),
    ("GOLD", "Gold"),
    ("NATURALGAS", "Natural Gas"),
    ("SILVER", "Silver"),
];

/// Display name for a symbol in `market`, falling back to the symbol.
pub(crate) fn display_name(market: crate::Market, symbol: &str) -> String {
    let table = match market {
        crate::Market::In => NAMES,
        crate::Market::Us => US_NAMES,
        crate::Market::Mcx => MCX_NAMES,
    };
    table
        .iter()
        .find(|(s, _)| s.eq_ignore_ascii_case(symbol))
        .map_or_else(|| symbol.to_owned(), |(_, n)| (*n).to_owned())
}

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
        assert_eq!(display_name(crate::Market::Us, "aapl"), "Apple");
        assert_eq!(display_name(crate::Market::Mcx, "GOLD"), "Gold");
        assert_eq!(display_name(crate::Market::Us, "XYZ"), "XYZ");
    }
}
