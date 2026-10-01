//! Merge per-source records into one row per film with explicit source precedence.
//! All iteration is over sorted structures so the result is deterministic.

use crate::model::{Film, RawRecord};
use crate::sources::wiki_footfalls::ConsensusRow;
use crate::util::{levenshtein, norm_title};
use anyhow::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct AliasRow {
    source_title: String,
    year: Option<u16>,
    canonical_title: String,
}

#[derive(Debug, Deserialize)]
pub struct DubbedRow {
    pub title: String,
    pub year: u16,
    pub original_language: String,
}

#[derive(Debug, Deserialize)]
pub struct ArticleRow {
    pub title: String,
    pub year: u16,
    pub hindi_nett_cr: Option<f64>,
    pub hindi_footfalls_cr: Option<f64>,
    pub is_dubbed: bool,
    pub original_language: Option<String>,
    pub source_url: String,
}

pub struct Aliases {
    map: BTreeMap<(String, Option<u16>), String>,
}

impl Aliases {
    pub fn load(path: &Path) -> Result<Self> {
        let mut map = BTreeMap::new();
        let mut rdr = csv::Reader::from_path(path)?;
        for row in rdr.deserialize::<AliasRow>() {
            let r = row?;
            map.insert((norm_title(&r.source_title), r.year), r.canonical_title);
        }
        Ok(Self { map })
    }

    pub fn canonical(&self, title: &str, year: Option<u16>) -> String {
        let title = title.trim().trim_end_matches('*').trim();
        let n = norm_title(title);
        if let Some(c) = self.map.get(&(n.clone(), year)) {
            return c.clone();
        }
        if let Some(c) = self.map.get(&(n, None)) {
            return c.clone();
        }
        title.trim().to_string()
    }
}

pub fn load_dubbed(path: &Path) -> Result<Vec<DubbedRow>> {
    let mut rdr = csv::Reader::from_path(path)?;
    Ok(rdr.deserialize::<DubbedRow>().collect::<Result<Vec<_>, _>>()?)
}

pub fn load_articles(path: &Path) -> Result<Vec<ArticleRow>> {
    let mut rdr = csv::Reader::from_path(path)?;
    Ok(rdr.deserialize::<ArticleRow>().collect::<Result<Vec<_>, _>>()?)
}

pub fn articles_to_records(rows: &[ArticleRow]) -> Vec<RawRecord> {
    rows.iter()
        .map(|r| RawRecord {
            source: "boi_article".to_string(),
            source_url: r.source_url.clone(),
            rank_in_source: None,
            title: r.title.clone(),
            year: Some(r.year),
            release_date: None,
            nett_cr: r.hindi_nett_cr,
            footfalls_cr: r.hindi_footfalls_cr,
            adjusted_nett_cr: None,
            verdict: None,
            is_dubbed_hint: r.is_dubbed,
            original_language: r.original_language.clone(),
        })
        .collect()
}

/// Index of films by normalised title for fuzzy (±1 year) matching.
struct Index {
    films: Vec<Film>,
    by_norm: BTreeMap<String, Vec<usize>>,
}

impl Index {
    fn new() -> Self {
        Self { films: Vec::new(), by_norm: BTreeMap::new() }
    }

    /// Exact normalised-title match in the same year, else in an adjacent year if unique.
    fn find(&self, norm: &str, year: u16) -> Option<usize> {
        let cands = self.by_norm.get(norm)?;
        if let Some(&i) = cands.iter().find(|&&i| self.films[i].year == year) {
            return Some(i);
        }
        let near: Vec<usize> = cands
            .iter()
            .copied()
            .filter(|&i| (self.films[i].year as i32 - year as i32).abs() == 1)
            .collect();
        if near.len() == 1 {
            Some(near[0])
        } else {
            None
        }
    }

    /// Fallback for spelling variants ("Imtihaan"/"Imtihan"): same year, edit distance
    /// <= 2, title length >= 8, and exactly one candidate. Deterministic.
    fn find_fuzzy(&self, norm: &str, year: u16) -> Option<usize> {
        if norm.chars().count() < 8 {
            return None;
        }
        let mut hits: Vec<usize> = Vec::new();
        for (other, idxs) in &self.by_norm {
            if (other.chars().count() as i32 - norm.chars().count() as i32).abs() > 2 {
                continue;
            }
            if levenshtein(norm, other) <= 2 {
                hits.extend(idxs.iter().copied().filter(|&i| self.films[i].year == year));
            }
        }
        if hits.len() == 1 {
            Some(hits[0])
        } else {
            None
        }
    }

    fn find_or_fuzzy(&self, norm: &str, year: u16) -> Option<usize> {
        self.find(norm, year).or_else(|| self.find_fuzzy(norm, year))
    }

    fn insert(&mut self, film: Film) -> usize {
        let norm = norm_title(&film.title);
        self.films.push(film);
        let i = self.films.len() - 1;
        self.by_norm.entry(norm).or_default().push(i);
        i
    }
}

/// `nett_sources` and `footfall_sources` are ordered by precedence (first wins).
pub fn merge(
    nett_sources: &[Vec<RawRecord>],
    footfall_sources: &[Vec<RawRecord>],
    aliases: &Aliases,
    dubbed: &[DubbedRow],
    consensus: &[ConsensusRow],
) -> Vec<Film> {
    let mut idx = Index::new();

    for recs in nett_sources {
        for r in recs {
            let year = match r.year {
                Some(y) => y,
                None => continue,
            };
            let title = aliases.canonical(&r.title, Some(year));
            let norm = norm_title(&title);
            let i = match idx.find(&norm, year) {
                Some(i) => i,
                None => idx.insert(Film {
                    key: format!("{norm}|{year}"),
                    title: title.clone(),
                    year,
                    release_date: None,
                    nett_cr: None,
                    nett_source: None,
                    footfalls_sourced_cr: None,
                    footfalls_source: None,
                    footfalls_alt_cr: None,
                    footfalls_alt_source: None,
                    is_dubbed: false,
                    original_language: None,
                    verdict: None,
                    footfalls_consensus_cr: None,
                }),
            };
            let f = &mut idx.films[i];
            if f.nett_cr.is_none() {
                if let Some(n) = r.nett_cr {
                    f.nett_cr = Some(n);
                    f.nett_source = Some(r.source.clone());
                }
            }
            if f.release_date.is_none() {
                f.release_date = r.release_date.clone();
            }
            if f.verdict.is_none() {
                f.verdict = r.verdict.clone();
            }
            if r.is_dubbed_hint {
                f.is_dubbed = true;
                if f.original_language.is_none() {
                    f.original_language = r.original_language.clone();
                }
            }
        }
    }

    for recs in footfall_sources {
        for r in recs {
            let (year, ff) = match (r.year, r.footfalls_cr) {
                (Some(y), Some(ff)) if ff > 0.0 => (y, ff), // zero is a placeholder, not data
                _ => continue,
            };
            let title = aliases.canonical(&r.title, Some(year));
            let norm = norm_title(&title);
            let i = match idx.find_or_fuzzy(&norm, year) {
                Some(i) => i,
                None => idx.insert(Film {
                    key: format!("{norm}|{year}"),
                    title: title.clone(),
                    year,
                    release_date: r.release_date.clone(),
                    nett_cr: None,
                    nett_source: None,
                    footfalls_sourced_cr: None,
                    footfalls_source: None,
                    footfalls_alt_cr: None,
                    footfalls_alt_source: None,
                    is_dubbed: r.is_dubbed_hint,
                    original_language: r.original_language.clone(),
                    verdict: None,
                    footfalls_consensus_cr: None,
                }),
            };
            let f = &mut idx.films[i];
            match (&f.footfalls_source, &f.footfalls_alt_source) {
                (None, _) => {
                    f.footfalls_sourced_cr = Some(ff);
                    f.footfalls_source = Some(r.source.clone());
                }
                (Some(s), None) if s != &r.source => {
                    f.footfalls_alt_cr = Some(ff);
                    f.footfalls_alt_source = Some(r.source.clone());
                }
                _ => {}
            }
            if r.is_dubbed_hint {
                f.is_dubbed = true;
                if f.original_language.is_none() {
                    f.original_language = r.original_language.clone();
                }
            }
            if f.release_date.is_none() {
                f.release_date = r.release_date.clone();
            }
        }
    }

    // Curated dubbed list.
    for d in dubbed {
        let title = aliases.canonical(&d.title, Some(d.year));
        if let Some(i) = idx.find_or_fuzzy(&norm_title(&title), d.year) {
            let f = &mut idx.films[i];
            f.is_dubbed = true;
            if f.original_language.is_none() {
                f.original_language = Some(d.original_language.clone());
            }
        }
    }

    // Published consensus footfalls (comparison column only; never creates a film).
    for c in consensus {
        let title = aliases.canonical(&c.title, Some(c.year));
        if let Some(i) = idx.find_or_fuzzy(&norm_title(&title), c.year) {
            let f = &mut idx.films[i];
            if f.footfalls_consensus_cr.is_none() {
                f.footfalls_consensus_cr = Some(c.footfalls_cr);
            }
        }
    }

    let mut films = idx.films;
    films.sort_by(|a, b| a.year.cmp(&b.year).then_with(|| a.key.cmp(&b.key)));
    films
}

/// Same-year title pairs within edit distance 2 that were NOT merged. Written out so
/// aliases can be curated by hand; this never changes the data.
#[derive(Debug, serde::Serialize)]
pub struct PossibleDuplicate {
    pub year: u16,
    pub title_a: String,
    pub title_b: String,
    pub nett_a: Option<f64>,
    pub nett_b: Option<f64>,
    pub source_a: Option<String>,
    pub source_b: Option<String>,
}

pub fn possible_duplicates(films: &[Film]) -> Vec<PossibleDuplicate> {
    let mut out = Vec::new();
    for (i, a) in films.iter().enumerate() {
        for b in films.iter().skip(i + 1) {
            if a.year != b.year {
                continue;
            }
            let (na, nb) = (norm_title(&a.title), norm_title(&b.title));
            if na.chars().count() < 5 || nb.chars().count() < 5 {
                continue;
            }
            if levenshtein(&na, &nb) <= 2 || na.replace(' ', "") == nb.replace(' ', "") {
                out.push(PossibleDuplicate {
                    year: a.year,
                    title_a: a.title.clone(),
                    title_b: b.title.clone(),
                    nett_a: a.nett_cr,
                    nett_b: b.nett_cr,
                    source_a: a.nett_source.clone().or(a.footfalls_source.clone()),
                    source_b: b.nett_source.clone().or(b.footfalls_source.clone()),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(source: &str, title: &str, year: u16, nett: Option<f64>, ff: Option<f64>) -> RawRecord {
        RawRecord {
            source: source.into(),
            source_url: String::new(),
            rank_in_source: None,
            title: title.into(),
            year: Some(year),
            release_date: None,
            nett_cr: nett,
            footfalls_cr: ff,
            adjusted_nett_cr: None,
            verdict: None,
            is_dubbed_hint: false,
            original_language: None,
        }
    }

    #[test]
    fn precedence_and_alt_footfalls() {
        let aliases = Aliases { map: BTreeMap::new() };
        let nett = vec![vec![rec("boi", "Dangal", 2016, Some(387.0), None)], vec![rec("wiki", "Dangal", 2016, Some(400.0), None)]];
        let ff = vec![vec![rec("boi", "Dangal", 2016, None, Some(3.70))], vec![rec("hungama", "Dangal", 2016, None, Some(3.60))]];
        let films = merge(&nett, &ff, &aliases, &[], &[]);
        assert_eq!(films.len(), 1);
        assert_eq!(films[0].nett_cr, Some(387.0));
        assert_eq!(films[0].nett_source.as_deref(), Some("boi"));
        assert_eq!(films[0].footfalls_sourced_cr, Some(3.70));
        assert_eq!(films[0].footfalls_alt_cr, Some(3.60));
    }

    #[test]
    fn fuzzy_spelling_attaches_footfalls() {
        let aliases = Aliases { map: BTreeMap::new() };
        let nett = vec![vec![rec("boi", "Rocky Aur Rani Ki Prem Kahaani", 2023, Some(147.5), None)]];
        let ff = vec![vec![rec("hungama", "Rocky Aur Rani Kii Prem Kahaani", 2023, None, Some(1.1))]];
        let films = merge(&nett, &ff, &aliases, &[], &[]);
        assert_eq!(films.len(), 1);
        assert_eq!(films[0].footfalls_sourced_cr, Some(1.1));
        // Short titles never fuzzy-match (Dus vs D, Mann vs Mast).
        let nett = vec![vec![rec("boi", "Mann", 1999, Some(16.6), None), rec("boi", "Mast", 1999, Some(5.3), None)]];
        let ff = vec![vec![rec("hungama", "Mano", 1999, None, Some(0.5))]];
        let films = merge(&nett, &ff, &aliases, &[], &[]);
        assert_eq!(films.len(), 3);
    }

    #[test]
    fn year_tolerance_and_dubbed() {
        let aliases = Aliases { map: BTreeMap::new() };
        let nett = vec![vec![rec("boi", "KGF Chapter 2", 2022, Some(427.0), None)]];
        let ff = vec![vec![rec("hungama", "KGF Chapter 2", 2023, None, Some(2.5))]];
        let dubbed = vec![DubbedRow { title: "KGF Chapter 2".into(), year: 2022, original_language: "Kannada".into() }];
        let films = merge(&nett, &ff, &aliases, &dubbed, &[]);
        assert_eq!(films.len(), 1);
        assert!(films[0].is_dubbed);
        assert_eq!(films[0].footfalls_sourced_cr, Some(2.5));
    }
}
