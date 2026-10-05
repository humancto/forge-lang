//! "Did you mean ...?" suggestions by edit distance.

/// Edit distance over chars where insertion, deletion, substitution and
/// swapping two adjacent chars each cost 1 (optimal string alignment), so
/// the common typo `lable` → `label` is one edit.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    d[a.len()][b.len()]
}

/// The candidate closest to `name`, if it is close enough to be a likely
/// typo: a case-insensitive match, or at most one edit per three chars
/// (at most two edits). Ties go to the earliest candidate, so callers list
/// the most likely names (the user's own, innermost first) before builtins.
pub fn closest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let len = name.chars().count();
    let limit = (len / 3).clamp(1, 2);
    let lower = name.to_lowercase();
    let mut best: Option<(usize, &'a str)> = None;
    for candidate in candidates {
        if candidate == name || candidate.starts_with("__") {
            continue;
        }
        let dist = if candidate.to_lowercase() == lower {
            0
        } else {
            edit_distance(name, candidate)
        };
        if dist > limit || dist >= len {
            continue;
        }
        let better = match best {
            None => true,
            Some((d, _)) => dist < d,
        };
        if better {
            best = Some((dist, candidate));
        }
    }
    best.map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_basics() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("same", "same"), 0);
        assert_eq!(edit_distance("lable", "label"), 1);
    }

    #[test]
    fn suggests_close_names_only() {
        let names = ["println", "print", "len", "length_of"];
        assert_eq!(closest("prnt", names), Some("print"));
        assert_eq!(closest("printn", names), Some("println"));
        assert_eq!(closest("lenn", names), Some("len"));
        assert_eq!(closest("zzz", names), None);
        // Short names need an exact-ish match: "x" -> "y" is not a typo.
        assert_eq!(closest("x", ["y"]), None);
    }

    #[test]
    fn case_mismatch_is_suggested() {
        assert_eq!(closest("Username", ["username"]), Some("username"));
    }
}
