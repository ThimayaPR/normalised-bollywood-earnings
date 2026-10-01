//! Sanity checks against published consensus. Each check is data, not code, so the
//! report can print it and the build can fail loudly when a check misses.

use crate::merge::DubbedRow;
use crate::model::NormalisedFilm;
use crate::util::norm_title;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CheckResult {
    pub check: String,
    pub target: String,
    pub expected: String,
    pub actual: String,
    pub pass: bool,
}

fn find<'a>(rows: &'a [NormalisedFilm], title: &str, year: u16) -> Option<&'a NormalisedFilm> {
    rows.iter().find(|r| r.title.eq_ignore_ascii_case(title) && (r.year as i32 - year as i32).abs() <= 1)
}

pub fn run(rows: &[NormalisedFilm], dubbed: &[DubbedRow]) -> Vec<CheckResult> {
    let mut out = Vec::new();

    // Footfall bands (crore tickets) from BOI's all-time footfalls page (independent of
    // our ATP series), and the three anchor films whose footfalls are *defined* by the
    // ticket-price anchors (so these only confirm the anchors were applied, not measured).
    let bands: [(&str, &str, u16, f64, f64); 7] = [
        ("footfalls_in_band", "Hum Aapke Hain Koun", 1994, 7.0, 7.8),
        ("footfalls_in_band", "Gadar", 2001, 4.8, 5.3),
        ("footfalls_in_band", "Dilwale Dulhania Le Jayenge", 1995, 4.5, 5.1),
        ("footfalls_in_band", "Dangal", 2016, 3.5, 3.9),
        ("anchor_film_reproduced", "Sholay", 1975, 10.0, 18.0),
        ("anchor_film_reproduced", "Mughal-e-Azam", 1960, 7.0, 13.0),
        ("anchor_film_reproduced", "Mother India", 1957, 7.0, 13.0),
    ];
    for (check, title, year, lo, hi) in bands {
        let (actual, pass) = match find(rows, title, year).and_then(|r| r.footfalls_cr) {
            Some(v) => (format!("{v:.2}"), v >= lo && v <= hi),
            None => ("missing".to_string(), false),
        };
        out.push(CheckResult {
            check: check.into(),
            target: format!("{title} ({year})"),
            expected: format!("{lo}-{hi} cr"),
            actual,
            pass,
        });
    }

    // 2023 nominal leaders should sit mid-table on footfalls within the 1994+ cohort
    // (the era BOI tracks directly), and never in the overall top 5.
    let mut cohort: Vec<&NormalisedFilm> = rows.iter().filter(|r| r.year >= 1994 && r.rank_footfalls.is_some()).collect();
    cohort.sort_by_key(|r| r.rank_footfalls.unwrap());
    for (title, year) in [("Jawan", 2023), ("Pathaan", 2023), ("Gadar 2", 2023), ("Animal", 2023)] {
        let cohort_rank = cohort.iter().position(|r| r.title.eq_ignore_ascii_case(title) && r.year == year).map(|p| p as u32 + 1);
        let overall = find(rows, title, year).and_then(|r| r.rank_footfalls);
        let (actual, pass) = match (cohort_rank, overall) {
            (Some(c), Some(o)) => (format!("cohort {c}, overall {o}"), (6..=30).contains(&c) && o > 5),
            _ => ("missing".to_string(), false),
        };
        out.push(CheckResult {
            check: "rank_footfalls_1994plus_cohort".into(),
            target: format!("{title} ({year})"),
            expected: "cohort rank 6-30 and overall > 5".into(),
            actual,
            pass,
        });
    }

    // Derived (tier C) footfalls vs published consensus: most should be within 40%.
    let mut n = 0;
    let mut ok = 0;
    for r in rows {
        if r.confidence == "C" {
            if let (Some(a), Some(c)) = (r.footfalls_cr, r.footfalls_consensus_cr) {
                n += 1;
                if ((a - c) / c).abs() <= 0.40 {
                    ok += 1;
                }
            }
        }
    }
    let share = if n > 0 { ok as f64 / n as f64 } else { 0.0 };
    out.push(CheckResult {
        check: "tier_c_within_40pct_of_consensus".into(),
        target: format!("{n} tier-C films with a consensus figure"),
        expected: ">= 60% within 40%".into(),
        actual: format!("{:.0}% ({ok}/{n})", share * 100.0),
        pass: n == 0 || share >= 0.60,
    });

    // Dubbed films present but excluded from the headline ranking.
    for (title, year) in [("Bahubali 2 - The Conclusion", 2017), ("KGF Chapter 2", 2022), ("Pushpa 2", 2024)] {
        let (actual, pass) = match find(rows, title, year) {
            Some(r) => (format!("is_dubbed={} rank={:?}", r.is_dubbed, r.rank_footfalls), r.is_dubbed && r.rank_footfalls.is_none()),
            None => ("missing".to_string(), false),
        };
        out.push(CheckResult {
            check: "dubbed_flagged_and_unranked".into(),
            target: format!("{title} ({year})"),
            expected: "is_dubbed=true rank=None".into(),
            actual,
            pass,
        });
    }

    // No dubbed film in the headline list: by flag, and independently by matching ranked
    // titles against the curated dubbed list (catches spelling variants the merge missed).
    let leak = rows.iter().filter(|r| r.is_dubbed && r.rank_footfalls.is_some()).count();
    out.push(CheckResult {
        check: "no_dubbed_in_headline".into(),
        target: "flagged rows".into(),
        expected: "0".into(),
        actual: leak.to_string(),
        pass: leak == 0,
    });
    let mut leaks: Vec<String> = Vec::new();
    for d in dubbed {
        let dn = norm_title(&d.title);
        for r in rows.iter().filter(|r| r.rank_footfalls.is_some() && r.year == d.year) {
            let rn = norm_title(&r.title);
            // Exact match, the ranked title is a truncation of the dubbed title, or the
            // ranked title extends it only by a subtitle marker (part / chapter / the).
            let extends_with_marker = rn.starts_with(&format!("{dn} "))
                && ["part", "chapter", "the", "1", "2"].iter().any(|w| rn[dn.len() + 1..].starts_with(w));
            if rn == dn || dn.starts_with(&format!("{rn} ")) || extends_with_marker {
                leaks.push(format!("{} ({})", r.title, r.year));
            }
        }
    }
    leaks.sort();
    leaks.dedup();
    out.push(CheckResult {
        check: "no_dubbed_title_variant_ranked".into(),
        target: "ranked titles vs curated dubbed list".into(),
        expected: "0".into(),
        actual: if leaks.is_empty() { "0".into() } else { leaks.join("; ") },
        pass: leaks.is_empty(),
    });

    // Cross-source agreement: BOI vs Hungama footfalls within 15% for most films.
    let mut compared = 0;
    let mut agree = 0;
    for r in rows {
        if let (Some(a), Some(b)) = (r.footfalls_cr, r.footfalls_alt_cr) {
            if r.footfalls_method == "sourced" {
                compared += 1;
                if ((a - b) / a).abs() <= 0.15 {
                    agree += 1;
                }
            }
        }
    }
    let share = if compared > 0 { agree as f64 / compared as f64 } else { 0.0 };
    out.push(CheckResult {
        check: "cross_source_footfalls_agree_15pct".into(),
        target: format!("{compared} films with two footfall sources"),
        expected: ">= 80% within 15%".into(),
        actual: format!("{:.0}% ({agree}/{compared})", share * 100.0),
        pass: compared == 0 || share >= 0.80,
    });

    out
}
