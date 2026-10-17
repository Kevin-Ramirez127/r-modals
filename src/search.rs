use crate::ModalOption;

/// Search / filter configuration.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Show the search field. If false, all options are listed unfiltered.
    pub enabled: bool,
    /// Placeholder (hint) text inside the search field.
    pub placeholder: String,
    /// `true` = `Foo` does NOT match `foo`. Defaults to false.
    pub case_sensitive: bool,
    /// Auto-focus the search field on open. Defaults to true.
    pub autofocus: bool,
    /// `true` = subsequence fuzzy match (ranked best-first) instead of
    /// substring match. Defaults to false (previous behavior).
    pub fuzzy: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            placeholder: "Type to filter…".to_string(),
            case_sensitive: false,
            autofocus: true,
            fuzzy: false,
        }
    }
}

/// Fuzzy subsequence score: every char of `query` must appear in `text`
/// in order. Higher is better: consecutive runs, word-start hits, and
/// early/short candidates score more. Returns `None` on no match.
pub fn fuzzy_score(text: &str, query: &str, case_sensitive: bool) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let (t, q): (Vec<char>, Vec<char>) = if case_sensitive {
        (text.chars().collect(), query.chars().collect())
    } else {
        (
            text.to_lowercase().chars().collect(),
            query.to_lowercase().chars().collect(),
        )
    };
    let mut ti = 0usize;
    let mut qi = 0usize;
    let mut score: i32 = 0;
    let mut run: i32 = 0;
    let mut first_at: Option<usize> = None;
    while ti < t.len() && qi < q.len() {
        if t[ti] == q[qi] {
            if first_at.is_none() {
                first_at = Some(ti);
            }
            run += 1;
            // Consecutive matches compound; word-start matches get a bonus.
            score += 10 + run * 5;
            if ti == 0
                || t[ti - 1] == ' '
                || t[ti - 1] == '-'
                || t[ti - 1] == '_'
                || t[ti - 1] == '/'
            {
                score += 8;
            }
            qi += 1;
        } else {
            run = 0;
        }
        ti += 1;
    }
    if qi != q.len() {
        return None;
    }
    let first = first_at.unwrap_or(0) as i32;
    let len_penalty = t.len() as i32;
    Some(score - first * 2 - len_penalty / 8)
}

/// Pure, GUI-free filtering used by the app and unit tests.
///
/// Substring match over label + id + description.
/// Returns indices into `options`, preserving order.
/// A parent option also matches when any of its descendants match (so it
/// can act as a gateway to drill into).
///
/// The query is lowered once here; each option is checked field-by-field
/// with early exit (label hit skips id/description), so matching options
/// cost a single short `to_lowercase` instead of a full haystack build.
pub fn filter_options(options: &[ModalOption], query: &str, case_sensitive: bool) -> Vec<usize> {
    filter_options_with(
        options,
        query,
        &SearchConfig {
            case_sensitive,
            ..SearchConfig::default()
        },
    )
}

/// Config-aware filtering: substring (default, order-preserving) or fuzzy
/// (`fuzzy: true`, ranked best-first, stable on ties).
pub fn filter_options_with(options: &[ModalOption], query: &str, cfg: &SearchConfig) -> Vec<usize> {
    if query.is_empty() {
        return (0..options.len()).collect();
    }
    if cfg.fuzzy {
        let mut scored: Vec<(usize, i32)> = options
            .iter()
            .enumerate()
            .filter_map(|(i, o)| o.fuzzy_score(query, cfg.case_sensitive).map(|s| (i, s)))
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.into_iter().map(|(i, _)| i).collect()
    } else if cfg.case_sensitive {
        options
            .iter()
            .enumerate()
            .filter(|(_, o)| o.matches_query(query, "", true))
            .map(|(i, _)| i)
            .collect()
    } else {
        let q = query.to_lowercase();
        options
            .iter()
            .enumerate()
            .filter(|(_, o)| o.matches_query(query, &q, false))
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests;
