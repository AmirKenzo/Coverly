use regex::Regex;
use std::{path::Path, sync::OnceLock};

pub fn rating(folder: &Path) -> Option<f32> {
    let rd = std::fs::read_dir(folder).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        let is_nfo = p
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("nfo"))
            .unwrap_or(false);
        if is_nfo {
            if let Ok(s) = std::fs::read_to_string(&p) {
                if let Some(r) = parse(&s) {
                    return Some(r);
                }
            }
        }
    }
    None
}

pub fn parse(s: &str) -> Option<f32> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"<(?:rating|value)(?:\s[^>]*)?>\s*([0-9]+(?:\.[0-9]+)?)\s*</(?:rating|value)>").unwrap()
    });
    let v: f32 = re.captures(s)?.get(1)?.as_str().parse().ok()?;
    let v = if v > 10.0 { v / 10.0 } else { v };
    (v > 0.0).then_some(v)
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_ratings() {
        assert_eq!(super::parse("<movie><rating>8.6</rating></movie>"), Some(8.6));
        assert_eq!(
            super::parse(r#"<ratings><rating name="imdb" max="10"><value>7.9</value></rating></ratings>"#),
            Some(7.9)
        );
        assert_eq!(super::parse("<movie></movie>"), None);
    }
}
