//! difflib.get_close_matches, as CPython computes it (SequenceMatcher's ratio, autojunk included).

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
        // no junk is given, so only popular elements stop the match above; extend over them
        let popular = |c: &char| !self.b2j.contains_key(c);
        while besti > alo && bestj > blo && popular(&self.b[bestj - 1]) && self.a[besti - 1] == self.b[bestj - 1] {
            (besti, bestj, bestsize) = (besti - 1, bestj - 1, bestsize + 1);
        }
        while besti + bestsize < ahi
            && bestj + bestsize < bhi
            && popular(&self.b[bestj + bestsize])
            && self.a[besti + bestsize] == self.b[bestj + bestsize]
        {
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

fn ratio(matches: usize, length: usize) -> f64 {
    if length == 0 {
        1.0
    } else {
        2.0 * matches as f64 / length as f64
    }
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
        if ratio(a.len().min(b.len()), length) < cutoff {
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
        if ratio(quick, length) < cutoff {
            continue;
        }
        let score = ratio(Matcher::new(&a, &b).matched(), length);
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
        let names: Vec<String> = ["floor_start", "floor_end", "map_start", "start"].iter().map(|w| w.to_string()).collect();
        assert_eq!(get_close_matches("flor_start", &names, 3, 0.6), ["floor_start", "start", "map_start"]);
    }
}
