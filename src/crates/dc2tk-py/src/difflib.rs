//! difflib.get_close_matches and SequenceMatcher.ratio, as CPython computes them (autojunk
//! included, no isjunk).

use std::collections::HashMap;

struct Matcher<'a> {
    a: &'a [char],
    b: &'a [char],
    b2j: HashMap<char, Vec<usize>>,
}

impl<'a> Matcher<'a> {
    fn new(a: &'a [char], b: &'a [char]) -> Matcher<'a> {
        let mut b2j: HashMap<char, Vec<usize>> = HashMap::new();
        for (at, c) in b.iter().enumerate() {
            b2j.entry(*c).or_default().push(at);
        }
        // autojunk: in a b of 200 or more, an element in more than 1% of it (plus one) is popular
        if b.len() >= 200 {
            let most = b.len() / 100 + 1;
            b2j.retain(|_, places| places.len() <= most);
        }
        Matcher { a, b, b2j }
    }

    fn longest(&self, alo: usize, ahi: usize, blo: usize, bhi: usize) -> (usize, usize, usize) {
        let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0);
        let mut j2len: HashMap<usize, usize> = HashMap::new();
        for i in alo..ahi {
            let mut next: HashMap<usize, usize> = HashMap::new();
            for &j in self.b2j.get(&self.a[i]).map(Vec::as_slice).unwrap_or(&[]) {
                if j < blo {
                    continue;
                }
                if j >= bhi {
                    break;
                }
                let k = j.checked_sub(1).and_then(|before| j2len.get(&before)).copied().unwrap_or(0) + 1;
                next.insert(j, k);
                if k > bestsize {
                    (besti, bestj, bestsize) = (i + 1 - k, j + 1 - k, k);
                }
            }
            j2len = next;
        }
        // Popular elements are not in b2j, so the match above stops at them. With no isjunk every
        // element is non-junk, and CPython extends the match over every equal element on each side
        // (popular ones, and the ones beyond them); its second pass, over junk, has nothing to do.
        while besti > alo && bestj > blo && self.a[besti - 1] == self.b[bestj - 1] {
            (besti, bestj, bestsize) = (besti - 1, bestj - 1, bestsize + 1);
        }
        while besti + bestsize < ahi && bestj + bestsize < bhi && self.a[besti + bestsize] == self.b[bestj + bestsize] {
            bestsize += 1;
        }
        (besti, bestj, bestsize)
    }

    fn matched(&self) -> usize {
        let mut total = 0;
        let mut queue = vec![(0, self.a.len(), 0, self.b.len())];
        while let Some((alo, ahi, blo, bhi)) = queue.pop() {
            let (i, j, k) = self.longest(alo, ahi, blo, bhi);
            if k > 0 {
                total += k;
                if alo < i && blo < j {
                    queue.push((alo, i, blo, j));
                }
                if i + k < ahi && j + k < bhi {
                    queue.push((i + k, ahi, j + k, bhi));
                }
            }
        }
        total
    }
}

/// _calculate_ratio: twice the matched elements over both lengths.
fn ratio_of(matches: usize, length: usize) -> f64 {
    if length == 0 {
        1.0
    } else {
        2.0 * matches as f64 / length as f64
    }
}

/// SequenceMatcher(None, a, b).ratio().
pub fn ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    ratio_of(Matcher::new(&a, &b).matched(), a.len() + b.len())
}

/// get_close_matches(word, possibilities, n, cutoff): the best n possibilities scoring at least
/// cutoff, best first (equal scores in reverse order of the text).
pub fn get_close_matches(word: &str, possibilities: &[String], n: usize, cutoff: f64) -> Vec<String> {
    let b: Vec<char> = word.chars().collect();
    let mut counts: HashMap<char, isize> = HashMap::new();
    for c in &b {
        *counts.entry(*c).or_default() += 1;
    }
    let mut found: Vec<(f64, &String)> = Vec::new();
    for possibility in possibilities {
        let a: Vec<char> = possibility.chars().collect();
        let length = a.len() + b.len();
        if ratio_of(a.len().min(b.len()), length) < cutoff {
            continue;
        }
        let mut available = counts.clone();
        let mut quick = 0;
        for c in &a {
            let left = available.entry(*c).or_default();
            if *left > 0 {
                quick += 1;
            }
            *left -= 1;
        }
        if ratio_of(quick, length) < cutoff {
            continue;
        }
        let score = ratio_of(Matcher::new(&a, &b).matched(), length);
        if score >= cutoff {
            found.push((score, possibility));
        }
    }
    found.sort_by(|x, y| y.0.total_cmp(&x.0).then_with(|| y.1.cmp(x.1)));
    found.into_iter().take(n).map(|(_, text)| text.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_python() {
        let words: Vec<String> = ["ape", "apple", "peach", "puppy"].iter().map(|word| word.to_string()).collect();
        assert_eq!(get_close_matches("appel", &words, 3, 0.6), ["apple", "ape"]);
        let names: Vec<String> =
            ["floor_start", "floor_end", "map_start", "start"].iter().map(|w| w.to_string()).collect();
        assert_eq!(get_close_matches("flor_start", &names, 3, 0.6), ["floor_start", "start", "map_start"]);
    }

    #[test]
    fn a_match_extends_past_popular_elements_like_python() {
        // in a b of 201, a and b are popular (more than 3 times) and X, Y, Z are not: the longest
        // match YZ extends over the "ab" before it, then over the X and b before those, so all six
        // of a match; stopping at X left a separate X match, five in all
        let b = format!("XbXabYZ{}", "ab".repeat(97));
        // SequenceMatcher(None, "bXabYZ", b).ratio() in CPython 3.13
        assert_eq!(ratio("bXabYZ", &b), 0.057971014492753624);
        assert_eq!(ratio("", ""), 1.0);
    }
}
