//! Yearly series: CPI, population and the Hindi nett average ticket price (ATP).
//! Every value carries a method label so the provenance of each year is explicit.

use crate::model::{Film, YearStats};
use crate::sources::boi::YearHeader;
use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

pub const FIRST_YEAR: u16 = 1940;
pub const LAST_YEAR: u16 = 2026;

#[derive(Debug, Deserialize)]
pub struct AtpAnchor {
    pub year: u16,
    pub nett_atp_rupees: f64,
}

#[derive(Debug, Deserialize)]
pub struct CpiManual {
    pub year: u16,
    pub cpi_index_2010_100: f64,
}

#[derive(Debug, Deserialize)]
pub struct PopManual {
    pub year: u16,
    pub population: f64,
}

#[derive(Debug, Deserialize)]
pub struct OrmaxAtp {
    pub year: u16,
    pub hindi_atp_gross_rupees: f64,
    pub gst_factor: f64,
}

pub fn load_csv<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>> {
    let mut rdr = csv::Reader::from_path(path)?;
    Ok(rdr.deserialize::<T>().collect::<Result<Vec<_>, _>>()?)
}

/// Log-linear interpolation between known points, returning (value, is_exact_point).
/// Years outside the known range return None.
pub fn log_interp(points: &BTreeMap<u16, f64>, year: u16) -> Option<(f64, bool)> {
    if let Some(v) = points.get(&year) {
        return Some((*v, true));
    }
    let (&y0, &v0) = points.range(..year).next_back()?;
    let (&y1, &v1) = points.range(year + 1..).next()?;
    let t = (year - y0) as f64 / (y1 - y0) as f64;
    Some(((v0.ln() * (1.0 - t) + v1.ln() * t).exp(), false))
}

pub struct Inputs<'a> {
    pub gdp_per_capita: &'a BTreeMap<u16, f64>,
    pub cpi_bis: &'a BTreeMap<u16, f64>,
    pub cpi_manual: &'a [CpiManual],
    pub population: &'a BTreeMap<u16, f64>,
    pub population_manual: &'a [PopManual],
    pub boi_headers: &'a BTreeMap<u16, YearHeader>,
    pub anchors: &'a [AtpAnchor],
    pub ormax: &'a [OrmaxAtp],
    pub films: &'a [Film],
}

pub fn build_year_stats(inp: &Inputs) -> Result<BTreeMap<u16, YearStats>> {
    // --- CPI ---------------------------------------------------------------
    let mut cpi_points: BTreeMap<u16, f64> = inp.cpi_manual.iter().map(|c| (c.year, c.cpi_index_2010_100)).collect();
    for (y, v) in inp.cpi_bis {
        cpi_points.insert(*y, *v);
    }
    let bis_first = *inp.cpi_bis.keys().next().ok_or_else(|| anyhow!("empty BIS CPI"))?;
    let bis_last = *inp.cpi_bis.keys().next_back().unwrap();

    // --- Population --------------------------------------------------------
    // Census anchors take priority for the pre-OWID interpolation; OWID is used directly
    // for every year it covers (see the assembly loop below).
    let mut pop_points: BTreeMap<u16, f64> = inp.population.clone();
    for p in inp.population_manual {
        pop_points.insert(p.year, p.population);
    }
    let pop_first = *inp.population.keys().next().ok_or_else(|| anyhow!("empty population"))?;
    let pop_last = *inp.population.keys().next_back().unwrap();
    // 5-year CAGR at the end of the series for forward extrapolation.
    let pop_cagr = {
        let a = inp.population[&(pop_last - 5)];
        let b = inp.population[&pop_last];
        (b / a).powf(1.0 / 5.0)
    };

    // --- ATP: BOI header years -------------------------------------------
    let mut atp: BTreeMap<u16, (f64, String)> = BTreeMap::new();
    for (y, h) in inp.boi_headers {
        if let Some(a) = h.atp {
            atp.insert(*y, (a, "boi_header".to_string()));
        }
    }
    let boi_first = *atp.keys().next().ok_or_else(|| anyhow!("no BOI header ATP"))?;
    let boi_last = *atp.keys().next_back().unwrap();

    // --- ATP: derived from BOI-matched films for 2018-2020 only (BOI footfall pages are
    //     empty from 2021; Ormax exact points take over from 2022) --------------------
    let derived_last = 2020.min(LAST_YEAR);
    for y in (boi_last + 1)..=derived_last {
        let (mut sn, mut sf, mut n) = (0.0, 0.0, 0u32);
        for f in inp.films.iter().filter(|f| f.year == y) {
            if f.nett_source.as_deref() == Some("boi_year_page") && f.footfalls_source.as_deref() == Some("boi_year_page") {
                if let (Some(nt), Some(ff)) = (f.nett_cr, f.footfalls_sourced_cr) {
                    sn += nt;
                    sf += ff;
                    n += 1;
                }
            }
        }
        if n >= 10 && sf > 0.0 {
            atp.insert(y, (sn / sf, format!("derived_boi_matched_n{n}")));
        }
    }

    // --- ATP: Ormax nett (gross / GST factor) as exact points ---------------
    let ormax_points: BTreeMap<u16, f64> =
        inp.ormax.iter().map(|o| (o.year, o.hindi_atp_gross_rupees / o.gst_factor)).collect();
    for (y, v) in &ormax_points {
        atp.insert(*y, (*v, "ormax_nett".to_string())); // exact points override anything derived
    }

    // --- ATP: fill gaps 1994+ by log-linear interpolation between known points;
    //     carry the last known value forward (e.g. 2026 from 2025). ---------------
    let known: BTreeMap<u16, f64> = atp.iter().map(|(y, (v, _))| (*y, *v)).collect();
    let known_last = *known.keys().next_back().unwrap();
    for y in boi_first..=LAST_YEAR {
        if atp.contains_key(&y) {
            continue;
        }
        if y > known_last {
            atp.insert(y, (known[&known_last], "carry_forward".to_string()));
        } else if let Some((v, _)) = log_interp(&known, y) {
            atp.insert(y, (v, "interpolated".to_string()));
        }
    }

    // --- ATP: pre-BOI anchors, log-linear to the first BOI header year -------
    let mut anchor_points: BTreeMap<u16, f64> = inp.anchors.iter().map(|a| (a.year, a.nett_atp_rupees)).collect();
    anchor_points.insert(boi_first, atp[&boi_first].0);
    for y in FIRST_YEAR..boi_first {
        if let Some((v, exact)) = log_interp(&anchor_points, y) {
            atp.insert(y, (v, if exact { "anchor".into() } else { "anchor_interpolated".into() }));
        }
    }

    // --- GDP per capita: World Bank 1960+, later years extrapolated with 5-year CAGR ---
    let gdp_last = *inp.gdp_per_capita.keys().next_back().ok_or_else(|| anyhow!("empty GDP per capita"))?;
    let gdp_cagr = (inp.gdp_per_capita[&gdp_last] / inp.gdp_per_capita[&(gdp_last - 5)]).powf(1.0 / 5.0);

    // --- Assemble ----------------------------------------------------------
    let mut out = BTreeMap::new();
    for y in FIRST_YEAR..=LAST_YEAR {
        let (cpi, cpi_method) = if let Some(v) = inp.cpi_bis.get(&y) {
            (Some(*v), "bis".to_string())
        } else if y > bis_last {
            (Some(inp.cpi_bis[&bis_last]), "bis_carry_forward".to_string())
        } else if y < bis_first {
            match log_interp(&cpi_points, y) {
                Some((v, true)) => (Some(v), "manual_anchor".to_string()),
                Some((v, false)) => (Some(v), "manual_interpolated".to_string()),
                None => (None, "missing".to_string()),
            }
        } else {
            (None, "missing".to_string())
        };
        let (pop, pop_method) = if let Some(v) = inp.population.get(&y) {
            (Some(*v), "owid_unwpp".to_string())
        } else if y > pop_last {
            (Some(inp.population[&pop_last] * pop_cagr.powi((y - pop_last) as i32)), "extrapolated_cagr5".to_string())
        } else if y < pop_first {
            match log_interp(&pop_points, y) {
                Some((v, true)) => (Some(v), "census".to_string()),
                Some((v, false)) => (Some(v), "census_interpolated".to_string()),
                None => {
                    // Before the first census anchor: extrapolate backwards from the first two points.
                    let mut it = pop_points.iter();
                    let (&y0, &v0) = it.next().unwrap();
                    let (&y1, &v1) = it.next().unwrap();
                    let r = (v1 / v0).powf(1.0 / (y1 - y0) as f64);
                    (Some(v0 / r.powi((y0 - y) as i32)), "census_extrapolated".to_string())
                }
            }
        } else {
            (None, "missing".to_string())
        };
        let (atp_v, atp_m) = atp.get(&y).map(|(v, m)| (Some(*v), m.clone())).unwrap_or((None, "missing".into()));
        let (gdp, gdp_method) = if let Some(v) = inp.gdp_per_capita.get(&y) {
            (Some(*v), "worldbank".to_string())
        } else if y > gdp_last {
            (Some(inp.gdp_per_capita[&gdp_last] * gdp_cagr.powi((y - gdp_last) as i32)), "extrapolated_cagr5".to_string())
        } else {
            (None, "missing".to_string())
        };
        let ticket_pct_daily_income = match (atp_v, gdp) {
            (Some(a), Some(g)) if g > 0.0 => Some(a / (g / 365.0) * 100.0),
            _ => None,
        };
        let h = inp.boi_headers.get(&y);
        out.insert(
            y,
            YearStats {
                year: y,
                cpi,
                cpi_method,
                population: pop,
                population_method: pop_method,
                atp_nett: atp_v,
                atp_method: atp_m,
                gdp_per_capita: gdp,
                gdp_method,
                ticket_pct_daily_income,
                boi_total_footfalls_cr: h.and_then(|h| h.total_footfalls_cr),
                boi_total_nett_cr: h.and_then(|h| h.total_nett_cr),
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_interp_is_geometric() {
        let mut p = BTreeMap::new();
        p.insert(2000u16, 1.0);
        p.insert(2002u16, 4.0);
        let (v, exact) = log_interp(&p, 2001).unwrap();
        assert!(!exact);
        assert!((v - 2.0).abs() < 1e-9);
        assert_eq!(log_interp(&p, 1999), None);
        assert_eq!(log_interp(&p, 2000), Some((1.0, true)));
    }
}
