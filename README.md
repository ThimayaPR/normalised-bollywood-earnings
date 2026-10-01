# Normalised Bollywood Earnings — era-normalised top-grossing Hindi films in India

A deterministic Rust pipeline that ranks Hindi films released in India from 1940 to 2026
by **box office normalised for ticket price** (estimated tickets sold × the 2025 ticket price),
with secondary lenses: raw footfalls, per-capita footfalls, nett at 2025 CPI, and nett relative
to the year's median top-10 film. Average income per person (World Bank GDP per capita) is
carried alongside to show ticket affordability over time. Output is a CSV plus a single-file interactive HTML report.

No machine learning, no LLM calls, no randomness. Every number is a function of the cached
source snapshot in `data/raw/` and the hand-curated CSVs in `data/manual/` (each row of
which cites its source URL).

**Report:** https://thimayapr.github.io/normalised-bollywood-earnings/output/report.html

## Quick start

```sh
cargo run --release -- fetch    # download sources into data/raw (cache-first, ~110 requests)
cargo run --release -- build    # offline: parse, merge, normalise, write output/, run checks
cargo run --release -- report   # render output/report.html
cargo test
qa/run.sh                       # headless-Chrome checks on the rendered report (needs Google Chrome)
```

`build` never touches the network. Running it twice on the same `data/raw/` produces
byte-identical `output/*.csv` and `output/report.html`. Commit `data/raw/` to freeze a
snapshot; re-run `fetch` after deleting a cached file to refresh that source.

## Outputs

| File | Contents |
|---|---|
| `output/hindi_films_normalised.csv` | one row per film: nett, footfalls, bands, every lens, every rank, provenance |
| `output/atp_series.csv` | per year: Hindi nett average ticket price, CPI, population, with a method label for each |
| `output/checks.csv` | sanity checks against published figures; `build` exits non-zero if any fails (use `--no-strict` to override) |
| `output/possible_duplicates.csv` | same-year title pairs within edit distance 2 that were not merged, for alias curation |
| `output/report.html` | interactive report as a standalone page (lens switcher, confidence floor, era filter, charts, appendix) |
| `output/report.fragment.html` | the same report without the document skeleton, for hosts that supply their own |
| `data/interim/*.csv` | one tidy table per source after parsing, plus `films_merged.csv` |

## Method

**Money basis.** Every figure is domestic India *nett* (post entertainment tax / GST).
Gross is never used: pre-2017 state entertainment tax made gross up to 2× nett, post-2019 GST
makes it about 1.18×.

**Footfalls.** `footfalls = nett_cr / atp_year` where `atp_year` is the Hindi nett average
ticket price in rupees (nett in crore rupees ÷ rupees per ticket = crore tickets).
Sourced footfalls (Box Office India, BOI articles, Bollywood Hungama) take precedence over
derived ones.

**Confidence tiers and bands.**

| Tier | Meaning | Band |
|---|---|---|
| A | footfalls published by a trade source | ±10% |
| B | derived from tracked nett (1994+) and the ATP series | ±15% |
| C | derived from reconstructed pre-1994 nett | ±25% (1975–93), ±40% (pre-1975) |

**Other lenses.**

- `footfalls_per_1000 = footfalls × 1e7 / population_year × 1000`
- `nett_cpi_2025_cr = nett × CPI_2025 / CPI_year`
- `normalised_boxoffice_2025_cr = footfalls × ATP_2025` (headline; ranks identically to footfalls)
- `rel_year_median = nett / median(nett of the *other* films in that year's top 10)`; null if the
  year has fewer than 5 films with nett. The film is removed from its own denominator so a
  mega-hit cannot compress its own score.

**Ranks** are computed over non-dubbed films only, descending, ties broken by nett then title.
Hindi-dubbed versions of South Indian films are kept in the dataset with `is_dubbed = true`
and null ranks.

**Ticket price series** (`output/atp_series.csv`, `atp_method` column):

| Years | Method |
|---|---|
| 1994–2017 | Box Office India yearly header (`boi_header`) |
| 2018–2020 | Σ nett ÷ Σ footfalls over films BOI tracks both for (`derived_boi_matched_nNN`) |
| 2022, 2023, 2025 | Ormax Hindi gross ATP ÷ 1.18 (`ormax_nett`) |
| 2021, 2024 | log-linear between neighbours (`interpolated`); 2026 carries 2025 (`carry_forward`) |
| 1940–1993 | log-linear between anchors in `data/manual/atp_anchors.csv` and the 1994 BOI value (`anchor`, `anchor_interpolated`) |

CPI: BIS long series 1953–2025, pre-1953 from `data/manual/cpi_pre1953.csv`. Population:
UN WPP via Our World in Data 1950–2023, census anchors before, 5-year CAGR after. Income:
World Bank GDP per capita in current rupees 1960–2025, 5-year CAGR after; `atp_series.csv`
carries `ticket_pct_daily_income` = ticket price ÷ (income ÷ 365) × 100.

## Sources and precedence

Nett: BOI year pages (1994–2024) → Wikipedia domestic-net tables (2024–26, dubbed table) →
BOI articles → keepalivebollywood decade tables (old BOI figures, pre-1994) → Wikipedia
top-film-by-year (1940–2026).

Footfalls: BOI year pages → BOI articles (`data/manual/boi_articles.csv`) → Bollywood Hungama.

Wikipedia's ticket-sales table is attached as `footfalls_consensus_cr` for comparison only.

## Known limitations

- No official admissions tracking exists in India; all footfalls are trade estimates.
- Box Office India's structured database stops in mid-2024. 2024–26 relies on Wikipedia nett,
  BOI articles and Hungama footfalls.
- Pre-1994 nett figures are retrospective reconstructions. The 1980s are the weakest decade:
  published claims (Coolie 7 cr, Maine Pyar Kiya 8 cr footfalls) imply a flat ₹2 ticket for the
  whole decade, inconsistent with BOI's ₹11 in 1994, so they are shown but not used.
- The 1957, 1960 and 1975 ticket-price anchors are themselves derived from the consensus footfalls
  of Mother India, Mughal-e-Azam and Sholay, so those three films' footfalls (and ranks 1–3) are
  assumed by construction, not measured. The checks on them are labelled `anchor_film_reproduced`.
- Dhurandhar 2 (2026) pairs Wikipedia nett (₹1,108 cr) with a BOI-article footfall figure (4 cr),
  implying a ₹277 ticket against the carried-forward series value of ₹182; both are kept as sourced.
- Not normalised: screen count and seat capacity, piracy, OTT windows, release competition.

## Layout

```
src/main.rs            CLI: fetch | build | report | all
src/fetch.rs           cache-first HTTP with manifest (sha256 per file)
src/sources/*.rs       one parser per source, each with unit tests on real HTML fragments
src/merge.rs           title normalisation, aliases, fuzzy same-year matching, precedence
src/atp.rs             CPI / population / ticket-price series with method labels
src/normalise.rs       the lenses, tiers, bands, ranks, tie flags
src/checks.rs          sanity checks against published figures
src/report.rs          fills templates/report.html with JSON
qa/                    headless-browser harness for the report (lenses, filters, finder, a11y)
data/manual/*.csv      hand-curated inputs, one source URL per row
```
