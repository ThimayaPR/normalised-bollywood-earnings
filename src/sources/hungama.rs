//! Bollywood Hungama footfalls tables (all-time top 100 and per-year pages).
//! Footfalls are reported in millions; we convert to crore (1 crore = 10 million).

use crate::fetch::Fetcher;
use crate::model::RawRecord;
use crate::sources::text_of;
use crate::util::year_from_date;
use anyhow::Result;
use scraper::{Html, Selector};

pub const SOURCE: &str = "hungama_footfalls";
pub const FIRST_YEAR: u16 = 1994;
pub const LAST_YEAR: u16 = 2026;

pub fn url(year: Option<u16>) -> String {
    match year {
        Some(y) => format!("https://www.bollywoodhungama.com/box-office-collections/footfalls/{y}/"),
        None => "https://www.bollywoodhungama.com/box-office-collections/footfalls/".to_string(),
    }
}

pub fn fetch(f: &mut Fetcher, year: Option<u16>) -> Result<String> {
    let slug = match year {
        Some(y) => format!("hungama_footfalls_{y}"),
        None => "hungama_footfalls_alltime".to_string(),
    };
    f.get(&slug, "html", &url(year))
}

pub fn parse(html: &str, year: Option<u16>) -> Result<Vec<RawRecord>> {
    let doc = Html::parse_document(html);
    let table_sel = Selector::parse("table.bh-bo-table").unwrap();
    let row_sel = Selector::parse("tbody tr").unwrap();
    let td_sel = Selector::parse("td").unwrap();
    let a_sel = Selector::parse("a.movie-name").unwrap();
    let mut out = Vec::new();
    for table in doc.select(&table_sel) {
        for row in table.select(&row_sel) {
            let tds: Vec<_> = row.select(&td_sel).collect();
            if tds.len() < 4 {
                continue;
            }
            let title = match row.select(&a_sel).next() {
                Some(a) => text_of(a),
                None => continue,
            };
            let rank = text_of(tds[0]).parse::<u32>().ok();
            let date = text_of(tds[2]);
            let millions: f64 = match text_of(tds[3]).replace(',', "").parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            out.push(RawRecord {
                source: SOURCE.to_string(),
                source_url: url(year),
                rank_in_source: rank,
                title,
                year: year_from_date(&date).or(year),
                release_date: Some(date),
                nett_cr: None,
                footfalls_cr: Some(millions / 10.0),
                adjusted_nett_cr: None,
                verdict: None,
                is_dubbed_hint: false,
                original_language: None,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_rows() {
        let html = r##"<table class="tablesaw bh-bo-table"><thead><tr><th>Rank</th></tr></thead><tbody>
        <tr class="table-row"><td class="table-cell"> 1</td><td><div class="name"><a class="movie-name" title="Hum Aapke Hain Koun..!" href="#">Hum Aapke Hain Koun..!</a></div></td>
        <td><div class="name"> 06-Aug-1994</div></td><td><div class="name"> 73.96</div></td></tr></tbody></table>"##;
        let rows = parse(html, None).unwrap();
        assert_eq!(rows.len(), 1);
        assert!((rows[0].footfalls_cr.unwrap() - 7.396).abs() < 1e-9);
        assert_eq!(rows[0].year, Some(1994));
    }
}
