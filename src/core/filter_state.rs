use crate::filter::FilterExpr;
use fancy_regex::Regex;

#[derive(Clone)]
pub struct FilterState {
    pub hide_regex: Option<Regex>,
    pub filter_expr: Option<FilterExpr>,
    pub highlight_expr: Option<FilterExpr>,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            hide_regex: None,
            filter_expr: None,
            highlight_expr: None,
        }
    }
}

impl FilterState {
    /// Apply hide_regex to content, removing matched portions.
    /// If regex has capture groups, only those groups are removed.
    /// Otherwise, the entire match is removed.
    pub fn apply_hide(&self, content: &str) -> Result<String, String> {
        let re = match &self.hide_regex {
            Some(re) => re,
            None => return Ok(content.to_string()),
        };

        let mut ranges_to_remove: Vec<(usize, usize)> = Vec::new();
        let mut search_start = 0;

        while search_start < content.len() {
            let hay = &content[search_start..];
            match re.captures(hay) {
                Ok(Some(caps)) => {
                    let full_match = caps.get(0).unwrap();
                    if caps.len() > 1 {
                        for i in 1..caps.len() {
                            if let Some(group) = caps.get(i) {
                                let abs_start = search_start + group.start();
                                let abs_end = search_start + group.end();
                                ranges_to_remove.push((abs_start, abs_end));
                            }
                        }
                    } else {
                        let abs_start = search_start + full_match.start();
                        let abs_end = search_start + full_match.end();
                        ranges_to_remove.push((abs_start, abs_end));
                    }
                    search_start += full_match.end().max(1);
                }
                Ok(None) => break,
                Err(e) => return Err(e.to_string()),
            }
        }

        if ranges_to_remove.is_empty() {
            return Ok(content.to_string());
        }

        ranges_to_remove.sort_by_key(|r| r.0);
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for range in ranges_to_remove {
            if let Some(last) = merged.last_mut() {
                if range.0 <= last.1 {
                    last.1 = last.1.max(range.1);
                    continue;
                }
            }
            merged.push(range);
        }

        let mut result = String::new();
        let mut pos = 0;
        for (start, end) in merged {
            if start > pos && start <= content.len() {
                result.push_str(&content[pos..start]);
            }
            pos = end.min(content.len());
        }
        if pos < content.len() {
            result.push_str(&content[pos..]);
        }
        Ok(result)
    }

    pub fn matches_filter(&self, content: &str) -> bool {
        match &self.filter_expr {
            Some(expr) => expr.matches(content),
            None => true,
        }
    }
}
