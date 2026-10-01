//! Deterministic helpers: number parsing, title normalisation, path handling.

use anyhow::{anyhow, Result};
use regex::Regex;
use std::sync::OnceLock;

/// Parse an Indian-formatted integer such as "7,39,62,000" into a plain f64.
pub fn parse_indian_int(s: &str) -> Result<f64> {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    if cleaned.is_empty() {
        return Err(anyhow!("no digits in {s:?}"));
    }
    cleaned
        .parse::<f64>()
        .map_err(|e| anyhow!("parse {s:?}: {e}"))
}

/// Parse rupee text such as "₹1,108.70 crore", "₹55 lakh", "4 crore (est.)" into crore.
pub fn parse_rupees_to_crore(s: &str) -> Option<f64> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)([\d,]+(?:\.\d+)?)\s*(crore|cr|lakh|lacs?|million)").unwrap()
    });
    let caps = re.captures(s)?;
    let num: f64 = caps[1].replace(',', "").parse().ok()?;
    let unit = caps[2].to_ascii_lowercase();
    Some(match unit.as_str() {
        "crore" | "cr" => num,
        "lakh" | "lac" | "lacs" => num / 100.0,
        "million" => num / 10.0,
        _ => return None,
    })
}

/// Normalise a film title for cross-source matching.
/// Lowercase, strip diacritics-ish punctuation, drop trailing "(Hindi)" markers,
/// collapse whitespace. Deterministic and reversible enough for a key.
pub fn norm_title(title: &str) -> String {
    static PAREN: OnceLock<Regex> = OnceLock::new();
    static NONALNUM: OnceLock<Regex> = OnceLock::new();
    let paren = PAREN.get_or_init(|| Regex::new(r"(?i)\s*\((hindi|dubbed|re-?release)\)\s*$").unwrap());
    let nonalnum = NONALNUM.get_or_init(|| Regex::new(r"[^a-z0-9]+").unwrap());
    static TRAIL: OnceLock<Regex> = OnceLock::new();
    let trail = TRAIL.get_or_init(|| Regex::new(r"(\s*\*|\s*\[\w+\]|\s*\(est\.?\))+$").unwrap());
    let t = trail.replace(title, "");
    let t = paren.replace(&t, "");
    let t = fold_diacritics(&t).to_lowercase();
    // Dots and apostrophes join rather than split ("M.B.B.S." -> "mbbs", "Let's" -> "lets").
    let t: String = t.chars().filter(|c| !matches!(c, '.' | '\'')).collect();
    let t = t.replace('&', " and ");
    let t = nonalnum.replace_all(&t, " ");
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Map common accented Latin letters to ASCII so "Brahmāstra" and "Brahmastra" match.
pub fn fold_diacritics(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ā' | 'á' | 'à' | 'â' | 'ä' | 'ã' | 'Ā' | 'Á' | 'À' | 'Â' | 'Ä' => 'a',
            'ē' | 'é' | 'è' | 'ê' | 'ë' | 'Ē' | 'É' | 'È' | 'Ê' => 'e',
            'ī' | 'í' | 'ì' | 'î' | 'ï' | 'Ī' | 'Í' => 'i',
            'ō' | 'ó' | 'ò' | 'ô' | 'ö' | 'Ō' | 'Ó' => 'o',
            'ū' | 'ú' | 'ù' | 'û' | 'ü' | 'Ū' | 'Ú' => 'u',
            'ñ' | 'ṇ' | 'ṅ' => 'n',
            'ś' | 'ṣ' | 'š' => 's',
            'ṭ' => 't',
            'ḍ' => 'd',
            '’' | '‘' => '\'',
            '–' | '—' => '-',
            other => other,
        })
        .collect()
}

/// Levenshtein edit distance over chars.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur.push((prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Extract a 4-digit year from a date string like "26 Aug 2005" or "06-Aug-1994".
pub fn year_from_date(s: &str) -> Option<u16> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(19[3-9]\d|20[0-2]\d)").unwrap());
    re.captures(s)?.get(1)?.as_str().parse().ok()
}

/// Round to a fixed number of decimals so CSV output is byte-stable.
pub fn round(v: f64, decimals: i32) -> f64 {
    let f = 10f64.powi(decimals);
    (v * f).round() / f
}

/// Median of a non-empty slice (copied, sorted). Returns None for empty input.
pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indian_ints() {
        assert_eq!(parse_indian_int("7,39,62,000").unwrap(), 73_962_000.0);
        assert_eq!(parse_indian_int("44,72,00,000").unwrap(), 447_200_000.0);
        assert_eq!(parse_indian_int(" 162,97,00,000 ").unwrap(), 1_629_700_000.0);
    }

    #[test]
    fn rupees() {
        assert_eq!(parse_rupees_to_crore("₹1,108.70 crore"), Some(1108.70));
        assert_eq!(parse_rupees_to_crore("₹55 lakh"), Some(0.55));
        assert_eq!(parse_rupees_to_crore("₹4 crore (est.)[12]"), Some(4.0));
        assert_eq!(parse_rupees_to_crore("n/a"), None);
    }

    #[test]
    fn titles() {
        assert_eq!(norm_title("Hum Aapke Hain Koun..!"), "hum aapke hain koun");
        assert_eq!(norm_title("Pushpa 2 (Hindi)"), "pushpa 2");
        assert_eq!(norm_title("Gadar - Ek Prem Katha"), "gadar ek prem katha");
        assert_eq!(norm_title("Dilwale Dulhania Le Jayenge"), norm_title("DILWALE  DULHANIA LE JAYENGE"));
        assert_eq!(norm_title("Munnabhai M.B.B.S."), norm_title("Munnabhai MBBS"));
        assert_eq!(norm_title("Hanuman Ansh *"), "hanuman ansh");
        assert_eq!(norm_title("Deewaar - Let's Bring Our Heroes Home"), norm_title("Deewaar - Lets bring our heroes home"));
    }

    #[test]
    fn diacritics_and_distance() {
        assert_eq!(norm_title("Brahmāstra: Part One – Shiva"), norm_title("Brahmastra - Part One: Shiva"));
        assert_eq!(levenshtein("rattan", "ratan"), 1);
        assert_eq!(levenshtein("judaai", "judwaa"), 2);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn years_and_median() {
        assert_eq!(year_from_date("26 Aug 2005"), Some(2005));
        assert_eq!(year_from_date("06-Aug-1994"), Some(1994));
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), Some(2.5));
        assert_eq!(median(&[]), None);
    }
}
