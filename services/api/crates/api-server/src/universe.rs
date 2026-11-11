//! The list of symbols the background scanner walks.
//!
//! Source, first match wins: `UNIVERSE_FILE`, `UNIVERSE`, `WATCHLIST`. The
//! `WATCHLIST` is always added on top, so your own stocks are scanned even
//! when they are not in the index file.
//! The file is either one symbol per line or a CSV with a `Symbol` column;
//! NSE's index constituent files (`Company Name,Industry,Symbol,Series,ISIN
//! Code`) work as downloaded, and their names and industries are kept.

use std::collections::HashSet;

use anyhow::Context;

use crate::config::Config;
use crate::params;

/// Never scanned, whatever the configuration says. The owner already holds
/// Indraprastha Medical Corporation, so it must never appear in a top list.
pub const ALWAYS_EXCLUDED: &[&str] = &["INDRAMEDCO"];

/// One symbol to scan, with whatever the source file says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniverseEntry {
    pub symbol: String,
    pub name: Option<String>,
    pub industry: Option<String>,
}

impl UniverseEntry {
    fn bare(symbol: String) -> Self {
        Self {
            symbol,
            name: None,
            industry: None,
        }
    }
}

/// Symbols to scan, after exclusions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Universe {
    pub entries: Vec<UniverseEntry>,
    /// Every excluded symbol (always-excluded plus `SCAN_EXCLUDE`), sorted.
    pub excluded: Vec<String>,
}

/// Builds the universe from the configuration.
pub fn load(config: &Config) -> anyhow::Result<Universe> {
    let entries = if let Some(path) = &config.universe_file {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("UNIVERSE_FILE: cannot read {}", path.display()))?;
        let entries = parse_file(&text);
        anyhow::ensure!(
            !entries.is_empty(),
            "UNIVERSE_FILE: no valid symbols in {}",
            path.display()
        );
        entries
    } else {
        let listed = params::parse_symbols_lenient(&config.universe)
            .map_err(|e| anyhow::anyhow!("UNIVERSE: {e}"))?;
        let symbols = if listed.is_empty() {
            config.watchlist()
        } else {
            listed
        };
        symbols.into_iter().map(UniverseEntry::bare).collect()
    };
    let entries = with_watchlist(entries, config.watchlist());
    let extra = params::parse_symbols_lenient(&config.scan_exclude)
        .map_err(|e| anyhow::anyhow!("SCAN_EXCLUDE: {e}"))?;
    Ok(apply_exclusions(entries, &extra))
}

/// Appends watchlist symbols missing from `entries`, keeping file order first.
fn with_watchlist(mut entries: Vec<UniverseEntry>, watchlist: Vec<String>) -> Vec<UniverseEntry> {
    for symbol in watchlist {
        if !entries.iter().any(|e| e.symbol == symbol) {
            entries.push(UniverseEntry::bare(symbol));
        }
    }
    entries
}

fn apply_exclusions(entries: Vec<UniverseEntry>, extra: &[String]) -> Universe {
    let mut excluded: Vec<String> = ALWAYS_EXCLUDED.iter().map(|s| (*s).to_owned()).collect();
    for s in extra {
        if !excluded.contains(s) {
            excluded.push(s.clone());
        }
    }
    excluded.sort();
    let entries = entries
        .into_iter()
        .filter(|e| !excluded.contains(&e.symbol))
        .collect();
    Universe { entries, excluded }
}

/// Parses a universe file. Invalid lines are skipped with a warning.
pub fn parse_file(text: &str) -> Vec<UniverseEntry> {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim().trim_start_matches('\u{feff}'))
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let Some(first) = lines.first() else {
        return Vec::new();
    };

    let header: Vec<String> = split_csv(first)
        .iter()
        .map(|h| h.to_ascii_lowercase())
        .collect();
    let symbol_col = header.iter().position(|h| h == "symbol");
    let (rows, symbol_col, name_col, industry_col) = match symbol_col {
        Some(col) => (
            &lines[1..],
            col,
            header
                .iter()
                .position(|h| h == "company name" || h == "name"),
            header.iter().position(|h| h == "industry"),
        ),
        None => (&lines[..], 0, None, None),
    };

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut skipped = 0usize;
    for line in rows {
        let fields = split_csv(line);
        let cell = |col: Option<usize>| {
            col.and_then(|c| fields.get(c))
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        };
        let Some(raw) = cell(Some(symbol_col)) else {
            skipped += 1;
            continue;
        };
        match params::normalize_symbol(&raw) {
            Ok(symbol) if seen.insert(symbol.clone()) => out.push(UniverseEntry {
                symbol,
                name: cell(name_col),
                industry: cell(industry_col),
            }),
            Ok(_) => {}
            Err(_) => skipped += 1,
        }
    }
    if skipped > 0 {
        tracing::warn!(skipped, "skipped invalid lines in UNIVERSE_FILE");
    }
    out
}

/// Splits one CSV line, honouring double quotes (`"a, b"` and `""`).
fn split_csv(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    fields.push(field);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::config_from;

    #[test]
    fn reads_nse_constituent_csv() {
        let csv = "\u{feff}Company Name,Industry,Symbol,Series,ISIN Code\n\
                   \"Action Construction Equipment Ltd.\",Capital Goods,ACE,EQ,INE731H01025\n\
                   Indraprastha Medical Corporation Ltd.,Healthcare,INDRAMEDCO,EQ,INE681B01017\n\
                   Mahindra & Mahindra Ltd.,Automobile,M&M,EQ,INE101A01026\n\
                   Bad Row,Nothing,NOT OK,EQ,X\n\
                   Duplicate,Capital Goods,ace,EQ,INE731H01025\n";
        let entries = parse_file(csv);
        let symbols: Vec<&str> = entries.iter().map(|e| e.symbol.as_str()).collect();
        assert_eq!(symbols, vec!["ACE", "INDRAMEDCO", "M&M"]);
        assert_eq!(
            entries[0].name.as_deref(),
            Some("Action Construction Equipment Ltd.")
        );
        assert_eq!(entries[0].industry.as_deref(), Some("Capital Goods"));

        let universe = apply_exclusions(entries, &["M&M".to_owned()]);
        let kept: Vec<&str> = universe.entries.iter().map(|e| e.symbol.as_str()).collect();
        assert_eq!(kept, vec!["ACE"]);
        assert_eq!(universe.excluded, vec!["INDRAMEDCO", "M&M"]);
    }

    #[test]
    fn reads_plain_symbol_lists() {
        let entries = parse_file("# my list\ncoforge\n\nMCX, ignored column\n");
        let symbols: Vec<&str> = entries.iter().map(|e| e.symbol.as_str()).collect();
        assert_eq!(symbols, vec!["COFORGE", "MCX"]);
        assert!(entries.iter().all(|e| e.name.is_none()));
        assert!(parse_file("").is_empty());
    }

    #[test]
    fn splits_quoted_csv_fields() {
        assert_eq!(
            split_csv(r#"a,"b, c","say ""hi""",,"#),
            vec!["a", "b, c", r#"say "hi""#, "", ""]
        );
    }

    #[test]
    fn falls_back_to_universe_then_watchlist() {
        let base = [
            "--api-token",
            "test-token-0123456789",
            "--data-providers",
            "demo",
        ];
        let mut args = base.to_vec();
        args.extend(["--watchlist", "IEX,INDRAMEDCO"]);
        let u = load(&config_from(&args)).unwrap();
        assert_eq!(u.entries, vec![UniverseEntry::bare("IEX".to_owned())]);
        assert_eq!(u.excluded, vec!["INDRAMEDCO"]);

        // UNIVERSE plus the watchlist (IEX), minus SCAN_EXCLUDE.
        let mut args = base.to_vec();
        args.extend([
            "--universe",
            "bse,cams",
            "--scan-exclude",
            "cams",
            "--watchlist",
            "IEX,BSE",
        ]);
        let u = load(&config_from(&args)).unwrap();
        let symbols: Vec<&str> = u.entries.iter().map(|e| e.symbol.as_str()).collect();
        assert_eq!(symbols, vec!["BSE", "IEX"]);
        assert_eq!(u.excluded, vec!["CAMS", "INDRAMEDCO"]);

        let mut args = base.to_vec();
        args.extend(["--universe-file", "/nonexistent/swing-trade/universe.csv"]);
        assert!(load(&config_from(&args)).is_err());
    }
}
