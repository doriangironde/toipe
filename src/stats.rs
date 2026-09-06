use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct TestRecord {
    pub ts: u64,
    pub wpm: f64,
    pub accuracy: f64,
    pub duration_ms: u64,
    pub words: usize,
    pub chars: usize,
    pub errors: usize,
    pub punct: bool,
}

pub fn history_path() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".local/share")
        });
    base.join("toipe/history")
}

pub fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn fmt_line(r: &TestRecord) -> String {
    format!(
        "{},{:.3},{:.3},{},{},{},{},{}",
        r.ts, r.wpm, r.accuracy, r.duration_ms, r.words, r.chars, r.errors, r.punct as u8
    )
}

fn parse_line(line: &str) -> Option<TestRecord> {
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() != 8 {
        return None;
    }
    Some(TestRecord {
        ts: parts[0].parse().ok()?,
        wpm: parts[1].parse().ok()?,
        accuracy: parts[2].parse().ok()?,
        duration_ms: parts[3].parse().ok()?,
        words: parts[4].parse().ok()?,
        chars: parts[5].parse().ok()?,
        errors: parts[6].parse().ok()?,
        punct: parts[7].parse::<u8>().ok()? != 0,
    })
}

pub fn append_record(r: &TestRecord) -> std::io::Result<()> {
    let path = history_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{}", fmt_line(r))
}

pub fn load() -> Vec<TestRecord> {
    fs::read_to_string(history_path())
        .map(|s| s.lines().filter_map(parse_line).collect())
        .unwrap_or_default()
}

pub fn best_wpm(records: &[TestRecord]) -> Option<f64> {
    records.iter().map(|r| r.wpm).reduce(f64::max)
}

pub fn print_stats(records: &[TestRecord]) {
    if records.is_empty() {
        println!("No tests recorded yet.");
        return;
    }
    let n = records.len();
    let avg_wpm = records.iter().map(|r| r.wpm).sum::<f64>() / n as f64;
    let avg_acc = records.iter().map(|r| r.accuracy).sum::<f64>() / n as f64;
    let best = best_wpm(records).unwrap_or(0.0);
    let best_acc = records.iter().map(|r| r.accuracy).fold(0.0f64, f64::max);
    println!("tests: {}", n);
    println!("best: {:.1} wpm at {:.1}% accuracy", best, best_acc * 100.0);
    println!(
        "avg: {:.1} wpm at {:.1}% accuracy",
        avg_wpm,
        avg_acc * 100.0
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(wpm: f64, acc: f64) -> TestRecord {
        TestRecord {
            ts: 1234,
            wpm,
            accuracy: acc,
            duration_ms: 1000,
            words: 20,
            chars: 100,
            errors: 2,
            punct: false,
        }
    }

    #[test]
    fn roundtrip() {
        for r in [rec(84.25, 0.984), rec(0.0, 0.5)] {
            let parsed = parse_line(&fmt_line(&r)).unwrap();
            assert_eq!(parsed.ts, r.ts);
            assert!((parsed.wpm - r.wpm).abs() < 1e-6);
            assert!((parsed.accuracy - r.accuracy).abs() < 1e-6);
            assert_eq!(parsed.duration_ms, r.duration_ms);
            assert_eq!(parsed.words, r.words);
            assert_eq!(parsed.chars, r.chars);
            assert_eq!(parsed.errors, r.errors);
            assert_eq!(parsed.punct, r.punct);
        }
    }

    #[test]
    fn bad_lines_skipped() {
        assert!(parse_line("not,a,record").is_none());
        assert!(parse_line("").is_none());
        assert!(parse_line("1,2,3,4,5,6,7").is_none());
    }

    #[test]
    fn best_wpm_works() {
        let rs = [rec(80.0, 0.9), rec(95.0, 0.98), rec(70.0, 0.85)];
        assert_eq!(best_wpm(&rs), Some(95.0));
        assert_eq!(best_wpm(&[]), None);
    }
}
