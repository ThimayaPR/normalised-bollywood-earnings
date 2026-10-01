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
    #[serde(default)]
    canonical_year: Option<u16>,
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
    map: BTreeMap<(String, Option<u16>), (String, Option<u16>)>,
}

impl Aliases {
    pub fn load(path: &Path) -> Result<Self> {
        let mut map = BTreeMap::new();
        let mut rdr = csv::Reader::from_path(path)?;
        for row in rdr.deserialize::<AliasRow>() {
            let r = row?;
            map.insert((norm_title(&r.source_title), r.year), (r.canonical_title, r.canonical_year));
        }
        Ok(Self { map })
    }

    /// Canonical title and (possibly corrected) year for a source title.
    pub fn canonical(&self, title: &str, year: Option<u16>) -> (String, Option<u16>) {
        let title = title.trim().trim_end_matches('*').trim();
        let n = norm_title(title);
        if let Some((c, y)) = self.map.get(&(n.clone(), year)) {
            return (c.clone(), y.or(year));
        }
        if let Some((c, y)) = self.map.get(&(n, None)) {
            return (c.clone(), y.or(year));
        }
        (title.to_string(), year)
    }
}

/// Language tag at the end of a title, e.g. "Spider-Man (English)" -> Some("English").
fn language_tag(title: &str) -> Option<String> {
    let t = title.trim();
    let open = t.rfind('(')?;
    if !t.ends_with(')') {
        return None;
    }
    let tag = t[open + 1..t.len() - 1].trim();
    const LANGS: [&str; 8] = ["english", "tamil", "telugu", "kannada", "malayalam", "marathi", "bengali", "punjabi"];
    LANGS.iter().find(|l| tag.eq_ignore_ascii_case(l)).map(|l| {
        let mut c = l.chars();
        c.next().unwrap().to_uppercase().collect::<String>() + c.as_str()
    })
}

/// Title with a trailing subtitle removed: "Lagaan: Once Upon a Time in India" -> "Lagaan".
fn title_stem(title: &str) -> Option<String> {
    for sep in [" - ", ": ", " – ", " — ", "..."] {
        if let Some(i) = title.find(sep) {
            let stem = title[..i].trim();
            if stem.chars().count() >= 4 {
                return Some(stem.to_string());
            }
        }
    }
    None
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
    /// normalised title with spaces removed ("biwino1", "nh10")
    by_compact: BTreeMap<String, Vec<usize>>,
    /// normalised title stem before a subtitle separator ("lagaan")
    by_stem: BTreeMap<String, Vec<usize>>,
}

fn compact(norm: &str) -> String {
    norm.replace(' ', "")
}

impl Index {
    fn new() -> Self {
        Self { films: Vec::new(), by_norm: BTreeMap::new(), by_compact: BTreeMap::new(), by_stem: BTreeMap::new() }
    }

    fn unique_in_year(&self, cands: Option<&Vec<usize>>, year: u16, tol: i32) -> Option<usize> {
        let c = cands?;
        if let Some(&i) = c.iter().find(|&&i| self.films[i].year == year) {
            return Some(i);
        }
        let near: Vec<usize> = c.iter().copied().filter(|&i| (self.films[i].year as i32 - year as i32).abs() <= tol).collect();
        if near.len() == 1 {
            Some(near[0])
        } else {
            None
        }
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

    /// For nett rows from different sources: exact -> fuzzy -> compacted spelling or
    /// subtitle stem, the last two only within the same year.
    fn find_for_nett(&self, title: &str, norm: &str, year: u16) -> Option<usize> {
        if let Some(i) = self.find_or_fuzzy(norm, year) {
            return Some(i);
        }
        if norm.chars().count() >= 4 {
            if let Some(i) = self.unique_in_year(self.by_compact.get(&compact(norm)), year, 0) {
                return Some(i);
            }
            if let Some(stem) = title_stem(title) {
                if let Some(i) = self.unique_in_year(self.by_norm.get(&norm_title(&stem)), year, 0) {
                    return Some(i);
                }
            }
            if let Some(i) = self.unique_in_year(self.by_stem.get(norm), year, 0) {
                return Some(i);
            }
        }
        None
    }

    /// Widest matching, used for footfall-only rows whose spelling is least controlled:
    /// exact -> fuzzy -> compacted spelling ("Biwi No.1" = "Biwi No. 1") -> subtitle stem
    /// in either direction ("Lagaan" = "Lagaan: Once Upon a Time in India").
    fn find_widest(&self, title: &str, norm: &str, year: u16) -> Option<usize> {
        if let Some(i) = self.find_or_fuzzy(norm, year) {
            return Some(i);
        }
        if norm.chars().count() >= 4 {
            if let Some(i) = self.unique_in_year(self.by_compact.get(&compact(norm)), year, 1) {
                return Some(i);
            }
        }
        if let Some(stem) = title_stem(title) {
            let sn = norm_title(&stem);
            if let Some(i) = self.unique_in_year(self.by_norm.get(&sn), year, 1) {
                return Some(i);
            }
        }
        if norm.chars().count() >= 4 {
            if let Some(i) = self.unique_in_year(self.by_stem.get(norm), year, 1) {
                return Some(i);
            }
        }
        None
    }

    fn insert(&mut self, film: Film) -> usize {
        let norm = norm_title(&film.title);
        let stem = title_stem(&film.title).map(|st| norm_title(&st));
        self.films.push(film);
        let i = self.films.len() - 1;
        self.by_compact.entry(compact(&norm)).or_default().push(i);
        self.by_norm.entry(norm).or_default().push(i);
        if let Some(st) = stem {
            self.by_stem.entry(st).or_default().push(i);
        }
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
            let (title, year) = match aliases.canonical(&r.title, r.year) {
                (t, Some(y)) => (t, y),
                _ => continue,
            };
            let norm = norm_title(&title);
            let i = match idx.find_for_nett(&title, &norm, year) {
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
            let ff = match r.footfalls_cr {
                Some(ff) if ff > 0.0 => ff, // zero is a placeholder, not data
                _ => continue,
            };
            // Language-tagged rows: Hollywood releases are not Hindi films and are dropped;
            // other tags mark a dubbed South Indian film.
            let lang = language_tag(&r.title);
            if lang.as_deref() == Some("English") {
                continue;
            }
            let bare = match r.title.rfind('(') {
                Some(i) if lang.is_some() => r.title[..i].trim().to_string(),
                _ => r.title.clone(),
            };
            let (title, year) = match aliases.canonical(&bare, r.year) {
                (t, Some(y)) => (t, y),
                _ => continue,
            };
            let norm = norm_title(&title);
            let dubbed_hint = r.is_dubbed_hint || lang.is_some();
            let i = match idx.find_widest(&title, &norm, year) {
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
                    is_dubbed: dubbed_hint,
                    original_language: r.original_language.clone().or(lang.clone()),
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
            if dubbed_hint {
                f.is_dubbed = true;
                if f.original_language.is_none() {
                    f.original_language = r.original_language.clone().or(lang.clone());
                }
            }
            if f.release_date.is_none() {
                f.release_date = r.release_date.clone();
            }
        }
    }

    // Curated dubbed list.
    for d in dubbed {
        let (title, year) = aliases.canonical(&d.title, Some(d.year));
        let year = year.unwrap_or(d.year);
        if let Some(i) = idx.find_widest(&title, &norm_title(&title), year) {
            let f = &mut idx.films[i];
            f.is_dubbed = true;
            if f.original_language.is_none() {
                f.original_language = Some(d.original_language.clone());
            }
        }
    }

    // Published consensus footfalls (comparison column only; never creates a film).
    for c in consensus {
        let (title, year) = aliases.canonical(&c.title, Some(c.year));
        let year = year.unwrap_or(c.year);
        if let Some(i) = idx.find_widest(&title, &norm_title(&title), year) {
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
            // A footfall-only row whose footfalls exactly equal another film's, within three
            // years, is almost always the same film under a different title or date.
            let footfall_twin = (a.nett_cr.is_none() || b.nett_cr.is_none())
                && (a.year as i32 - b.year as i32).abs() <= 3
                && matches!((a.footfalls_sourced_cr, b.footfalls_sourced_cr), (Some(x), Some(y)) if (x - y).abs() < 1e-9 && x > 0.05);
            if footfall_twin {
                out.push(PossibleDuplicate {
                    year: a.year,
                    title_a: a.title.clone(),
                    title_b: b.title.clone(),
                    nett_a: a.nett_cr,
                    nett_b: b.nett_cr,
                    source_a: a.nett_source.clone().or(a.footfalls_source.clone()),
                    source_b: b.nett_source.clone().or(b.footfalls_source.clone()),
                });
                continue;
            }
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
    fn stem_compact_and_language_tags() {
        let aliases = Aliases { map: BTreeMap::new() };
        let nett = vec![vec![
            rec("boi", "Lagaan: Once Upon a Time in India", 2001, Some(32.0), None),
            rec("boi", "Biwi No. 1", 1999, Some(21.0), None),
            rec("boi", "Garv", 2004, Some(7.0), None),
        ]];
        let ff = vec![vec![
            rec("hungama", "Lagaan", 2001, None, Some(1.5)),
            rec("hungama", "Biwi No.1", 1999, None, Some(1.1)),
            rec("hungama", "Garv - Pride & Honour", 2004, None, Some(0.4)),
            rec("hungama", "Spider-Man - No Way Home (English)", 2021, None, Some(2.0)),
            rec("hungama", "Jailer (Tamil)", 2023, None, Some(0.3)),
        ]];
        let films = merge(&nett, &ff, &aliases, &[], &[]);
        let by = |t: &str| films.iter().find(|f| f.title == t).unwrap();
        assert_eq!(films.len(), 4, "{:?}", films.iter().map(|f| &f.title).collect::<Vec<_>>());
        assert_eq!(by("Lagaan: Once Upon a Time in India").footfalls_sourced_cr, Some(1.5));
        assert_eq!(by("Biwi No. 1").footfalls_sourced_cr, Some(1.1));
        assert_eq!(by("Garv").footfalls_sourced_cr, Some(0.4));
        let jailer = by("Jailer");
        assert!(jailer.is_dubbed);
        assert_eq!(jailer.original_language.as_deref(), Some("Tamil"));
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
