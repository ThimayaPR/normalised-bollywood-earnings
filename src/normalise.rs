//! Compute the normalisation lenses for every film. Pure functions, no I/O.

use crate::model::{Film, NormalisedFilm, YearStats};
use crate::util::{median, round};
use std::collections::BTreeMap;

pub const REFERENCE_YEAR: u16 = 2025;
/// Minimum films with nett in a year before the year-relative lens is computed.
pub const MIN_FILMS_FOR_YEAR_LENS: usize = 5;

fn band_fraction(confidence: &str, year: u16) -> f64 {
    match confidence {
        "A" => 0.10,
        "B" => 0.15,
        _ => {
            if year >= 1975 {
                0.25
            } else {
                0.40
            }
        }
    }
}

/// Rank `items` (index, value) descending by value with deterministic tie-breaks:
/// higher nett first, then title ascending. Returns index -> rank (1-based).
fn rank_desc(
    items: Vec<(usize, f64)>,
    films: &[Film],
) -> BTreeMap<usize, u32> {
    let mut v = items;
    v.sort_by(|(ia, a), (ib, b)| {
        b.partial_cmp(a)
            .unwrap()
            .then_with(|| {
                let na = films[*ia].nett_cr.unwrap_or(0.0);
                let nb = films[*ib].nett_cr.unwrap_or(0.0);
                nb.partial_cmp(&na).unwrap()
            })
            .then_with(|| films[*ia].title.cmp(&films[*ib].title))
            .then_with(|| films[*ia].year.cmp(&films[*ib].year))
    });
    v.into_iter().enumerate().map(|(r, (i, _))| (i, (r + 1) as u32)).collect()
}

pub fn normalise(films: &[Film], stats: &BTreeMap<u16, YearStats>) -> Vec<NormalisedFilm> {
    let ref_stats = &stats[&REFERENCE_YEAR];
    let cpi_ref = ref_stats.cpi.expect("reference year CPI");
    let atp_ref = ref_stats.atp_nett.expect("reference year ATP");

    // Year pools of nett for the year-relative lens (all films incl. dubbed: they are
    // part of that year's Hindi market).
    let mut year_nett: BTreeMap<u16, Vec<(f64, usize)>> = BTreeMap::new();
    for (i, f) in films.iter().enumerate() {
        if let Some(n) = f.nett_cr {
            year_nett.entry(f.year).or_default().push((n, i));
        }
    }
    for v in year_nett.values_mut() {
        v.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then_with(|| a.1.cmp(&b.1)));
    }

    // Pass 1: per-film metrics.
    struct Row {
        footfalls: Option<f64>,
        method: String,
        confidence: String,
        low: Option<f64>,
        high: Option<f64>,
        per_1000: Option<f64>,
        cpi_adj: Option<f64>,
        atp_adj: Option<f64>,
        rel: Option<f64>,
        year_n: u32,
    }
    let rows: Vec<Row> = films
        .iter()
        .enumerate()
        .map(|(fi, f)| {
            let ys = &stats[&f.year];
            let (footfalls, method, confidence) = match f.footfalls_sourced_cr {
                Some(ff) => (Some(ff), "sourced".to_string(), "A".to_string()),
                None => match (f.nett_cr, ys.atp_nett) {
                    (Some(n), Some(atp)) => (
                        Some(n / atp),
                        "derived_nett_over_atp".to_string(),
                        if f.year >= 1994 { "B".to_string() } else { "C".to_string() },
                    ),
                    _ => (None, "unavailable".to_string(), "C".to_string()),
                },
            };
            let frac = band_fraction(&confidence, f.year);
            let (low, high) = match footfalls {
                Some(ff) => (Some(ff * (1.0 - frac)), Some(ff * (1.0 + frac))),
                None => (None, None),
            };
            let per_1000 = match (footfalls, ys.population) {
                (Some(ff), Some(pop)) => Some(ff * 1e7 / pop * 1000.0),
                _ => None,
            };
            let cpi_adj = match (f.nett_cr, ys.cpi) {
                (Some(n), Some(c)) => Some(n * cpi_ref / c),
                _ => None,
            };
            let atp_adj = footfalls.map(|ff| ff * atp_ref);
            let pool = year_nett.get(&f.year).map(|v| v.as_slice()).unwrap_or(&[]);
            let year_n = pool.len() as u32;
            let rel = match f.nett_cr {
                Some(n) if pool.len() >= MIN_FILMS_FOR_YEAR_LENS => {
                    // Top 10 by nett, with the film itself removed by index if present.
                    let others: Vec<f64> = pool.iter().take(10).filter(|(_, i)| *i != fi).map(|(v, _)| *v).collect();
                    median(&others).map(|m| n / m)
                }
                _ => None,
            };
            Row { footfalls, method, confidence, low, high, per_1000, cpi_adj, atp_adj, rel, year_n }
        })
        .collect();

    // Pass 2: ranks over non-dubbed films only.
    let eligible = |i: usize| !films[i].is_dubbed;
    let collect = |get: &dyn Fn(&Row) -> Option<f64>| -> Vec<(usize, f64)> {
        rows.iter()
            .enumerate()
            .filter(|(i, _)| eligible(*i))
            .filter_map(|(i, r)| get(r).map(|v| (i, v)))
            .collect()
    };
    let rank_ff = rank_desc(collect(&|r| r.footfalls), films);
    let rank_pc = rank_desc(collect(&|r| r.per_1000), films);
    let rank_cpi = rank_desc(collect(&|r| r.cpi_adj), films);
    let rank_rel = rank_desc(collect(&|r| r.rel), films);
    let rank_norm = rank_desc(collect(&|r| r.atp_adj), films);
    let rank_nom = rank_desc(
        films.iter().enumerate().filter(|(i, _)| eligible(*i)).filter_map(|(i, f)| f.nett_cr.map(|n| (i, n))).collect(),
        films,
    );

    // Tie flags: adjacent films in the footfalls ranking whose uncertainty bands overlap
    // ACROSS tiers (a tier-C estimate next to a tier-A/B figure). Two tier-C classics
    // overlapping each other is expected and not flagged.
    let mut order: Vec<(u32, usize)> = rank_ff.iter().map(|(i, r)| (*r, *i)).collect();
    order.sort();
    let mut tie = vec![false; films.len()];
    for w in order.windows(2) {
        let (a, b) = (w[0].1, w[1].1);
        let (ra, rb) = (&rows[a], &rows[b]);
        let c_involved = (ra.confidence == "C") != (rb.confidence == "C");
        if let (Some(la), Some(hb)) = (ra.low, rb.high) {
            if c_involved && hb >= la {
                tie[a] = true;
                tie[b] = true;
            }
        }
    }

    films
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let r = &rows[i];
            let ys = &stats[&f.year];
            NormalisedFilm {
                rank_normalised: rank_norm.get(&i).copied(),
                rank_footfalls: rank_ff.get(&i).copied(),
                title: f.title.clone(),
                year: f.year,
                release_date: f.release_date.clone(),
                is_dubbed: f.is_dubbed,
                original_language: f.original_language.clone(),
                confidence: r.confidence.clone(),
                nett_cr: f.nett_cr.map(|v| round(v, 2)),
                nett_source: f.nett_source.clone(),
                footfalls_cr: r.footfalls.map(|v| round(v, 4)),
                footfalls_method: r.method.clone(),
                footfalls_source: f.footfalls_source.clone(),
                footfalls_low_cr: r.low.map(|v| round(v, 4)),
                footfalls_high_cr: r.high.map(|v| round(v, 4)),
                footfalls_alt_cr: f.footfalls_alt_cr.map(|v| round(v, 3)),
                footfalls_alt_source: f.footfalls_alt_source.clone(),
                footfalls_consensus_cr: f.footfalls_consensus_cr.map(|v| round(v, 3)),
                atp_year_nett: ys.atp_nett.map(|v| round(v, 2)),
                atp_method: ys.atp_method.clone(),
                population: ys.population.map(|v| round(v, 0)),
                footfalls_per_1000: r.per_1000.map(|v| round(v, 2)),
                cpi_index: ys.cpi.map(|v| round(v, 3)),
                nett_cpi_2025_cr: r.cpi_adj.map(|v| round(v, 1)),
                normalised_boxoffice_2025_cr: r.atp_adj.map(|v| round(v, 2)),
                rel_year_median: r.rel.map(|v| round(v, 3)),
                year_films_with_nett: r.year_n,
                rank_per_capita: rank_pc.get(&i).copied(),
                rank_cpi: rank_cpi.get(&i).copied(),
                rank_rel_year: rank_rel.get(&i).copied(),
                rank_nominal: rank_nom.get(&i).copied(),
                tie_flag: tie[i],
                verdict: f.verdict.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn film(title: &str, year: u16, nett: Option<f64>, ff: Option<f64>, dubbed: bool) -> Film {
        Film {
            key: format!("{title}|{year}"),
            title: title.into(),
            year,
            release_date: None,
            nett_cr: nett,
            nett_source: nett.map(|_| "t".into()),
            footfalls_sourced_cr: ff,
            footfalls_source: ff.map(|_| "t".into()),
            footfalls_alt_cr: None,
            footfalls_alt_source: None,
            is_dubbed: dubbed,
            original_language: None,
            verdict: None,
            footfalls_consensus_cr: None,
        }
    }

    fn stats() -> BTreeMap<u16, YearStats> {
        let mut m = BTreeMap::new();
        for (y, cpi, pop, atp) in [(1975u16, 10.0, 600e6, 1.0), (2016, 150.0, 1300e6, 100.0), (2025, 300.0, 1450e6, 200.0)] {
            m.insert(y, YearStats { year: y, cpi: Some(cpi), cpi_method: "t".into(), population: Some(pop), population_method: "t".into(), atp_nett: Some(atp), atp_method: "t".into(), gdp_per_capita: None, gdp_method: "t".into(), ticket_pct_daily_income: None, boi_total_footfalls_cr: None, boi_total_nett_cr: None });
        }
        m
    }

    #[test]
    fn formulas() {
        let films = vec![
            film("Sholay", 1975, Some(15.0), None, false),
            film("Dangal", 2016, Some(387.0), Some(3.70), false),
            film("Dub", 2016, Some(500.0), Some(5.0), true),
        ];
        let out = normalise(&films, &stats());
        let sholay = &out[0];
        assert_eq!(sholay.footfalls_cr, Some(15.0)); // 15 cr / Rs 1.00
        assert_eq!(sholay.confidence, "C");
        assert_eq!(sholay.footfalls_low_cr, Some(11.25)); // ±25% for 1975+
        assert_eq!(sholay.nett_cpi_2025_cr, Some(450.0)); // 15 × 300/10
        assert_eq!(sholay.normalised_boxoffice_2025_cr, Some(3000.0)); // 15 cr tickets × Rs 200
        assert_eq!(sholay.rank_normalised, Some(1));
        assert_eq!(sholay.footfalls_per_1000, Some(250.0)); // 15e7 / 600e6 × 1000
        let dangal = &out[1];
        assert_eq!(dangal.confidence, "A");
        assert_eq!(dangal.footfalls_per_1000, Some(round(3.70e7 / 1300e6 * 1000.0, 2)));
        // Dubbed film excluded from ranks even though it has more footfalls.
        assert_eq!(out[2].rank_footfalls, None);
        assert_eq!(sholay.rank_footfalls, Some(1));
        assert_eq!(dangal.rank_footfalls, Some(2));
        // Year-relative lens needs >= 5 films in the year.
        assert_eq!(dangal.rel_year_median, None);
    }

    #[test]
    fn year_relative_excludes_self() {
        let mut films: Vec<Film> = (1..=6).map(|k| film(&format!("F{k}"), 2016, Some(k as f64 * 10.0), None, false)).collect();
        films.push(film("Big", 2016, Some(1000.0), None, false));
        let out = normalise(&films, &stats());
        let big = out.iter().find(|r| r.title == "Big").unwrap();
        // Others in top-10: 10,20,30,40,50,60 -> median 35 -> 1000/35
        assert_eq!(big.rel_year_median, Some(round(1000.0 / 35.0, 3)));
        let f1 = out.iter().find(|r| r.title == "F1").unwrap();
        // Others: 20,30,40,50,60,1000 -> median 45 -> 10/45
        assert_eq!(f1.rel_year_median, Some(round(10.0 / 45.0, 3)));
    }
}
