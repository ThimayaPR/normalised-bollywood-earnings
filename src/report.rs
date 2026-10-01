//! Render output/report.html by substituting JSON into templates/report.html.
//! Pure templating: the page is a deterministic function of the output CSVs.

use crate::model::{NormalisedFilm, YearStats};
use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::Path;

const TEMPLATE: &str = include_str!("../templates/report.html");

#[derive(Serialize)]
struct FilmJs<'a> {
    t: &'a str,
    y: u16,
    d: bool,
    l: Option<&'a str>,
    c: &'a str,
    n: Option<f64>,
    ns: Option<&'a str>,
    f: Option<f64>,
    fm: &'a str,
    fs: Option<&'a str>,
    lo: Option<f64>,
    hi: Option<f64>,
    fa: Option<f64>,
    fas: Option<&'a str>,
    fc: Option<f64>,
    atp: Option<f64>,
    pc: Option<f64>,
    cpi: Option<f64>,
    a25: Option<f64>,
    rel: Option<f64>,
    rz: Option<u32>,
    rf: Option<u32>,
    rn: Option<u32>,
    tie: bool,
}

#[derive(Serialize)]
struct SeriesJs {
    year: u16,
    atp: Option<f64>,
    atp_method: String,
    cpi: Option<f64>,
    pop: Option<f64>,
    gdp: Option<f64>,
    gdp_method: String,
    tix_pct: Option<f64>,
}

#[derive(Serialize)]
struct Source {
    name: &'static str,
    role: &'static str,
    url: &'static str,
}

#[derive(Serialize)]
struct Meta {
    atp_2025: f64,
    sources: Vec<Source>,
}

fn read_csv<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<Vec<T>> {
    let mut rdr = csv::Reader::from_path(path).with_context(|| format!("open {}", path.display()))?;
    Ok(rdr.deserialize::<T>().collect::<Result<Vec<_>, _>>()?)
}

pub fn render(root: &Path) -> Result<()> {
    let output = root.join("output");
    let films: Vec<NormalisedFilm> = read_csv(&output.join("hindi_films_normalised.csv"))?;
    let series: Vec<YearStats> = read_csv(&output.join("atp_series.csv"))?;
    let checks: Vec<crate::checks::CheckResult> = read_csv(&output.join("checks.csv"))?;

    let films_js: Vec<FilmJs> = films
        .iter()
        .map(|r| FilmJs {
            t: &r.title,
            y: r.year,
            d: r.is_dubbed,
            l: r.original_language.as_deref(),
            c: &r.confidence,
            n: r.nett_cr,
            ns: r.nett_source.as_deref(),
            f: r.footfalls_cr,
            fm: &r.footfalls_method,
            fs: r.footfalls_source.as_deref(),
            lo: r.footfalls_low_cr,
            hi: r.footfalls_high_cr,
            fa: r.footfalls_alt_cr,
            fas: r.footfalls_alt_source.as_deref(),
            fc: r.footfalls_consensus_cr,
            atp: r.atp_year_nett,
            pc: r.footfalls_per_1000,
            cpi: r.nett_cpi_2025_cr,
            a25: r.normalised_boxoffice_2025_cr,
            rel: r.rel_year_median,
            rz: r.rank_normalised,
            rf: r.rank_footfalls,
            rn: r.rank_nominal,
            tie: r.tie_flag,
        })
        .collect();
    let series_js: Vec<SeriesJs> = series
        .iter()
        .map(|s| SeriesJs { year: s.year, atp: s.atp_nett, atp_method: s.atp_method.clone(), cpi: s.cpi, pop: s.population, gdp: s.gdp_per_capita, gdp_method: s.gdp_method.clone(), tix_pct: s.ticket_pct_daily_income })
        .collect();
    let atp_2025 = series.iter().find(|s| s.year == 2025).and_then(|s| s.atp_nett).unwrap_or(0.0);
    let meta = Meta {
        atp_2025,
        sources: vec![
            Source { name: "Box Office India year pages", role: "nett and footfalls per film 1994-2024, yearly average ticket price 1994-2017", url: "https://www.boxofficeindia.com/years.php?year=2005&pageId=4" },
            Source { name: "Box Office India, Top Ten Footfalls Post Pandemic", role: "Hindi-version footfalls 2022-2026", url: "https://www.boxofficeindia.com/report-details.php?articleid=9794" },
            Source { name: "Bollywood Hungama footfalls", role: "alternate footfalls 1994-2026", url: "https://www.bollywoodhungama.com/box-office-collections/footfalls/" },
            Source { name: "keepalivebollywood decade tables", role: "pre-1994 nett (old Box Office India figures)", url: "https://www.keepalivebollywood.com/boxofficebollywood.php?yr=1970-1979" },
            Source { name: "Wikipedia, highest domestic net collection of Hindi films", role: "nett 2024-2026, dubbed-film table, top film by year 1940-2026", url: "https://en.wikipedia.org/wiki/List_of_highest_domestic_net_collection_of_Hindi_films" },
            Source { name: "Wikipedia, highest-grossing films in India", role: "published ticket-sales consensus (comparison only)", url: "https://en.wikipedia.org/wiki/List_of_highest-grossing_films_in_India" },
            Source { name: "Ormax Media box office reports", role: "Hindi average ticket price 2022, 2023, 2025", url: "https://www.ormaxmedia.com/stories/the-ormax-box-office-report-2025.html" },
            Source { name: "BIS long consumer price index", role: "CPI 1953-2025", url: "https://stats.bis.org/api/v1/data/WS_LONG_CPI/A.IN/all?format=csv" },
            Source { name: "Our World in Data / UN World Population Prospects", role: "population 1950-2023", url: "https://ourworldindata.org/grapher/population-unwpp.csv?country=~IND" },
            Source { name: "World Bank, GDP per capita (current LCU)", role: "average income proxy 1960-2025", url: "https://api.worldbank.org/v2/country/IN/indicator/NY.GDP.PCAP.CN?format=json&per_page=100" },
            Source { name: "Box Office India, Sholay at 50 (Galaxy Rajkot record)", role: "1975 single-screen nett and footfalls used as a ticket-price anchor", url: "https://boxofficeindia.com/report-details.php?articleid=9313" },
        ],
    };

    let page = TEMPLATE
        .replace("__FILMS_JSON__", &serde_json::to_string(&films_js)?)
        .replace("__SERIES_JSON__", &serde_json::to_string(&series_js)?)
        .replace("__CHECKS_JSON__", &serde_json::to_string(&checks)?)
        .replace("__META_JSON__", &serde_json::to_string(&meta)?);
    let out = output.join("report.html");
    fs::write(&out, page)?;
    eprintln!("report: wrote {} ({} films)", out.display(), films.len());
    Ok(())
}
