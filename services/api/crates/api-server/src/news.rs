//! Google News RSS headlines for the Stock news page.
//!
//! The feed is fetched server-side (the browser never calls Google), cached
//! for [`NEWS_TTL`], and reduced to title, publisher, link and date. Queries
//! contain only company names, never anything about the owner.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, SecondsFormat};
use market_data::{Market, NewsItem};
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::Url;

use crate::cache::TtlCache;
use crate::error::ApiError;

/// How long a feed is reused.
pub const NEWS_TTL: Duration = Duration::from_secs(30 * 60);
const FEED_URL: &str = "https://news.google.com/rss/search";
/// Headlines kept per feed.
const MAX_ITEMS: usize = 20;

/// Fetches and caches Google News RSS search feeds.
pub struct GoogleNews {
    http: reqwest::Client,
    cache: TtlCache<String, Arc<Vec<NewsItem>>>,
}

impl Default for GoogleNews {
    fn default() -> Self {
        Self::new()
    }
}

impl GoogleNews {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("swing-trade/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "news HTTP client fell back to defaults");
                reqwest::Client::new()
            });
        Self {
            http,
            cache: TtlCache::new(NEWS_TTL, 500),
        }
    }

    /// Headlines for `query`, in the edition for `market`, newest first.
    pub async fn search(
        &self,
        query: &str,
        market: Market,
    ) -> Result<Arc<Vec<NewsItem>>, ApiError> {
        let url = feed_url(query, market)?;
        if let Some(hit) = self.cache.get(&url).await {
            return Ok(hit);
        }
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Upstream(format!("Google News: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(ApiError::Upstream(format!("Google News returned {status}")));
        }
        let xml = resp
            .text()
            .await
            .map_err(|e| ApiError::Upstream(format!("Google News: {e}")))?;
        let items = Arc::new(parse_rss(&xml));
        self.cache.insert(url, items.clone()).await;
        Ok(items)
    }
}

/// The RSS search URL; Indian edition except for US stocks.
fn feed_url(query: &str, market: Market) -> Result<String, ApiError> {
    let (hl, gl, ceid) = match market {
        Market::Us => ("en-US", "US", "US:en"),
        Market::In | Market::Mcx => ("en-IN", "IN", "IN:en"),
    };
    Url::parse_with_params(
        FEED_URL,
        &[("q", query), ("hl", hl), ("gl", gl), ("ceid", ceid)],
    )
    .map(String::from)
    .map_err(|e| ApiError::Internal(format!("news URL: {e}")))
}

/// The search query for a stock: its name plus a market hint.
pub fn query_for(name: &str, market: Market) -> String {
    match market {
        Market::In => format!("\"{name}\" share"),
        Market::Us => format!("\"{name}\" stock"),
        Market::Mcx => format!("{name} price MCX"),
    }
}

/// Items of an RSS 2.0 feed. Google titles end in `" - Publisher"`, which
/// is dropped when it repeats the `<source>`.
pub fn parse_rss(xml: &str) -> Vec<NewsItem> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut items = Vec::new();
    let mut in_item = false;
    let mut field: Option<&'static str> = None;
    let (mut title, mut link, mut date, mut source) =
        (String::new(), String::new(), String::new(), String::new());
    loop {
        let text = match reader.read_event() {
            Ok(Event::Start(e)) => {
                match e.name().as_ref() {
                    b"item" => {
                        in_item = true;
                        title.clear();
                        link.clear();
                        date.clear();
                        source.clear();
                    }
                    b"title" if in_item => field = Some("title"),
                    b"link" if in_item => field = Some("link"),
                    b"pubDate" if in_item => field = Some("date"),
                    b"source" if in_item => field = Some("source"),
                    _ => field = None,
                }
                continue;
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"item" && in_item {
                    in_item = false;
                    if let Some(item) = build_item(&title, &link, &date, &source) {
                        items.push(item);
                    }
                }
                field = None;
                continue;
            }
            Ok(Event::Text(t)) => t.unescape().map(|s| s.into_owned()).unwrap_or_default(),
            Ok(Event::CData(c)) => String::from_utf8_lossy(&c.into_inner()).into_owned(),
            Ok(Event::Eof) => break,
            Err(e) => {
                tracing::warn!(error = %e, "malformed news feed; keeping what parsed");
                break;
            }
            Ok(_) => continue,
        };
        match field {
            Some("title") => title.push_str(&text),
            Some("link") => link.push_str(&text),
            Some("date") => date.push_str(&text),
            Some("source") => source.push_str(&text),
            _ => {}
        }
    }
    items.sort_by(|a, b| b.published_at.cmp(&a.published_at));
    items.truncate(MAX_ITEMS);
    items
}

fn build_item(title: &str, link: &str, date: &str, source: &str) -> Option<NewsItem> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    let source = source.trim();
    let title = title
        .strip_suffix(&format!(" - {source}"))
        .filter(|_| !source.is_empty())
        .unwrap_or(title);
    let url = link.trim();
    Some(NewsItem {
        title: title.to_owned(),
        summary: None,
        sentiment: None,
        published_at: DateTime::parse_from_rfc2822(date.trim())
            .ok()
            .map(|d| d.to_utc().to_rfc3339_opts(SecondsFormat::Secs, true)),
        source: (!source.is_empty()).then(|| source.to_owned()),
        url: (url.starts_with("https://") || url.starts_with("http://")).then(|| url.to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>"Coforge" share - Google News</title>
<item><title>Coforge shares rise after Q2 results - Moneycontrol</title>
<link>https://news.google.com/rss/articles/abc?oc=5</link>
<pubDate>Mon, 09 Nov 2026 08:15:00 GMT</pubDate>
<description>&lt;a href="x"&gt;ignored&lt;/a&gt;</description>
<source url="https://www.moneycontrol.com">Moneycontrol</source></item>
<item><title><![CDATA[Coforge & peers: IT stocks in focus - ET]]></title>
<link>https://news.google.com/rss/articles/def</link>
<pubDate>Tue, 10 Nov 2026 04:00:00 GMT</pubDate>
<source url="https://economictimes.com">The Economic Times</source></item>
<item><title></title><link>javascript:alert(1)</link></item>
</channel></rss>"#;

    #[test]
    fn parses_google_news_rss() {
        let items = parse_rss(FEED);
        assert_eq!(items.len(), 2);
        // Newest first.
        assert_eq!(items[0].title, "Coforge & peers: IT stocks in focus - ET");
        assert_eq!(items[0].source.as_deref(), Some("The Economic Times"));
        assert_eq!(
            items[0].published_at.as_deref(),
            Some("2026-11-10T04:00:00Z")
        );
        assert_eq!(items[1].title, "Coforge shares rise after Q2 results");
        assert_eq!(
            items[1].url.as_deref(),
            Some("https://news.google.com/rss/articles/abc?oc=5")
        );
        assert!(parse_rss("not xml at all").is_empty());
    }

    #[test]
    fn builds_queries_and_urls() {
        assert_eq!(query_for("Coforge", Market::In), "\"Coforge\" share");
        assert_eq!(query_for("Gold", Market::Mcx), "Gold price MCX");
        let url = feed_url("\"Apple\" stock", Market::Us).unwrap();
        assert!(url.starts_with("https://news.google.com/rss/search?q=%22Apple%22+stock"));
        assert!(url.ends_with("&hl=en-US&gl=US&ceid=US%3Aen"));
    }

    #[test]
    fn rejects_non_http_links() {
        let item = build_item("t", "javascript:alert(1)", "", "").unwrap();
        assert_eq!(item.url, None);
    }
}
