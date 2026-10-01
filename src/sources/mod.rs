pub mod boi;
pub mod hungama;
pub mod keepalive;
pub mod macro_series;
pub mod wiki;
pub mod wiki_footfalls;

use scraper::ElementRef;

/// Collapse an element's text into a single-spaced string.
pub fn text_of(el: ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}
