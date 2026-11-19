//! Catalysts for NSE stocks from NSE's public feeds: results dates,
//! corporate actions, tagged announcements, bulk and block deals, the
//! surveillance lists and the F&O ban list.
//!
//! Every source is cached under `<dir>/events/<name>.json` with its own
//! refresh interval. A source that fails keeps serving its last saved copy
//! and reports the problem in [`EventData::errors`]; it never blocks the
//! others. Announcements are fetched incrementally and only the categories
//! that matter for a swing trade are kept (NSE tags each filing itself).
//!
//! These endpoints sit on `www.nseindia.com/api`, which is known to turn
//! automated clients away at times, so everything here is best effort.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as Days, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::nse::ID;
use crate::{Error, Result};

const API: &str = "https://www.nseindia.com/api";
const BAN_URL: &str = "https://nsearchives.nseindia.com/content/fo/fo_secban.csv";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
const REQUEST_GAP: Duration = Duration::from_millis(300);
/// IST is UTC+05:30.
const IST_OFFSET_MINS: i64 = 330;

/// Refresh intervals, in seconds.
const TTL_MEETINGS: i64 = 6 * 3_600;
const TTL_ACTIONS: i64 = 6 * 3_600;
const TTL_ANNOUNCEMENTS: i64 = 3_600;
const TTL_DEALS: i64 = 3 * 3_600;
const TTL_SURVEILLANCE: i64 = 12 * 3_600;
const TTL_BAN: i64 = 3 * 3_600;

/// Days of announcements and deals kept.
const KEEP_DAYS: i64 = 10;
/// Board meetings and corporate actions are read this far ahead and back.
const AHEAD_DAYS: i64 = 30;
const BACK_DAYS: i64 = 15;
/// Longest filing text kept.
const MAX_TEXT: usize = 260;

/// What kind of event a catalyst is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CatalystKind {
    /// Board meeting to approve financial results.
    Results,
    Dividend,
    Bonus,
    Split,
    Buyback,
    Demerger,
    Rights,
    Fundraise,
    OrderWin,
    Expansion,
    /// Acquisition, merger or a material agreement.
    Deal,
    CreditRating,
    /// Resignation, cessation, auditor change.
    Management,
    /// Regulatory action, litigation, insolvency.
    Legal,
    /// The exchange asked about a price or volume move, or a news report.
    ExchangeQuery,
    /// Takeover-regulation disclosure (stake change, pledge).
    StakeChange,
    BulkDeal,
    BlockDeal,
    /// On the ASM or GSM surveillance list.
    Surveillance,
    FnoBan,
}

impl CatalystKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CatalystKind::Results => "results",
            CatalystKind::Dividend => "dividend",
            CatalystKind::Bonus => "bonus",
            CatalystKind::Split => "split",
            CatalystKind::Buyback => "buyback",
            CatalystKind::Demerger => "demerger",
            CatalystKind::Rights => "rights",
            CatalystKind::Fundraise => "fundraise",
            CatalystKind::OrderWin => "order_win",
            CatalystKind::Expansion => "expansion",
            CatalystKind::Deal => "deal",
            CatalystKind::CreditRating => "credit_rating",
            CatalystKind::Management => "management",
            CatalystKind::Legal => "legal",
            CatalystKind::ExchangeQuery => "exchange_query",
            CatalystKind::StakeChange => "stake_change",
            CatalystKind::BulkDeal => "bulk_deal",
            CatalystKind::BlockDeal => "block_deal",
            CatalystKind::Surveillance => "surveillance",
            CatalystKind::FnoBan => "fno_ban",
        }
    }
}

/// How an event usually bears on a long swing trade. A rule of thumb for
/// colouring badges, not a prediction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Positive,
    Negative,
    Neutral,
    /// A scheduled event or restriction that adds risk to a new position.
    Risk,
}

impl Tone {
    pub fn as_str(&self) -> &'static str {
        match self {
            Tone::Positive => "positive",
            Tone::Negative => "negative",
            Tone::Neutral => "neutral",
            Tone::Risk => "risk",
        }
    }
}

/// One dated event for one stock.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalyst {
    pub kind: CatalystKind,
    pub tone: Tone,
    pub date: NaiveDate,
    pub title: String,
    pub detail: Option<String>,
    /// Link to the filing, when there is one.
    pub url: Option<String>,
}

/// Everything known about catalysts right now.
#[derive(Debug, Clone, Default)]
pub struct EventData {
    /// Symbol → events, newest (or furthest ahead) first.
    pub catalysts: HashMap<String, Vec<Catalyst>>,
    /// Symbol → dates of board meetings on financial results, oldest first.
    pub results: HashMap<String, Vec<NaiveDate>>,
    /// Symbol → surveillance stage, e.g. `LTASM Stage I`.
    pub surveillance: HashMap<String, String>,
    pub fno_ban: HashSet<String>,
    /// Sources that could not be refreshed this time.
    pub errors: Vec<String>,
    /// Unix seconds of the last refresh; 0 before the first.
    pub refreshed_at: i64,
}

impl EventData {
    /// Events of `symbol` dated from `from` to `to` inclusive, in stored order.
    pub fn window(&self, symbol: &str, from: NaiveDate, to: NaiveDate) -> Vec<Catalyst> {
        self.catalysts
            .get(symbol)
            .map(|v| {
                v.iter()
                    .filter(|c| c.date >= from && c.date <= to)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Cached {
    /// Unix seconds.
    fetched_at: i64,
    body: Value,
}

/// Fetches, caches and serves NSE catalysts.
pub struct NseEvents {
    dir: PathBuf,
    http: reqwest::Client,
    data: RwLock<Arc<EventData>>,
}

impl NseEvents {
    /// Caches under `<dir>/events`.
    pub fn new(dir: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self {
            dir: dir.into().join("events"),
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(45))
                .user_agent(USER_AGENT)
                .build()?,
            data: RwLock::new(Arc::new(EventData::default())),
        })
    }

    /// The result of the last refresh.
    pub async fn data(&self) -> Arc<EventData> {
        self.data.read().await.clone()
    }

    /// Refreshes every source that is due and rebuilds the catalysts.
    pub async fn refresh(&self) -> Arc<EventData> {
        let now = Utc::now();
        let today = (now + Days::minutes(IST_OFFSET_MINS)).date_naive();
        let mut errors = Vec::new();
        if let Err(e) = tokio::fs::create_dir_all(&self.dir).await {
            errors.push(format!("cache directory: {e}"));
        }
        let dmy = |d: NaiveDate| d.format("%d-%m-%Y").to_string();
        let (back, ahead) = (
            today - Days::days(BACK_DAYS),
            today + Days::days(AHEAD_DAYS),
        );
        let range = format!("from_date={}&to_date={}", dmy(back), dmy(ahead));
        let recent = format!(
            "from={}&to={}",
            dmy(today - Days::days(KEEP_DAYS)),
            dmy(today)
        );

        let meetings = self
            .source(
                "board-meetings",
                TTL_MEETINGS,
                &format!("{API}/corporate-board-meetings?index=equities&{range}"),
                &mut errors,
            )
            .await;
        let actions = self
            .source(
                "corporate-actions",
                TTL_ACTIONS,
                &format!("{API}/corporates-corporateActions?index=equities&{range}"),
                &mut errors,
            )
            .await;
        let bulk = self
            .source(
                "bulk-deals",
                TTL_DEALS,
                &format!(
                    "{API}/historicalOR/bulk-block-short-deals?optionType=bulk_deals&{recent}"
                ),
                &mut errors,
            )
            .await;
        let block = self
            .source(
                "block-deals",
                TTL_DEALS,
                &format!(
                    "{API}/historicalOR/bulk-block-short-deals?optionType=block_deals&{recent}"
                ),
                &mut errors,
            )
            .await;
        let asm = self
            .source(
                "asm",
                TTL_SURVEILLANCE,
                &format!("{API}/reportASM"),
                &mut errors,
            )
            .await;
        let gsm = self
            .source(
                "gsm",
                TTL_SURVEILLANCE,
                &format!("{API}/reportGSM"),
                &mut errors,
            )
            .await;
        let ban = self.ban_list(&mut errors).await;
        let announcements = self.announcements(today, &mut errors).await;

        let mut data = build(&Sources {
            today,
            meetings: &meetings,
            actions: &actions,
            announcements: &announcements,
            bulk: &bulk,
            block: &block,
            asm: &asm,
            gsm: &gsm,
            ban: &ban,
        });
        data.errors = errors;
        data.refreshed_at = now.timestamp();
        let data = Arc::new(data);
        *self.data.write().await = data.clone();
        data
    }

    /// A JSON source: the saved copy while fresh, otherwise a new download.
    /// On failure the stale copy (or `null`) is returned and the error noted.
    async fn source(&self, name: &str, ttl: i64, url: &str, errors: &mut Vec<String>) -> Value {
        let path = self.dir.join(format!("{name}.json"));
        let cached = read_cached(&path).await;
        let now = Utc::now().timestamp();
        if let Some(c) = cached.as_ref().filter(|c| now - c.fetched_at < ttl) {
            return c.body.clone();
        }
        match self.get_json(url).await {
            Ok(body) => {
                save_cached(&path, now, &body).await;
                body
            }
            Err(e) => {
                errors.push(format!("{name}: {e}"));
                cached.map(|c| c.body).unwrap_or(Value::Null)
            }
        }
    }

    /// Symbols in the F&O ban, as `{"date": "...", "symbols": [...]}`.
    async fn ban_list(&self, errors: &mut Vec<String>) -> Value {
        let path = self.dir.join("fno-ban.json");
        let cached = read_cached(&path).await;
        let now = Utc::now().timestamp();
        if let Some(c) = cached.as_ref().filter(|c| now - c.fetched_at < TTL_BAN) {
            return c.body.clone();
        }
        match self.get_text(BAN_URL).await {
            Ok(text) => {
                let body = parse_ban(&text);
                save_cached(&path, now, &body).await;
                body
            }
            Err(e) => {
                errors.push(format!("fno-ban: {e}"));
                cached.map(|c| c.body).unwrap_or(Value::Null)
            }
        }
    }

    /// Kept announcements as an array of reduced rows. Only the days since
    /// the last fetch are downloaded and merged in.
    async fn announcements(&self, today: NaiveDate, errors: &mut Vec<String>) -> Value {
        let path = self.dir.join("announcements.json");
        let cached = read_cached(&path).await;
        let now = Utc::now().timestamp();
        if let Some(c) = cached
            .as_ref()
            .filter(|c| now - c.fetched_at < TTL_ANNOUNCEMENTS)
        {
            return c.body.clone();
        }
        let oldest = today - Days::days(KEEP_DAYS);
        // Overlap one day so nothing filed around midnight is missed.
        let from = cached
            .as_ref()
            .and_then(|c| chrono::DateTime::from_timestamp(c.fetched_at, 0))
            .map(|t| (t + Days::minutes(IST_OFFSET_MINS)).date_naive() - Days::days(1))
            .map_or(oldest, |d| d.max(oldest));
        let url = format!(
            "{API}/corporate-announcements?index=equities&from_date={}&to_date={}",
            from.format("%d-%m-%Y"),
            today.format("%d-%m-%Y")
        );
        let mut rows: Vec<Value> = cached
            .as_ref()
            .and_then(|c| c.body.as_array().cloned())
            .unwrap_or_default();
        match self.get_json(&url).await {
            Ok(fresh) => {
                let known: HashSet<String> = rows
                    .iter()
                    .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_owned))
                    .collect();
                rows.extend(reduce_announcements(&fresh).into_iter().filter(|r| {
                    r.get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| !known.contains(id))
                }));
                let cutoff = oldest.format("%Y-%m-%d").to_string();
                rows.retain(|r| {
                    r.get("date")
                        .and_then(Value::as_str)
                        .is_some_and(|d| d >= cutoff.as_str())
                });
                let body = Value::Array(rows);
                save_cached(&path, now, &body).await;
                body
            }
            Err(e) => {
                errors.push(format!("announcements: {e}"));
                Value::Array(rows)
            }
        }
    }

    async fn get_text(&self, url: &str) -> Result<String> {
        tokio::time::sleep(REQUEST_GAP).await;
        let resp = self
            .http
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json, text/plain, */*")
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(Error::Upstream {
                provider: ID,
                message: format!("{url} returned {status}"),
            });
        }
        Ok(resp.text().await?)
    }

    async fn get_json(&self, url: &str) -> Result<Value> {
        let text = self.get_text(url).await?;
        serde_json::from_str(&text).map_err(|e| Error::Decode {
            provider: ID,
            message: format!("{url}: {e}"),
        })
    }
}

async fn read_cached(path: &Path) -> Option<Cached> {
    let bytes = tokio::fs::read(path).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

async fn save_cached(path: &Path, fetched_at: i64, body: &Value) {
    let cached = json!({ "fetched_at": fetched_at, "body": body });
    let tmp = path.with_extension("tmp");
    let saved = match serde_json::to_vec(&cached) {
        Ok(bytes) => match tokio::fs::write(&tmp, bytes).await {
            Ok(()) => tokio::fs::rename(&tmp, path).await,
            Err(e) => Err(e),
        },
        Err(e) => Err(std::io::Error::other(e)),
    };
    if let Err(e) = saved {
        tracing::warn!(path = %path.display(), error = %e, "could not save NSE events cache");
    }
}

/// Rows of a response that is either an array or `{"data": [...]}`.
fn rows(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array()
        .or_else(|| v.get("data").and_then(Value::as_array))
        .into_iter()
        .flatten()
}

fn text<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "-")
}

/// `05-Dec-2026` or `05-DEC-2026`.
fn parse_dmy(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%d-%b-%Y").ok()
}

fn clip(s: &str) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > MAX_TEXT {
        format!("{}…", s.chars().take(MAX_TEXT).collect::<String>())
    } else {
        s
    }
}

/// `{"date": "20-NOV-2026", "symbols": ["KAYNES"]}` from the ban CSV:
/// a title line with the trade date, then `1,SYMBOL` rows.
fn parse_ban(text: &str) -> Value {
    let mut lines = text.lines();
    let date = lines
        .next()
        .and_then(|l| l.split("Date").nth(1))
        .map(|d| d.trim().trim_end_matches(':').trim().to_owned())
        .unwrap_or_default();
    let symbols: Vec<String> = lines
        .filter_map(|l| l.split(',').nth(1))
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect();
    json!({ "date": date, "symbols": symbols })
}

/// What an NSE announcement category means for the screen; `None` for the
/// routine ones (newspaper copies, investor meets, general updates).
fn announcement_kind(category: &str) -> Option<(CatalystKind, Tone, &'static str)> {
    use CatalystKind as K;
    let c = category.to_ascii_lowercase();
    let has = |needle: &str| c.contains(needle);
    Some(if has("bagging") || has("orders/contracts") {
        (K::OrderWin, Tone::Positive, "Order win")
    } else if has("capacity addition") || has("commencement of commercial") {
        (K::Expansion, Tone::Positive, "Capacity / new operations")
    } else if has("buyback") {
        (K::Buyback, Tone::Positive, "Buyback")
    } else if has("acquisition") || has("amalgamation") || has("merger") || c == "agreements" {
        (K::Deal, Tone::Neutral, "Acquisition / merger / agreement")
    } else if has("credit rating") {
        (K::CreditRating, Tone::Neutral, "Credit rating")
    } else if has("resignation") || has("cessation") || has("change in auditor") {
        (
            K::Management,
            Tone::Negative,
            "Resignation / auditor change",
        )
    } else if has("orders passed")
        || has("litigation")
        || has("insolvency")
        || has("disclosure of material issue")
        || has("delayed/non-submission")
    {
        (K::Legal, Tone::Negative, "Regulatory / legal")
    } else if has("spurt in volume") || has("price movement") || has("news verification") {
        (K::ExchangeQuery, Tone::Neutral, "Exchange query")
    } else if has("takeover regulations") {
        (K::StakeChange, Tone::Neutral, "Stake / pledge disclosure")
    } else if has("allotment of securities")
        || has("fund raising")
        || has("qualified institutional")
    {
        (K::Fundraise, Tone::Neutral, "Fund raising")
    } else {
        return None;
    })
}

/// Keeps the announcements worth showing, as small rows
/// `{id, symbol, date, category, text, url}`.
fn reduce_announcements(fresh: &Value) -> Vec<Value> {
    rows(fresh)
        .filter_map(|r| {
            let category = text(r, "desc")?;
            announcement_kind(category)?;
            let date = text(r, "sort_date")?.get(..10)?;
            NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            Some(json!({
                "id": text(r, "seq_id")?,
                "symbol": text(r, "symbol")?,
                "date": date,
                "category": category,
                "text": text(r, "attchmntText").map(clip),
                "url": text(r, "attchmntFile").filter(|u| u.starts_with("https://")),
            }))
        })
        .collect()
}

/// Kind of a corporate-action subject line.
fn action_kind(subject: &str) -> Option<(CatalystKind, Tone)> {
    use CatalystKind as K;
    let s = subject.to_ascii_lowercase();
    Some(if s.contains("dividend") {
        (K::Dividend, Tone::Neutral)
    } else if s.contains("bonus") {
        (K::Bonus, Tone::Neutral)
    } else if s.contains("split") || s.contains("sub-division") || s.contains("consolidat") {
        (K::Split, Tone::Neutral)
    } else if s.contains("buyback") || s.contains("buy back") {
        (K::Buyback, Tone::Positive)
    } else if s.contains("demerg") {
        (K::Demerger, Tone::Neutral)
    } else if s.contains("rights") {
        (K::Rights, Tone::Neutral)
    } else {
        return None;
    })
}

/// Raw responses of every source, borrowed for [`build`].
struct Sources<'a> {
    today: NaiveDate,
    meetings: &'a Value,
    actions: &'a Value,
    announcements: &'a Value,
    bulk: &'a Value,
    block: &'a Value,
    asm: &'a Value,
    gsm: &'a Value,
    ban: &'a Value,
}

fn build(src: &Sources<'_>) -> EventData {
    let mut data = EventData::default();
    let mut add = |symbol: &str, c: Catalyst| {
        data.catalysts.entry(symbol.to_owned()).or_default().push(c);
    };
    let mut results: HashMap<String, Vec<NaiveDate>> = HashMap::new();

    // Board meetings: results dates, plus buyback / fund-raising / dividend meetings.
    let mut seen: HashSet<(String, NaiveDate, CatalystKind)> = HashSet::new();
    for r in rows(src.meetings) {
        let (Some(symbol), Some(date)) =
            (text(r, "bm_symbol"), text(r, "bm_date").and_then(parse_dmy))
        else {
            continue;
        };
        let purpose = text(r, "bm_purpose").unwrap_or_default();
        let desc = text(r, "bm_desc").unwrap_or_default();
        let both = format!("{purpose} {desc}").to_ascii_lowercase();
        let (kind, tone, title) = if both.contains("financial result") {
            (
                CatalystKind::Results,
                if date > src.today {
                    Tone::Risk
                } else {
                    Tone::Neutral
                },
                "Results",
            )
        } else if both.contains("buyback") || both.contains("buy back") {
            (
                CatalystKind::Buyback,
                Tone::Positive,
                "Board meets on buyback",
            )
        } else if both.contains("fund rais") {
            (
                CatalystKind::Fundraise,
                Tone::Neutral,
                "Board meets on fund raising",
            )
        } else if purpose.to_ascii_lowercase().contains("dividend") {
            (
                CatalystKind::Dividend,
                Tone::Neutral,
                "Board meets on dividend",
            )
        } else {
            continue;
        };
        if !seen.insert((symbol.to_owned(), date, kind)) {
            continue;
        }
        if kind == CatalystKind::Results {
            results.entry(symbol.to_owned()).or_default().push(date);
        }
        add(
            symbol,
            Catalyst {
                kind,
                tone,
                date,
                title: title.to_owned(),
                detail: (!desc.is_empty()).then(|| clip(desc)),
                url: text(r, "attachment")
                    .filter(|u| u.starts_with("https://"))
                    .map(str::to_owned),
            },
        );
    }

    for r in rows(src.actions) {
        let series = text(r, "series").unwrap_or("EQ");
        let (Some(symbol), Some(date), Some(subject)) = (
            text(r, "symbol"),
            text(r, "exDate").and_then(parse_dmy),
            text(r, "subject"),
        ) else {
            continue;
        };
        let Some((kind, tone)) = action_kind(subject).filter(|_| series == "EQ" || series == "BE")
        else {
            continue;
        };
        add(
            symbol,
            Catalyst {
                kind,
                tone,
                date,
                title: format!("Ex-date: {}", clip(subject)),
                detail: None,
                url: None,
            },
        );
    }

    for r in rows(src.announcements) {
        let (Some(symbol), Some(date), Some(category)) = (
            text(r, "symbol"),
            text(r, "date").and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()),
            text(r, "category"),
        ) else {
            continue;
        };
        let Some((kind, tone, title)) = announcement_kind(category) else {
            continue;
        };
        add(
            symbol,
            Catalyst {
                kind,
                tone,
                date,
                title: title.to_owned(),
                detail: text(r, "text").map(str::to_owned),
                url: text(r, "url").map(str::to_owned),
            },
        );
    }

    for (deals, kind, label) in [
        (src.bulk, CatalystKind::BulkDeal, "Bulk deal"),
        (src.block, CatalystKind::BlockDeal, "Block deal"),
    ] {
        for r in rows(deals) {
            let (Some(symbol), Some(date)) = (
                text(r, "BD_SYMBOL"),
                text(r, "BD_DT_DATE").and_then(parse_dmy),
            ) else {
                continue;
            };
            let side = text(r, "BD_BUY_SELL").unwrap_or("").to_ascii_lowercase();
            let qty = r.get("BD_QTY_TRD").and_then(Value::as_f64).unwrap_or(0.0);
            let price = r.get("BD_TP_WATP").and_then(Value::as_f64).unwrap_or(0.0);
            add(
                symbol,
                Catalyst {
                    kind,
                    // Every deal has a buyer and a seller; the side is of this client only.
                    tone: Tone::Neutral,
                    date,
                    title: format!("{label}: {side} {} sh @ {price:.2}", group_thousands(qty)),
                    detail: text(r, "BD_CLIENT_NAME").map(clip),
                    url: None,
                },
            );
        }
    }

    // Surveillance lists: ASM is `{longterm: {data}, shortterm: {data}}`, GSM a plain list.
    let mut flag = |symbol: &str, stage: String| {
        data.surveillance.entry(symbol.to_owned()).or_insert(stage);
    };
    for (key, name) in [
        ("longterm", "Long-term ASM"),
        ("shortterm", "Short-term ASM"),
    ] {
        for r in src.asm.get(key).map(rows).into_iter().flatten() {
            if let Some(symbol) = text(r, "symbol") {
                let stage = text(r, "asmSurvIndicator").unwrap_or("listed");
                flag(symbol, format!("{name} {stage}"));
            }
        }
    }
    for r in rows(src.gsm) {
        if let Some(symbol) = text(r, "symbol") {
            flag(symbol, "GSM".to_owned());
        }
    }
    for (symbol, stage) in data.surveillance.clone() {
        data.catalysts.entry(symbol).or_default().push(Catalyst {
            kind: CatalystKind::Surveillance,
            tone: Tone::Risk,
            date: src.today,
            title: format!("On surveillance list: {stage}"),
            detail: Some("Extra margins and tighter price bands apply.".to_owned()),
            url: None,
        });
    }

    let ban_date = text(src.ban, "date")
        .and_then(parse_dmy)
        .unwrap_or(src.today);
    for symbol in src
        .ban
        .get("symbols")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        data.fno_ban.insert(symbol.to_owned());
        data.catalysts
            .entry(symbol.to_owned())
            .or_default()
            .push(Catalyst {
                kind: CatalystKind::FnoBan,
                tone: Tone::Risk,
                date: ban_date,
                title: "In F&O ban period".to_owned(),
                detail: Some(
                    "No new F&O positions; often follows heavy speculative interest.".to_owned(),
                ),
                url: None,
            });
    }

    for list in data.catalysts.values_mut() {
        list.sort_by_key(|c| std::cmp::Reverse(c.date));
    }
    for dates in results.values_mut() {
        dates.sort();
        dates.dedup();
    }
    data.results = results;
    data
}

/// `475000` → `4,75,000` (Indian grouping).
fn group_thousands(v: f64) -> String {
    let digits = format!("{:.0}", v.abs());
    let n = digits.len();
    if n <= 3 {
        return digits;
    }
    let (head, tail) = digits.split_at(n - 3);
    let mut out = String::new();
    for (i, ch) in head.chars().enumerate() {
        if i > 0 && (head.len() - i) % 2 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    format!("{out},{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn classifies_announcement_categories() {
        let kind = |c: &str| announcement_kind(c).map(|(k, t, _)| (k, t));
        assert_eq!(
            kind("Bagging/Receiving of orders/contracts"),
            Some((CatalystKind::OrderWin, Tone::Positive))
        );
        assert_eq!(
            kind("Resignation of Director/KMP/SMP"),
            Some((CatalystKind::Management, Tone::Negative))
        );
        assert_eq!(
            kind("Change in Auditors"),
            Some((CatalystKind::Management, Tone::Negative))
        );
        assert_eq!(
            kind("Credit Rating- Revision"),
            Some((CatalystKind::CreditRating, Tone::Neutral))
        );
        assert_eq!(
            kind("Action(s) taken or orders passed"),
            Some((CatalystKind::Legal, Tone::Negative))
        );
        assert_eq!(
            kind("Capacity addition"),
            Some((CatalystKind::Expansion, Tone::Positive))
        );
        assert_eq!(
            kind("Spurt in Volume"),
            Some((CatalystKind::ExchangeQuery, Tone::Neutral))
        );
        for routine in [
            "Copy of Newspaper Publication",
            "Analysts/Institutional Investor Meet/Con. Call Updates",
            "General Updates",
            "Outcome of Board Meeting",
            "Press Release",
        ] {
            assert_eq!(kind(routine), None, "{routine}");
        }
    }

    #[test]
    fn reduces_and_parses_small_things() {
        let fresh = json!([
            {"seq_id": "1", "symbol": "HILINFRA", "desc": "Bagging/Receiving of orders/contracts",
             "sort_date": "2026-11-18 10:15:00", "attchmntText": "Highway  Infrastructure has informed the Exchange about an order",
             "attchmntFile": "https://nsearchives.nseindia.in/corporate/x.pdf"},
            {"seq_id": "2", "symbol": "PACE", "desc": "General Updates", "sort_date": "2026-11-19 21:24:48"},
            {"seq_id": "3", "symbol": "BAD", "desc": "Acquisition", "sort_date": "soon"}
        ]);
        let kept = reduce_announcements(&fresh);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0]["date"], "2026-11-18");
        assert_eq!(
            kept[0]["text"],
            "Highway Infrastructure has informed the Exchange about an order"
        );

        assert_eq!(parse_dmy("05-NOV-2026"), Some(day(2026, 11, 5)));
        assert_eq!(parse_dmy("05-Dec-2026"), Some(day(2026, 12, 5)));
        assert_eq!(group_thousands(475_000.0), "4,75,000");
        assert_eq!(group_thousands(44_000_000.0), "4,40,00,000");
        assert_eq!(group_thousands(950.0), "950");
        let ban = parse_ban("Securities in Ban For Trade Date 20-NOV-2026:\n1,KAYNES\n2, sail\n");
        assert_eq!(
            ban,
            json!({"date": "20-NOV-2026", "symbols": ["KAYNES", "SAIL"]})
        );
        assert_eq!(clip(&"x".repeat(300)).chars().count(), MAX_TEXT + 1);
    }

    #[test]
    fn builds_catalysts_from_every_source() {
        let today = day(2026, 11, 19);
        let meetings = json!([
            {"bm_symbol": "ACE", "bm_date": "25-Nov-2026", "bm_purpose": "Financial Results",
             "bm_desc": "To consider and approve the financial results for the period ended September 30, 2026"},
            {"bm_symbol": "ACE", "bm_date": "25-Nov-2026", "bm_purpose": "Board Meeting Intimation",
             "bm_desc": "Financial results (duplicate intimation)"},
            {"bm_symbol": "ACE", "bm_date": "10-Nov-2026", "bm_purpose": "Board Meeting Intimation",
             "bm_desc": "To approve financial results"},
            {"bm_symbol": "IEX", "bm_date": "24-Nov-2026", "bm_purpose": "Buyback", "bm_desc": "To consider buyback"},
            {"bm_symbol": "CAMS", "bm_date": "24-Nov-2026", "bm_purpose": "Other business matters", "bm_desc": "-"}
        ]);
        let actions = json!([
            {"symbol": "NATCOPHARM", "series": "EQ", "exDate": "19-Nov-2026", "subject": "Interim Dividend - Rs 1.25 Per Share"},
            {"symbol": "X", "series": "N1", "exDate": "19-Nov-2026", "subject": "Interest Payment"}
        ]);
        let announcements = json!([
            {"id": "1", "symbol": "ACE", "date": "2026-11-18", "category": "Bagging/Receiving of orders/contracts",
             "text": "Order for 74 loaders", "url": "https://nsearchives.nseindia.in/corporate/a.pdf"}
        ]);
        let bulk = json!({"data": [
            {"BD_DT_DATE": "17-NOV-2026", "BD_SYMBOL": "ACE", "BD_CLIENT_NAME": "SOME FUND", "BD_BUY_SELL": "BUY",
             "BD_QTY_TRD": 475000, "BD_TP_WATP": 1158.03}
        ]});
        let asm = json!({"longterm": {"data": [{"symbol": "AAATECH", "asmSurvIndicator": "Stage I"}]},
                         "shortterm": {"data": [{"symbol": "AGIIL", "asmSurvIndicator": "Stage II"}]}});
        let gsm = json!([{"symbol": "AGSTRA", "gsmStage": "LXII"}]);
        let ban = json!({"date": "20-NOV-2026", "symbols": ["KAYNES"]});
        let data = build(&Sources {
            today,
            meetings: &meetings,
            actions: &actions,
            announcements: &announcements,
            bulk: &bulk,
            block: &Value::Null,
            asm: &asm,
            gsm: &gsm,
            ban: &ban,
        });

        assert_eq!(
            data.results["ACE"],
            vec![day(2026, 11, 10), day(2026, 11, 25)]
        );
        let ace = &data.catalysts["ACE"];
        let kinds: Vec<(CatalystKind, NaiveDate)> = ace.iter().map(|c| (c.kind, c.date)).collect();
        assert_eq!(
            kinds,
            vec![
                (CatalystKind::Results, day(2026, 11, 25)),
                (CatalystKind::OrderWin, day(2026, 11, 18)),
                (CatalystKind::BulkDeal, day(2026, 11, 17)),
                (CatalystKind::Results, day(2026, 11, 10)),
            ],
            "newest first, duplicate intimation dropped"
        );
        assert_eq!(
            ace[0].tone,
            Tone::Risk,
            "upcoming results are an event risk"
        );
        assert_eq!(ace[3].tone, Tone::Neutral);
        assert_eq!(ace[2].title, "Bulk deal: buy 4,75,000 sh @ 1158.03");
        assert_eq!(ace[2].detail.as_deref(), Some("SOME FUND"));
        assert_eq!(data.catalysts["IEX"][0].kind, CatalystKind::Buyback);
        assert!(!data.catalysts.contains_key("CAMS"));
        assert!(!data.catalysts.contains_key("X"));
        assert_eq!(
            data.catalysts["NATCOPHARM"][0].title,
            "Ex-date: Interim Dividend - Rs 1.25 Per Share"
        );

        assert_eq!(data.surveillance["AAATECH"], "Long-term ASM Stage I");
        assert_eq!(data.surveillance["AGIIL"], "Short-term ASM Stage II");
        assert_eq!(data.surveillance["AGSTRA"], "GSM");
        assert_eq!(data.catalysts["AGSTRA"][0].kind, CatalystKind::Surveillance);
        assert!(data.fno_ban.contains("KAYNES"));
        assert_eq!(data.catalysts["KAYNES"][0].date, day(2026, 11, 20));

        let window = data.window("ACE", day(2026, 11, 12), day(2026, 11, 30));
        assert_eq!(window.len(), 3);
        assert!(data.window("NOPE", today, today).is_empty());
    }
}
