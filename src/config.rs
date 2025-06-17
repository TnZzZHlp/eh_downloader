use std::io::BufRead;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::gallery::Gallery;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub cookie: String,

    pub concurrency: i64,

    pub original: bool,

    pub proxy: Option<String>,

    pub input: String,

    pub output: String,
}

impl Config {
    pub fn read_from_file(file_path: &str) -> Result<Self> {
        let file = std::fs::File::open(file_path)?;
        let config: Config = serde_json::from_reader(file)?;
        Ok(config)
    }

    pub fn get_links(&self) -> Result<Vec<Gallery>> {
        let file = std::fs::File::open(&self.input)?;
        let reader = std::io::BufReader::new(file);
        let links: Vec<Gallery> = reader
            .lines()
            .filter_map(|r| r.ok().and_then(|l| Gallery::new(l).ok()))
            .collect();
        Ok(links)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn test_read_from_file() {
        let mut tmp = NamedTempFile::new().expect("Failed to create temporary file");
        let sample = json!({
            "cookie": "test_cookie",
            "concurrency": 4,
            "input": "in",
            "output": "out"
        });
        write!(tmp, "{}", sample).expect("Failed to write to temporary file");

        let config = Config::read_from_file(tmp.path().to_str().unwrap());
        assert!(config.is_ok());
    }

    #[test]
    fn test_config_fields() {
        let mut tmp = NamedTempFile::new().expect("Failed to create temporary file");
        let sample = json!({
            "cookie": "test_cookie",
            "concurrency": 4,
            "input": "in",
            "output": "out"
        });
        write!(tmp, "{}", sample).expect("Failed to write to temporary file");
        let config = Config::read_from_file(tmp.path().to_str().unwrap()).unwrap();
        assert!(!config.cookie.is_empty());
        assert!(config.concurrency > 0);
        assert!(!config.input.is_empty());
        assert!(!config.output.is_empty());

        let mut tmp = NamedTempFile::new().expect("Failed to create temporary file");
        let sample = json!({
            "cookie": "test_cookie",
            "concurrency": 4,
            "output": "out"
        });

        write!(tmp, "{}", sample).expect("Failed to write to temporary file");
        let config = Config::read_from_file(tmp.path().to_str().unwrap());

        assert!(config.is_err(), "Expected error due to missing input field");
        if let Err(e) = config {
            assert!(e.to_string().contains("missing field `input`"));
        }
    }
}
