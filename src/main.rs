//! hindi_bo: deterministic, era-normalised ranking of top-grossing Hindi films in India.
//!
//! Commands:
//!   fetch   download every source into data/raw (cache-first; safe to re-run)
//!   build   parse the cache offline, merge, normalise, write output/*.csv, run checks
//!   report  render output/report.html from the CSVs
//!   all     fetch + build + report

mod atp;
mod checks;
mod fetch;
mod merge;
mod model;
mod normalise;
mod report;
mod sources;
mod util;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use fetch::Fetcher;
use model::RawRecord;
use sources::{boi, hungama, keepalive, macro_series, wiki, wiki_footfalls};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "hindi_bo", version, about)]
struct Cli {
    /// Project root containing data/ and output/
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Fetch,
    Build {
        /// Do not fail the build when a sanity check misses
        #[arg(long)]
        no_strict: bool,
    },
    Report,
    All {
        #[arg(long)]
        no_strict: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.root.canonicalize().context("root dir")?;
    match cli.cmd {
        Cmd::Fetch => fetch_all(&root),
        Cmd::Build { no_strict } => build(&root, !no_strict),
        Cmd::Report => report::render(&root),
        Cmd::All { no_strict } => {
            fetch_all(&root)?;
            build(&root, !no_strict)?;
            report::render(&root)
        }
    }
}

fn fetch_all(root: &Path) -> Result<()> {
    let mut f = Fetcher::new(&root.join("data/raw"), false)?;
    let mut failures = Vec::new();
    for year in boi::FIRST_YEAR..=boi::LAST_YEAR {
        for page in [4u8, 6u8] {
            if let Err(e) = boi::fetch_year(&mut f, year, page) {
                failures.push(format!("boi {year} p{page}: {e}"));
            }
        }
    }
    if let Err(e) = hungama::fetch(&mut f, None) {
        failures.push(format!("hungama all-time: {e}"));
    }
    for year in hungama::FIRST_YEAR..=hungama::LAST_YEAR {
        if let Err(e) = hungama::fetch(&mut f, Some(year)) {
            failures.push(format!("hungama {year}: {e}"));
        }
    }
    for decade in keepalive::DECADES {
        if let Err(e) = keepalive::fetch(&mut f, decade) {
            failures.push(format!("keepalive {decade}s: {e}"));
        }
    }
    if let Err(e) = wiki::fetch(&mut f) {
        failures.push(format!("wiki: {e}"));
    }
    if let Err(e) = wiki_footfalls::fetch(&mut f) {
        failures.push(format!("wiki footfalls: {e}"));
    }
    if let Err(e) = macro_series::fetch_cpi(&mut f) {
        failures.push(format!("bis cpi: {e}"));
    }
    if let Err(e) = macro_series::fetch_population(&mut f) {
        failures.push(format!("owid population: {e}"));
    }
    if let Err(e) = macro_series::fetch_gdp_per_capita(&mut f) {
        failures.push(format!("world bank gdp per capita: {e}"));
    }
    if failures.is_empty() {
        eprintln!("fetch: all sources cached under data/raw");
        Ok(())
    } else {
        for m in &failures {
            eprintln!("fetch failure: {m}");
        }
        Err(anyhow!("{} fetch failure(s)", failures.len()))
    }
}

fn write_csv<T: serde::Serialize>(path: &Path, rows: &[T]) -> Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let mut w = csv::Writer::from_path(path)?;
    for r in rows {
        w.serialize(r)?;
    }
    w.flush()?;
    Ok(())
}

fn build(root: &Path, strict: bool) -> Result<()> {
    let raw = root.join("data/raw");
    let manual = root.join("data/manual");
    let interim = root.join("data/interim");
    let output = root.join("output");
    fs::create_dir_all(&interim)?;
    fs::create_dir_all(&output)?;
    // Offline: the build never touches the network, so it is reproducible from the cache.
    let mut f = Fetcher::new(&raw, true)?;

    // ---- parse sources ----------------------------------------------------
    let mut boi_nett: Vec<RawRecord> = Vec::new();
    let mut boi_ff: Vec<RawRecord> = Vec::new();
    let mut boi_headers: BTreeMap<u16, boi::YearHeader> = BTreeMap::new();
    for year in boi::FIRST_YEAR..=boi::LAST_YEAR {
        if let Ok(html) = boi::fetch_year(&mut f, year, 4) {
            boi_headers.insert(year, boi::parse_header(&html));
            boi_nett.extend(boi::parse_list(&html, year, 4, true)?);
        }
        if let Ok(html) = boi::fetch_year(&mut f, year, 6) {
            boi_ff.extend(boi::parse_list(&html, year, 6, false)?);
        }
    }
    let mut hungama_ff: Vec<RawRecord> = Vec::new();
    if let Ok(html) = hungama::fetch(&mut f, None) {
        hungama_ff.extend(hungama::parse(&html, None)?);
    }
    for year in hungama::FIRST_YEAR..=hungama::LAST_YEAR {
        if let Ok(html) = hungama::fetch(&mut f, Some(year)) {
            hungama_ff.extend(hungama::parse(&html, Some(year))?);
        }
    }
    let mut keep: Vec<RawRecord> = Vec::new();
    for decade in keepalive::DECADES {
        if let Ok(html) = keepalive::fetch(&mut f, decade) {
            keep.extend(keepalive::parse(&html, decade)?);
        }
    }
    let wiki_rows = wiki::parse(&wiki::fetch(&mut f)?)?;
    let (wiki_original, wiki_dubbed, wiki_by_year): (Vec<_>, Vec<_>, Vec<_>) = {
        let mut o = Vec::new();
        let mut d = Vec::new();
        let mut y = Vec::new();
        for r in wiki_rows {
            match r.source.as_str() {
                wiki::SOURCE_ORIGINAL => o.push(r),
                wiki::SOURCE_DUBBED => d.push(r),
                _ => y.push(r),
            }
        }
        (o, d, y)
    };
    let consensus = wiki_footfalls::parse(&wiki_footfalls::fetch(&mut f)?)?;
    write_csv(&interim.join("wiki_footfalls_consensus.csv"), &consensus)?;
    let cpi = macro_series::parse_cpi(&macro_series::fetch_cpi(&mut f)?)?;
    let population = macro_series::parse_population(&macro_series::fetch_population(&mut f)?)?;
    let gdp_per_capita = macro_series::parse_gdp_per_capita(&macro_series::fetch_gdp_per_capita(&mut f)?)?;

    let articles = merge::load_articles(&manual.join("boi_articles.csv"))?;
    let article_recs = merge::articles_to_records(&articles);

    write_csv(&interim.join("boi_nett.csv"), &boi_nett)?;
    write_csv(&interim.join("boi_footfalls.csv"), &boi_ff)?;
    write_csv(&interim.join("hungama_footfalls.csv"), &hungama_ff)?;
    write_csv(&interim.join("keepalive.csv"), &keep)?;
    write_csv(&interim.join("wiki_original.csv"), &wiki_original)?;
    write_csv(&interim.join("wiki_dubbed.csv"), &wiki_dubbed)?;
    write_csv(&interim.join("wiki_by_year.csv"), &wiki_by_year)?;
    eprintln!(
        "parsed: boi nett {} / footfalls {} rows, hungama {}, keepalive {}, wiki {}+{}+{}, cpi {} yrs, pop {} yrs",
        boi_nett.len(), boi_ff.len(), hungama_ff.len(), keep.len(),
        wiki_original.len(), wiki_dubbed.len(), wiki_by_year.len(), cpi.len(), population.len()
    );

    // ---- merge ------------------------------------------------------------
    let aliases = merge::Aliases::load(&manual.join("title_aliases.csv"))?;
    let dubbed = merge::load_dubbed(&manual.join("dubbed_films.csv"))?;
    // Nett precedence: BOI year pages -> Wikipedia original -> Wikipedia dubbed ->
    // BOI articles -> keepalive (pre-1994) -> Wikipedia by-year (top-1 per year).
    let nett_sources = [boi_nett, wiki_original, wiki_dubbed, article_recs.clone(), keep, wiki_by_year];
    // Footfalls precedence: BOI year pages -> BOI articles -> Hungama.
    let footfall_sources = [boi_ff, article_recs, hungama_ff];
    let films = merge::merge(&nett_sources, &footfall_sources, &aliases, &dubbed, &consensus);
    write_csv(&interim.join("films_merged.csv"), &films)?;
    let dupes = merge::possible_duplicates(&films);
    write_csv(&output.join("possible_duplicates.csv"), &dupes)?;
    eprintln!("possible duplicates for alias review: {} (output/possible_duplicates.csv)", dupes.len());
    eprintln!("merged: {} films ({} dubbed)", films.len(), films.iter().filter(|f| f.is_dubbed).count());

    // ---- yearly series ----------------------------------------------------
    let anchors: Vec<atp::AtpAnchor> = atp::load_csv(&manual.join("atp_anchors.csv"))?;
    let cpi_manual: Vec<atp::CpiManual> = atp::load_csv(&manual.join("cpi_pre1953.csv"))?;
    let pop_manual: Vec<atp::PopManual> = atp::load_csv(&manual.join("population_pre1950.csv"))?;
    let ormax: Vec<atp::OrmaxAtp> = atp::load_csv(&manual.join("ormax_atp.csv"))?;
    let stats = atp::build_year_stats(&atp::Inputs {
        gdp_per_capita: &gdp_per_capita,
        cpi_bis: &cpi,
        cpi_manual: &cpi_manual,
        population: &population,
        population_manual: &pop_manual,
        boi_headers: &boi_headers,
        anchors: &anchors,
        ormax: &ormax,
        films: &films,
    })?;
    let stats_rows: Vec<_> = stats.values().cloned().collect();
    write_csv(&output.join("atp_series.csv"), &stats_rows)?;

    // ---- normalise --------------------------------------------------------
    let mut rows = normalise::normalise(&films, &stats);
    rows.sort_by(|a, b| {
        a.rank_normalised
            .unwrap_or(u32::MAX)
            .cmp(&b.rank_normalised.unwrap_or(u32::MAX))
            .then_with(|| b.footfalls_cr.partial_cmp(&a.footfalls_cr).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.title.cmp(&b.title))
    });
    write_csv(&output.join("hindi_films_normalised.csv"), &rows)?;

    // ---- checks -----------------------------------------------------------
    let results = checks::run(&rows, &dubbed);
    write_csv(&output.join("checks.csv"), &results)?;
    let failed: Vec<_> = results.iter().filter(|c| !c.pass).collect();
    for c in &results {
        eprintln!("[{}] {} :: {} expected {} got {}", if c.pass { "PASS" } else { "FAIL" }, c.check, c.target, c.expected, c.actual);
    }
    eprintln!("build: {} films written, {} checks, {} failed", rows.len(), results.len(), failed.len());
    if strict && !failed.is_empty() {
        return Err(anyhow!("{} sanity check(s) failed; see output/checks.csv", failed.len()));
    }
    Ok(())
}
