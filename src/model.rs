//! Core data types shared across the pipeline.

use serde::{Deserialize, Serialize};

/// One row as parsed from a single source, before merging.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawRecord {
    pub source: String,
    pub source_url: String,
    pub rank_in_source: Option<u32>,
    pub title: String,
    pub year: Option<u16>,
    pub release_date: Option<String>,
    pub nett_cr: Option<f64>,
    pub footfalls_cr: Option<f64>,
    /// Source-provided "adjusted nett" (keepalive / old BOI), kept for shape checks only.
    pub adjusted_nett_cr: Option<f64>,
    pub verdict: Option<String>,
    pub is_dubbed_hint: bool,
    pub original_language: Option<String>,
}

/// Yearly macro / industry figures.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct YearStats {
    pub year: u16,
    pub cpi: Option<f64>,
    pub cpi_method: String,
    pub population: Option<f64>,
    pub population_method: String,
    pub atp_nett: Option<f64>,
    pub atp_method: String,
    /// GDP per capita in current rupees (World Bank), a proxy for average income.
    pub gdp_per_capita: Option<f64>,
    pub gdp_method: String,
    /// Ticket price as a share of one day's per-capita income, in percent.
    pub ticket_pct_daily_income: Option<f64>,
    /// BOI header: industry-wide Hindi footfalls for the year (crore), 1994-2017 only.
    pub boi_total_footfalls_cr: Option<f64>,
    pub boi_total_nett_cr: Option<f64>,
}

/// One film after merging all sources.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Film {
    pub key: String,
    pub title: String,
    pub year: u16,
    pub release_date: Option<String>,
    pub nett_cr: Option<f64>,
    pub nett_source: Option<String>,
    pub footfalls_sourced_cr: Option<f64>,
    pub footfalls_source: Option<String>,
    pub footfalls_alt_cr: Option<f64>,
    pub footfalls_alt_source: Option<String>,
    pub is_dubbed: bool,
    pub original_language: Option<String>,
    pub verdict: Option<String>,
    /// Published consensus estimate (Wikipedia ticket-sales table), comparison only.
    pub footfalls_consensus_cr: Option<f64>,
}

/// Final normalised row written to the output CSV.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NormalisedFilm {
    /// Headline rank: normalised box office (identical ordering to footfalls).
    pub rank_normalised: Option<u32>,
    pub rank_footfalls: Option<u32>,
    pub title: String,
    pub year: u16,
    pub release_date: Option<String>,
    pub is_dubbed: bool,
    pub original_language: Option<String>,
    pub confidence: String,
    pub nett_cr: Option<f64>,
    pub nett_source: Option<String>,
    pub footfalls_cr: Option<f64>,
    pub footfalls_method: String,
    pub footfalls_source: Option<String>,
    pub footfalls_low_cr: Option<f64>,
    pub footfalls_high_cr: Option<f64>,
    pub footfalls_alt_cr: Option<f64>,
    pub footfalls_alt_source: Option<String>,
    pub footfalls_consensus_cr: Option<f64>,
    pub atp_year_nett: Option<f64>,
    pub atp_method: String,
    pub population: Option<f64>,
    pub footfalls_per_1000: Option<f64>,
    pub cpi_index: Option<f64>,
    pub nett_cpi_2025_cr: Option<f64>,
    /// Box office earnings normalised for ticket price: footfalls × 2025 ticket price.
    pub normalised_boxoffice_2025_cr: Option<f64>,
    pub rel_year_median: Option<f64>,
    pub year_films_with_nett: u32,
    pub rank_per_capita: Option<u32>,
    pub rank_cpi: Option<u32>,
    pub rank_rel_year: Option<u32>,
    pub rank_nominal: Option<u32>,
    pub tie_flag: bool,
    pub verdict: Option<String>,
}
