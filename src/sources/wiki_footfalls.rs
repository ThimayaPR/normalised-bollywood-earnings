//! Wikipedia "List of highest-grossing films in India" ticket-sales (footfalls) table.
//! Used ONLY as a published-consensus comparison column, never as a ranking input.

use crate::fetch::Fetcher;
use crate::sources::text_of;
use anyhow::Result;
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

pub const URL: &str = "https://en.wikipedia.org/wiki/List_of_highest-grossing_films_in_India";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsensusRow {
    pub title: String,
    pub year: u16,
    pub footfalls_cr: f64,
    pub range_text: String,
    pub language: Option<String>,
}

pub fn fetch(f: &mut Fetcher) -> Result<String> {
    f.get("wiki_highest_grossing_india", "html", URL)
}

pub fn parse(html: &str) -> Result<Vec<ConsensusRow>> {
    let doc = Html::parse_document(html);
    let table_sel = Selector::parse("table.wikitable").unwrap();
    let tr_sel = Selector::parse("tr").unwrap();
    let cell_sel = Selector::parse("td").unwrap();
    let th_sel = Selector::parse("th").unwrap();
    let year_re = Regex::new(r"^(19[3-9]\d|20[0-2]\d)$").unwrap();
    let num_re = Regex::new(r"[\d,]{7,}").unwrap();
    let mut out = Vec::new();
    for table in doc.select(&table_sel) {
        let hdr: Vec<String> = table.select(&th_sel).map(|t| text_of(t).to_lowercase()).collect();
        if !hdr.iter().any(|h| h.contains("ticket sales") || h.contains("footfall") || h.contains("admission")) {
            continue;
        }
        for row in table.select(&tr_sel) {
            let cells: Vec<String> = row.select(&cell_sel).map(text_of).collect();
            // Rows with a rowspanned rank have one cell fewer; locate the year cell instead.
            let year_i = match cells.iter().position(|c| year_re.is_match(c.trim())) {
                Some(i) if i >= 1 && i + 1 < cells.len() => i,
                _ => continue,
            };
            let title = cells[year_i - 1].trim().to_string();
            let year: u16 = cells[year_i].trim().parse().unwrap();
            let sales_text = cells[year_i + 1].clone();
            let nums: Vec<f64> = num_re
                .find_iter(&sales_text)
                .filter_map(|m| m.as_str().replace(',', "").parse::<f64>().ok())
                .collect();
            if nums.is_empty() {
                continue;
            }
            // Midpoint of a range, in crore tickets.
            let mid = (nums.iter().sum::<f64>() / nums.len() as f64) / 1e7;
            let language = cells.get(year_i + 2).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            out.push(ConsensusRow { title, year, footfalls_cr: mid, range_text: sales_text.trim().to_string(), language });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_rowspan_rows() {
        let html = r##"<table class="wikitable"><tr><th>Rank</th><th>Title</th><th>Year</th><th>Ticket sales (est.)</th><th>Language(s)</th></tr>
        <tr><td rowspan="2">2</td><td>Mughal-e-Azam</td><td>1960</td><td>100,000,000</td><td>Hindustani</td></tr>
        <tr><td>Mother India</td><td>1957</td><td>100,000,000</td><td>Hindi</td></tr>
        <tr><td>1</td><td>Sholay</td><td>1975</td><td>150,000,000–180,000,000</td><td>Hindi</td></tr></table>"##;
        let rows = parse(html).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].title, "Mother India");
        assert_eq!(rows[1].footfalls_cr, 10.0);
        assert_eq!(rows[2].footfalls_cr, 16.5);
    }
}
