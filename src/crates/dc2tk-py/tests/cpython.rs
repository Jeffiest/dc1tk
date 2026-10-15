//! The crate against what CPython gives, line by line of tests/cpython.tsv (tests/cpython.py
//! writes it, and says what each kind of line holds).

use dc2tk_py::{difflib, IntError};

fn float(hex: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(hex, 16).expect("a float's bits in hex"))
}

fn text(json: &str) -> String {
    serde_json::from_str(json).expect("a JSON string")
}

fn texts(json: &str) -> Vec<String> {
    serde_json::from_str(json).expect("a JSON list of strings")
}

/// Floats compare by bits, so -0.0 and the NaNs' bits count.
fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits()
}

/// What the line's check found wrong, or None when the crate agrees with CPython.
fn check(fields: &[&str]) -> Option<String> {
    let wrong = |got: &dyn std::fmt::Debug, expected: &dyn std::fmt::Debug| Some(format!("{got:?} != {expected:?}"));
    match fields {
        ["repr", x, expected] => {
            let got = dc2tk_py::float_repr(float(x));
            (got != *expected).then(|| format!("{got} != {expected}"))
        }
        ["g", x, precision, expected] => {
            let got = dc2tk_py::format_g(float(x), precision.parse().unwrap());
            (got != *expected).then(|| format!("{got} != {expected}"))
        }
        ["f", x, precision, expected] => {
            let got = dc2tk_py::format_f(float(x), precision.parse().unwrap());
            (got != *expected).then(|| format!("{got} != {expected}"))
        }
        ["sum", xs, expected] => {
            let xs: Vec<f64> = xs.split(',').filter(|x| !x.is_empty()).map(float).collect();
            let got = dc2tk_py::sum_floats(xs);
            if same(got, float(expected)) {
                None
            } else {
                wrong(&got, &float(expected))
            }
        }
        ["int", s, expected] => {
            let s = text(s);
            // Python's ints have no limit; int_base holds an i128 and parse_int an i64
            let (base, int) = match expected.parse::<i128>() {
                Ok(value) => (Ok(value), i64::try_from(value).ok()),
                Err(_) if *expected == "ERR" => (Err(IntError::Invalid), None),
                Err(_) => (Err(IntError::TooLarge), None),
            };
            let got = (dc2tk_py::int_base(&s, 0), dc2tk_py::parse_int(&s));
            if got == (base, int) {
                None
            } else {
                wrong(&got, &(base, int))
            }
        }
        ["float", s, expected] => {
            let got = dc2tk_py::parse_float(&text(s));
            let expected = (*expected != "ERR").then(|| float(expected));
            match (got, expected) {
                (Some(got), Some(expected)) if same(got, expected) || (got.is_nan() && expected.is_nan()) => None,
                (None, None) => None,
                _ => wrong(&got, &expected),
            }
        }
        ["split", s, expected] => {
            let s = text(s);
            let got = dc2tk_py::split(&s);
            if got == texts(expected) {
                None
            } else {
                wrong(&got, &texts(expected))
            }
        }
        ["strip", s, expected] => {
            let s = text(s);
            let got = dc2tk_py::strip(&s);
            if got == text(expected) {
                None
            } else {
                wrong(&got, &text(expected))
            }
        }
        ["space", points] => {
            let expected: Vec<u32> = serde_json::from_str(points).unwrap();
            let got: Vec<u32> =
                (0..=char::MAX as u32).filter(|&c| char::from_u32(c).is_some_and(dc2tk_py::is_space)).collect();
            if got == expected {
                None
            } else {
                wrong(&got, &expected)
            }
        }
        ["expandtabs", s, expected] => {
            let got = dc2tk_py::expand_tabs(&text(s));
            if got == text(expected) {
                None
            } else {
                wrong(&got, &text(expected))
            }
        }
        ["ratio", a, b, expected] => {
            let got = difflib::ratio(&text(a), &text(b));
            if same(got, float(expected)) {
                None
            } else {
                wrong(&got, &float(expected))
            }
        }
        ["close", word, possibilities, n, cutoff, expected] => {
            let got = difflib::get_close_matches(&text(word), &texts(possibilities), n.parse().unwrap(), float(cutoff));
            if got == texts(expected) {
                None
            } else {
                wrong(&got, &texts(expected))
            }
        }
        _ => Some("an unknown kind of line".into()),
    }
}

#[test]
fn agrees_with_cpython() {
    let listing = include_str!("cpython.tsv");
    let mut failures = Vec::new();
    let mut lines = 0;
    for line in listing.lines() {
        lines += 1;
        let fields: Vec<&str> = line.split('\t').collect();
        if let Some(problem) = check(&fields) {
            failures.push(format!("{line}\n    {problem}"));
        }
    }
    assert!(lines > 1000, "the listing has only {lines} lines");
    let shown: Vec<&str> = failures.iter().take(20).map(String::as_str).collect();
    assert!(failures.is_empty(), "{} of {lines} lines disagree with CPython:\n{}", failures.len(), shown.join("\n"));
}

/// repr() against a longer listing than cpython.tsv holds: a file of "hex bits<TAB>Python's repr"
/// lines named by $C2_REPR_FILE.
#[test]
#[ignore]
fn reprs_like_python_for_a_listing() {
    let path = std::env::var("C2_REPR_FILE").expect("C2_REPR_FILE names the listing");
    let listing = std::fs::read_to_string(path).unwrap();
    let mut wrong = 0;
    for line in listing.lines() {
        let (bits, expected) = line.split_once('\t').unwrap();
        let got = dc2tk_py::float_repr(float(bits));
        if got != expected {
            wrong += 1;
            if wrong < 10 {
                eprintln!("{bits}: {got} != {expected}");
            }
        }
    }
    assert_eq!(wrong, 0, "{wrong} of {} differ", listing.lines().count());
}
