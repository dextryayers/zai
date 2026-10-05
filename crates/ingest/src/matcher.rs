use anyhow::{Context, Result};

/// Pattern matcher for fs.search. Fixed substring default, regex opt in.
pub enum Matcher {
    All,
    Fixed {
        needle: String,
        lowered: Option<String>,
    },
    Regex(regex::Regex),
}

impl Matcher {
    pub fn new(pattern: Option<String>, is_regex: bool, ignore_case: bool) -> Result<Self> {
        match pattern {
            None => Ok(Matcher::All),
            Some(p) if p.is_empty() => Ok(Matcher::All),
            Some(p) => {
                if is_regex {
                    let re = regex::RegexBuilder::new(&p)
                        .case_insensitive(ignore_case)
                        .build()
                        .with_context(|| format!("invalid regex: {p}"))?;
                    Ok(Matcher::Regex(re))
                } else if ignore_case {
                    Ok(Matcher::Fixed {
                        needle: p.clone(),
                        lowered: Some(p.to_lowercase()),
                    })
                } else {
                    Ok(Matcher::Fixed {
                        needle: p,
                        lowered: None,
                    })
                }
            }
        }
    }

    pub fn is_match(&self, haystack: &str) -> bool {
        match self {
            Matcher::All => true,
            Matcher::Fixed { needle, lowered } => {
                if let Some(n) = lowered {
                    haystack.to_lowercase().contains(n)
                } else {
                    haystack.contains(needle.as_str())
                }
            }
            Matcher::Regex(re) => re.is_match(haystack),
        }
    }
}
