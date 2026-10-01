//! keepalivebollywood.com decade tables (old Box Office India figures), used for
//! 1940-1993 nett gross. Columns: Rank, Film, Year, Nett Gross, Adjusted Nett Gross, Verdict.

use crate::fetch::Fetcher;
use crate::model::RawRecord;
use crate::sources::text_of;
use crate::util::parse_indian_int;
use anyhow::Result;
use scraper::{Html, Selector};

pub const SOURCE: &str = "keepalive_decade";
pub const DECADES: [u16; 6] = [1940, 1950, 1960, 1970, 1980, 1990];

pub fn url(decade: u16) -> String {
    format!("https://www.keepalivebollywood.com/boxofficebollywood.php?yr={decade}-{}", decade + 9)
}

pub fn fetch(f: &mut Fetcher, decade: u16) -> Result<String> {
    f.get(&format!("keepalive_{decade}s"), "html", &url(decade))
}

pub fn parse(html: &str, decade: u16) -> Result<Vec<RawRecord>> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("table#AutoNumber1 tr").unwrap();
    let td_sel = Selector::parse(":scope > td").unwrap();
    let mut out = Vec::new();
    for row in doc.select(&row_sel) {
        let tds: Vec<_> = row.select(&td_sel).collect();
        if tds.len() < 5 {
            continue;
        }
        let rank = match text_of(tds[0]).parse::<u32>() {
            Ok(r) => r,
            Err(_) => continue, // header rows
        };
        let title = text_of(tds[1]);
        let year = text_of(tds[2]).parse::<u16>().ok();
        let nett = parse_indian_int(&text_of(tds[3])).ok().map(|v| v / 1e7);
        let adjusted = parse_indian_int(&text_of(tds[4])).ok().map(|v| v / 1e7);
        let verdict = tds.get(5).map(|t| text_of(*t)).filter(|s| !s.is_empty());
        if title.is_empty() || nett.is_none() {
            continue;
        }
        out.push(RawRecord {
            source: SOURCE.to_string(),
            source_url: url(decade),
            rank_in_source: Some(rank),
            title,
            year,
            release_date: None,
            nett_cr: nett,
            footfalls_cr: None,
            adjusted_nett_cr: adjusted,
            verdict,
            is_dubbed_hint: false,
            original_language: None,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_rows() {
        let html = r##"<table id="AutoNumber1"><tr><td colspan="6">Top Earners</td></tr>
        <tr><td>Rank</td><td>Film</td><td>Year</td><td>Nett Gross</td><td>Adjusted Nett Gross</td><td>Verdict</td></tr>
        <tr><td><span><font>1</font></span></td><td><a href="#">Sholay</a></td><td>1975</td><td>15,00,00,000</td><td>162,97,00,000</td><td>All Time Blockbuster</td></tr></table>"##;
        let rows = parse(html, 1970).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Sholay");
        assert_eq!(rows[0].year, Some(1975));
        assert_eq!(rows[0].nett_cr, Some(15.0));
        assert_eq!(rows[0].adjusted_nett_cr, Some(162.97));
    }
}
