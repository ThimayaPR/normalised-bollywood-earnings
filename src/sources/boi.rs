//! Box Office India year pages: nett (pageId=4) and footfalls (pageId=6), plus the
//! yearly header block (average ticket price, total footfalls, total nett).

use crate::fetch::Fetcher;
use crate::model::RawRecord;
use crate::sources::text_of;
use crate::util::{parse_indian_int, year_from_date};
use anyhow::Result;
use regex::Regex;
use scraper::{Html, Selector};

pub const SOURCE: &str = "boi_year_page";
pub const FIRST_YEAR: u16 = 1994;
pub const LAST_YEAR: u16 = 2024;

pub fn url(year: u16, page_id: u8) -> String {
    format!("https://www.boxofficeindia.com/years.php?year={year}&pageId={page_id}")
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct YearHeader {
    pub atp: Option<f64>,
    pub total_footfalls_cr: Option<f64>,
    pub total_nett_cr: Option<f64>,
}

pub fn fetch_year(f: &mut Fetcher, year: u16, page_id: u8) -> Result<String> {
    f.get(&format!("boi_{year}_p{page_id}"), "html", &url(year, page_id))
}

/// Parse the header block. Values show "---" when BOI has no data for that year.
pub fn parse_header(html: &str) -> YearHeader {
    let grab = |label: &str| -> Option<f64> {
        let re = Regex::new(&format!(
            r"(?s){}\s*</td>\s*<td[^>]*>(.*?)</td>",
            regex::escape(label)
        ))
        .unwrap();
        let cell = re.captures(html)?.get(1)?.as_str();
        let txt = Regex::new(r"<[^>]+>").unwrap().replace_all(cell, " ");
        let txt = txt.trim();
        if txt.starts_with("---") || txt.is_empty() {
            return None;
        }
        let num = Regex::new(r"[\d,]+(?:\.\d+)?").unwrap().find(txt)?.as_str().replace(',', "");
        num.parse().ok()
    };
    YearHeader {
        atp: grab("Average Ticket Price"),
        total_footfalls_cr: grab("Total Footfalls"),
        total_nett_cr: grab("Total Nett Gross"),
    }
}

/// Parse the ranked list. `value_is_nett` selects whether the 4th column is rupees
/// (nett) or a ticket count (footfalls).
pub fn parse_list(html: &str, year: u16, page_id: u8, value_is_nett: bool) -> Result<Vec<RawRecord>> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("tr.boi-listing-rows").unwrap();
    let td_sel = Selector::parse(":scope > td").unwrap();
    let a_sel = Selector::parse("a[href*='movie.php']").unwrap();
    let mut out = Vec::new();
    for row in doc.select(&row_sel) {
        let tds: Vec<_> = row.select(&td_sel).collect();
        if tds.len() < 4 {
            continue;
        }
        let rank = text_of(tds[0]).parse::<u32>().ok();
        let title = match row.select(&a_sel).map(text_of).find(|t| !t.is_empty()) {
            Some(t) => t,
            None => continue,
        };
        let date = text_of(tds[2]);
        let value = match parse_indian_int(&text_of(tds[3])) {
            Ok(v) => v,
            Err(_) => continue, // BOI shows "--" when a figure is unavailable
        };
        if !value_is_nett && value == 0.0 {
            continue; // BOI shows 0 footfalls as a placeholder when it has no estimate
        }
        let verdict = tds.get(4).map(|t| text_of(*t)).filter(|s| !s.is_empty());
        let crore = value / 1e7;
        out.push(RawRecord {
            source: SOURCE.to_string(),
            source_url: url(year, page_id),
            rank_in_source: rank,
            title,
            year: year_from_date(&date).or(Some(year)),
            release_date: Some(date),
            nett_cr: if value_is_nett { Some(crore) } else { None },
            footfalls_cr: if value_is_nett { None } else { Some(crore) },
            adjusted_nett_cr: None,
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

    const SAMPLE: &str = r#"
    <table><tr><td> Average Ticket Price</td><td style="x"><span><img src="a.png"/></span>49.16</td></tr>
    <tr><td> Total Footfalls </td><td>20.55 crore</td></tr>
    <tr><td> Total Nett Gross</td><td><span></span>704.20 crore</td></tr></table>
    <table><tr class="grayrow boi-listing-rows"><td>1</td><td><table><tr><td><a href="movie.php?movieid=389"><img title="No Entry"/></a></td>
    <td><a href="movie.php?movieid=389">No Entry</a></td></tr></table></td><td>26 Aug 2005</td>
    <td><span class="rupeesblack"><img/></span>44,72,00,000</td><td>Super Hit</td></tr></table>"#;

    #[test]
    fn header() {
        let h = parse_header(SAMPLE);
        assert_eq!(h.atp, Some(49.16));
        assert_eq!(h.total_footfalls_cr, Some(20.55));
        assert_eq!(h.total_nett_cr, Some(704.20));
        let none = parse_header("<td> Average Ticket Price</td><td>---</td>");
        assert_eq!(none.atp, None);
    }

    #[test]
    fn list() {
        let rows = parse_list(SAMPLE, 2005, 4, true).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "No Entry");
        assert_eq!(rows[0].nett_cr, Some(44.72));
        assert_eq!(rows[0].year, Some(2005));
        assert_eq!(rows[0].verdict.as_deref(), Some("Super Hit"));
    }
}
