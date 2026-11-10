//! `GET /v1/news`: Google News headlines for a market's watchlist or one stock.

use std::collections::{HashMap, HashSet};

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::Json;
use futures::stream::{self, StreamExt};
use market_data::Market;
use serde::Deserialize;

use crate::dto::{Headline, HeadlinesResponse};
use crate::error::ApiError;
use crate::news::query_for;
use crate::params;
use crate::state::AppState;

/// Feeds fetched at once (one per stock).
const FEED_CONCURRENCY: usize = 3;
/// Headlines returned for a whole watchlist.
const MAX_HEADLINES: usize = 60;
/// Per stock in a watchlist-wide view, so one ambiguous name (e.g. "BSE",
/// which also matches Sensex market news) cannot crowd out the rest.
const MAX_PER_SYMBOL: usize = 6;

#[derive(Debug, Default, Deserialize)]
pub struct HeadlinesQuery {
    pub market: Option<String>,
    pub symbol: Option<String>,
}

pub async fn headlines(
    State(state): State<AppState>,
    query: Result<Query<HeadlinesQuery>, QueryRejection>,
) -> Result<Json<HeadlinesResponse>, ApiError> {
    let Query(q) = query.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let market = params::parse_market(q.market.as_deref())?;
    let symbols = match q.symbol.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => vec![params::normalize_symbol(s)?],
        _ => state.config.watchlist_for(market),
    };

    // Company names make far better search queries than tickers. Quotes are
    // usually cached; if they fail, fall back to the symbol.
    let keys: Vec<String> = symbols.iter().map(|s| market.key(s)).collect();
    let names: HashMap<String, String> = match state.quotes(&keys).await {
        Ok((quotes, _)) => quotes
            .into_iter()
            .map(|q| {
                (
                    Market::split(&q.symbol).1.to_owned(),
                    clean_name(&q.name, market),
                )
            })
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "news: no quotes for company names");
            HashMap::new()
        }
    };

    let jobs: Vec<_> = symbols
        .into_iter()
        .map(|symbol| {
            let state = state.clone();
            let name = names
                .get(&symbol)
                .cloned()
                .unwrap_or_else(|| symbol.clone());
            async move {
                let found = state.google.search(&query_for(&name, market), market).await;
                (symbol, name, found)
            }
        })
        .collect();
    let results: Vec<_> = stream::iter(jobs)
        .buffer_unordered(FEED_CONCURRENCY)
        .collect()
        .await;

    let mut items: Vec<Headline> = Vec::new();
    let mut failures = 0usize;
    let total = results.len();
    for (symbol, name, found) in results {
        match found {
            Ok(feed) => items.extend(feed.iter().map(|item| Headline {
                symbol: symbol.clone(),
                name: name.clone(),
                item: item.clone(),
            })),
            Err(e) => {
                failures += 1;
                tracing::warn!(%symbol, error = %e, "news feed failed");
            }
        }
    }
    if total > 0 && failures == total {
        return Err(ApiError::Upstream(
            "Google News could not be reached".to_owned(),
        ));
    }

    // The same story often matches several stocks; keep its first match.
    let mut seen = HashSet::new();
    let mut per_symbol: HashMap<String, usize> = HashMap::new();
    let single = total == 1;
    items.sort_by(|a, b| b.item.published_at.cmp(&a.item.published_at));
    items.retain(|h| {
        if !seen.insert(h.item.title.to_lowercase()) {
            return false;
        }
        let n = per_symbol.entry(h.symbol.clone()).or_default();
        *n += 1;
        single || *n <= MAX_PER_SYMBOL
    });
    items.truncate(MAX_HEADLINES);

    Ok(Json(HeadlinesResponse {
        source: "google_news",
        market: market.as_str(),
        as_of: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        items,
    }))
}

/// MCX contract names ("GOLD 04 Dec Fut") search better as the bare
/// commodity ("Gold"); company names are used as they are.
fn clean_name(name: &str, market: Market) -> String {
    match market {
        Market::Mcx => {
            let first = name.split_whitespace().next().unwrap_or(name);
            let mut chars = first.chars();
            match chars.next() {
                Some(c) => c
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect(),
                None => name.to_owned(),
            }
        }
        _ => name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_mcx_contract_names() {
        assert_eq!(clean_name("GOLD 04 Dec Fut", Market::Mcx), "Gold");
        assert_eq!(clean_name("Gold", Market::Mcx), "Gold");
        assert_eq!(clean_name("ASK Automotive", Market::In), "ASK Automotive");
    }
}
