//! Wikipedia "List of highest domestic net collection of Hindi films":
//! table of original Hindi films (2009+), table of dubbed films, and the by-year table (1940+).

use crate::fetch::Fetcher;
use crate::model::RawRecord;
use crate::sources::text_of;
use crate::util::{parse_rupees_to_crore, year_from_date};
use anyhow::Result;
use scraper::{ElementRef, Html, Selector};

pub const SOURCE_ORIGINAL: &str = "wiki_domnet_original";
pub const SOURCE_DUBBED: &str = "wiki_domnet_dubbed";
pub const SOURCE_BY_YEAR: &str = "wiki_domnet_by_year";
pub const URL: &str = "https://en.wikipedia.org/wiki/List_of_highest_domestic_net_collection_of_Hindi_films";

pub fn fetch(f: &mut Fetcher) -> Result<String> {
    f.get("wiki_domestic_net_hindi", "html", URL)
}

fn headers(table: ElementRef) -> Vec<String> {
    let th = Selector::parse("th").unwrap();
    table.select(&th).map(|t| text_of(t).to_lowercase()).collect()
}

fn col_index(hdrs: &[String], needle: &str) -> Option<usize> {
    hdrs.iter().position(|h| h.contains(needle))
}

pub fn parse(html: &str) -> Result<Vec<RawRecord>> {
    let doc = Html::parse_document(html);
    let table_sel = Selector::parse("table.wikitable").unwrap();
    let tr_sel = Selector::parse("tr").unwrap();
    let td_sel = Selector::parse("td").unwrap();
    let mut out = Vec::new();
    for table in doc.select(&table_sel) {
        let hdrs = headers(table);
        let film_i = match col_index(&hdrs, "film") {
            Some(i) => i,
            None => continue,
        };
        let net_i = match col_index(&hdrs, "domestic net") {
            Some(i) => i,
            None => continue,
        };
        let year_i = col_index(&hdrs, "year");
        let date_i = col_index(&hdrs, "release date");
        let lang_i = col_index(&hdrs, "language");
        let source = if year_i.is_some() {
            SOURCE_BY_YEAR
        } else if lang_i.is_some() {
            SOURCE_DUBBED
        } else {
            SOURCE_ORIGINAL
        };
        let mut rank = 0u32;
        for row in table.select(&tr_sel) {
            let tds: Vec<_> = row.select(&td_sel).collect();
            if tds.len() <= film_i.max(net_i) {
                continue;
            }
            let title = text_of(tds[film_i]);
            if title.is_empty() {
                continue;
            }
            let nett = match parse_rupees_to_crore(&text_of(tds[net_i])) {
                Some(v) => v,
                None => continue,
            };
            let release_date = date_i.and_then(|i| tds.get(i)).map(|t| text_of(*t));
            let year = year_i
                .and_then(|i| tds.get(i))
                .and_then(|t| year_from_date(&text_of(*t)))
                .or_else(|| release_date.as_deref().and_then(year_from_date));
            let original_language = lang_i.and_then(|i| tds.get(i)).map(|t| text_of(*t));
            rank += 1;
            out.push(RawRecord {
                source: source.to_string(),
                source_url: URL.to_string(),
                rank_in_source: Some(rank),
                title,
                year,
                release_date,
                nett_cr: Some(nett),
                footfalls_cr: None,
                adjusted_nett_cr: None,
                verdict: None,
                is_dubbed_hint: source == SOURCE_DUBBED,
                original_language,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn three_table_kinds() {
        let html = r#"
        <table class="wikitable"><tr><th>Film</th><th>Release date</th><th>Studio</th><th>Domestic net</th><th>Ref</th></tr>
        <tr><td><i>Jawan</i></td><td>7 September 2023</td><td>RCE</td><td>₹640.25 crore</td><td>[1]</td></tr></table>
        <table class="wikitable"><tr><th>Film</th><th>Release date</th><th>Original language</th><th>Studio</th><th>Domestic net</th><th>Ref</th></tr>
        <tr><td>Pushpa 2: The Rule</td><td>5 December 2024</td><td>Telugu</td><td>MMM</td><td>₹830 crore</td><td></td></tr></table>
        <table class="wikitable"><tr><th>Year</th><th>Film</th><th>Studio</th><th>Domestic net</th><th>Ref</th></tr>
        <tr><td>1940</td><td>Zindagi</td><td>NT</td><td>₹55 lakh</td><td></td></tr></table>"#;
        let rows = parse(html).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].source, SOURCE_ORIGINAL);
        assert_eq!(rows[0].year, Some(2023));
        assert_eq!(rows[1].source, SOURCE_DUBBED);
        assert!(rows[1].is_dubbed_hint);
        assert_eq!(rows[1].original_language.as_deref(), Some("Telugu"));
        assert_eq!(rows[2].source, SOURCE_BY_YEAR);
        assert_eq!(rows[2].year, Some(1940));
        assert_eq!(rows[2].nett_cr, Some(0.55));
    }
}
