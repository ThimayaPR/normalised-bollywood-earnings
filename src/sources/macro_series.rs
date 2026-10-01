//! Macro series: BIS long consumer price index (1953+) and OWID/UN WPP population (1950+).

use crate::fetch::Fetcher;
use anyhow::{anyhow, Result};
use std::collections::BTreeMap;

pub const BIS_URL: &str = "https://stats.bis.org/api/v1/data/WS_LONG_CPI/A.IN/all?format=csv";
pub const OWID_URL: &str = "https://ourworldindata.org/grapher/population-unwpp.csv?country=~IND";
pub const WB_GDPPC_URL: &str = "https://api.worldbank.org/v2/country/IN/indicator/NY.GDP.PCAP.CN?format=json&per_page=100";

pub fn fetch_cpi(f: &mut Fetcher) -> Result<String> {
    f.get("bis_long_cpi_india", "csv", BIS_URL)
}

pub fn fetch_population(f: &mut Fetcher) -> Result<String> {
    f.get("owid_population_unwpp", "csv", OWID_URL)
}

pub fn fetch_gdp_per_capita(f: &mut Fetcher) -> Result<String> {
    f.get("worldbank_gdp_per_capita_lcu", "json", WB_GDPPC_URL)
}

/// Returns year -> GDP per capita in current rupees (World Bank NY.GDP.PCAP.CN).
pub fn parse_gdp_per_capita(json_text: &str) -> Result<BTreeMap<u16, f64>> {
    let v: serde_json::Value = serde_json::from_str(json_text)?;
    let rows = v.get(1).and_then(|x| x.as_array()).ok_or_else(|| anyhow!("unexpected World Bank JSON shape"))?;
    let mut out = BTreeMap::new();
    for r in rows {
        let year = r.get("date").and_then(|d| d.as_str()).and_then(|d| d.parse::<u16>().ok());
        let val = r.get("value").and_then(|x| x.as_f64());
        if let (Some(y), Some(x)) = (year, val) {
            out.insert(y, x);
        }
    }
    if out.is_empty() {
        return Err(anyhow!("no GDP per capita rows parsed"));
    }
    Ok(out)
}

/// Returns year -> CPI index (BIS UNIT_MEASURE 628, 2010 = 100).
pub fn parse_cpi(csv_text: &str) -> Result<BTreeMap<u16, f64>> {
    let mut rdr = csv::Reader::from_reader(csv_text.as_bytes());
    let hdr = rdr.headers()?.clone();
    let idx = |name: &str| hdr.iter().position(|h| h == name).ok_or_else(|| anyhow!("missing {name}"));
    let (unit_i, period_i, value_i) = (idx("UNIT_MEASURE")?, idx("TIME_PERIOD")?, idx("OBS_VALUE")?);
    let mut out = BTreeMap::new();
    for rec in rdr.records() {
        let rec = rec?;
        if rec.get(unit_i) != Some("628") {
            continue;
        }
        let year: u16 = match rec.get(period_i).and_then(|s| s.parse().ok()) {
            Some(y) => y,
            None => continue,
        };
        if let Some(v) = rec.get(value_i).and_then(|s| s.parse::<f64>().ok()) {
            out.insert(year, v);
        }
    }
    if out.is_empty() {
        return Err(anyhow!("no CPI rows parsed"));
    }
    Ok(out)
}

/// Returns year -> population for India from the OWID grapher CSV (ISO code IND).
pub fn parse_population(csv_text: &str) -> Result<BTreeMap<u16, f64>> {
    let mut rdr = csv::Reader::from_reader(csv_text.as_bytes());
    let hdr = rdr.headers()?.clone();
    let code_i = hdr.iter().position(|h| h == "Code").ok_or_else(|| anyhow!("missing Code"))?;
    let year_i = hdr.iter().position(|h| h == "Year").ok_or_else(|| anyhow!("missing Year"))?;
    let pop_i = hdr
        .iter()
        .position(|h| h.to_lowercase().starts_with("population"))
        .ok_or_else(|| anyhow!("missing population column"))?;
    let mut out = BTreeMap::new();
    for rec in rdr.records() {
        let rec = rec?;
        if rec.get(code_i) != Some("IND") {
            continue;
        }
        if let (Some(y), Some(p)) = (
            rec.get(year_i).and_then(|s| s.parse::<u16>().ok()),
            rec.get(pop_i).and_then(|s| s.parse::<f64>().ok()),
        ) {
            out.insert(y, p);
        }
    }
    if out.is_empty() {
        return Err(anyhow!("no population rows parsed for IND"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpi() {
        let txt = "FREQ,REF_AREA,UNIT_MEASURE,UNIT_MULT,TIME_FORMAT,BREAKS,COVERAGE,DECIMALS,TITLE_TS,TIME_PERIOD,OBS_VALUE,OBS_CONF,OBS_PRE_BREAK,OBS_STATUS\nA,IN,628,0,,,,,,1953,2.740387,F,,A\nA,IN,771,0,,,,,,1953,1.5,F,,A\n";
        let m = parse_cpi(txt).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[&1953], 2.740387);
    }
    #[test]
    fn gdp() {
        let txt = r#"[{"page":1},[{"date":"2025","value":235999.5},{"date":"2024","value":null},{"date":"1960","value":404.44}]]"#;
        let m = parse_gdp_per_capita(txt).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[&1960], 404.44);
    }
    #[test]
    fn pop() {
        let txt = "Entity,Code,Year,Population (historical estimates)\nIndia,IND,1950,371857000\nAfghanistan,AFG,1950,7776180\n";
        let m = parse_population(txt).unwrap();
        assert_eq!(m[&1950], 371857000.0);
    }
}
