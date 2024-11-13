use std::str::FromStr;

enum Format {
    Afrf,
}

impl FromStr for Format {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "afrf" => Self::Afrf,
            _ => return Err(format!("invalid format {value:?}")),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Policy {
    None,
    Quarantine,
    Reject,
}

impl FromStr for Policy {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "none" => Self::None,
            "quarantine" => Self::Quarantine,
            "reject" => Self::Reject,
            _ => return Err(format!("invalid policy {value:?}")),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Relaxed,
    Strict,
}

impl From<Mode> for char {
    fn from(value: Mode) -> Self {
        match value {
            Mode::Relaxed => 'r',
            Mode::Strict => 's',
        }
    }
}

impl FromStr for Mode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "r" => Self::Relaxed,
            "s" => Self::Strict,
            _ => return Err(format!("invalid mode {value:?}")),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReportFailure {
    all_pass: bool,
    any_pass: bool,
    dkim: bool,
    spf: bool,
}

impl FromStr for ReportFailure {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut new = Self::default();
        for part in value.split(':') {
            match part.trim() {
                "0" => new.all_pass = true,
                "1" => new.any_pass = true,
                "d" => new.dkim = true,
                "s" => new.spf = true,
                _ => return Err(format!("invalid report failure {value:?}")),
            }
        }

        Ok(new)
    }
}

impl Default for ReportFailure {
    fn default() -> Self {
        Self {
            all_pass: true,
            any_pass: false,
            dkim: false,
            spf: false,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct FeedbackAddress {
    uri: String,
    size: Option<u64>,
}

impl FromStr for FeedbackAddress {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err("empty feedback address {s:?}".to_owned());
        }

        let Some((uri, size)) = s.trim().rsplit_once('!') else {
            return Ok(Self {
                uri: s.to_owned(),
                size: None,
            });
        };

        let size = size.trim();
        if size.is_empty() {
            return Err(format!("empty size in {s:?}"));
        }

        let mut power = 0;
        match size.chars().next_back() {
            Some('k') => power = 10,
            Some('m') => power = 20,
            Some('g') => power = 30,
            Some('t') => power = 40,
            _ => {}
        }

        let size = match power {
            0 => size,
            _ => &size[..size.len() - 1],
        };

        let size = u64::from_str(size).map_err(|_| format!("invalid size in {s:?}"))? << power;
        Ok(Self {
            uri: uri.to_owned(),
            size: Some(size),
        })
    }
}

struct Record {
    align_dkim: Mode,
    align_spf: Mode,
    report_failure: ReportFailure,
    policy: Policy,
    rate: u8,
    format: Format,
    interval: u32,
    aggregate_feedback: Vec<FeedbackAddress>,
    message_failure: Vec<FeedbackAddress>,
    subdomain_policy: Policy,
}

impl FromStr for Record {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut new = Self {
            align_dkim: Mode::Relaxed,
            align_spf: Mode::Relaxed,
            report_failure: ReportFailure::default(),
            policy: Policy::None,
            rate: 100,
            format: Format::Afrf,
            interval: 86400,
            aggregate_feedback: Vec::new(),
            message_failure: Vec::new(),
            subdomain_policy: Policy::None,
        };

        let (mut version, mut policy) = (false, false);
        for part in s.split(';') {
            let Some((key, value)) = part.split_once('=') else {
                return Err(format!("invalid part {part:?}"));
            };

            let (key, value) = (key.trim(), value.trim());
            if !version {
                match (key, value) {
                    ("v", "DMARC1") => {
                        version = true;
                        continue;
                    }
                    _ => return Err(format!("invalid key {key:?}")),
                }
            }

            match key {
                "p" => {
                    new.policy = Policy::from_str(value)?;
                    new.subdomain_policy = new.policy;
                    policy = true;
                }
                "adkim" => new.align_dkim = Mode::from_str(value)?,
                "aspf" => new.align_spf = Mode::from_str(value)?,
                "fo" => new.report_failure = ReportFailure::from_str(value)?,
                "pct" => {
                    new.rate = u8::from_str(value)
                        .map_err(|_| format!("invalid value {value:?} for pct"))?
                }
                "rf" => new.format = Format::from_str(value)?,
                "ri" => {
                    new.interval = u32::from_str(value)
                        .map_err(|_| format!("invalid value {value:?} for ri"))?
                }
                "rua" => {
                    for addr in value.split(',') {
                        new.aggregate_feedback
                            .push(FeedbackAddress::from_str(addr)?);
                    }
                }
                "ruf" => {
                    for addr in value.split(',') {
                        new.message_failure.push(FeedbackAddress::from_str(addr)?);
                    }
                }
                "sp" => new.subdomain_policy = Policy::from_str(value)?,
                _ => return Err(format!("invalid key {key:?}")),
            }
        }

        match policy {
            true => Ok(new),
            false => Err(format!("missing policy in {s:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_b_2_1() {
        // https://www.rfc-editor.org/rfc/rfc7489#appendix-B.2.1
        const B_2_1: &str = "v=DMARC1; p=none; rua=mailto:dmarc-feedback@example.com";
        let record = Record::from_str(B_2_1).unwrap();
        assert_eq!(record.policy, Policy::None);
        assert_eq!(record.rate, 100);
        assert_eq!(
            record.aggregate_feedback[0].uri,
            "mailto:dmarc-feedback@example.com",
        );
        assert_eq!(record.aggregate_feedback[0].size, None);
    }

    #[test]
    fn parse_b_2_2() {
        // https://www.rfc-editor.org/rfc/rfc7489#appendix-B.2.2
        const B_2_2: &str = "v=DMARC1; p=none; rua=mailto:dmarc-feedback@example.com; ruf=mailto:auth-reports@example.com";
        let record = Record::from_str(B_2_2).unwrap();
        assert_eq!(record.policy, Policy::None);
        assert_eq!(
            record.aggregate_feedback[0].uri,
            "mailto:dmarc-feedback@example.com",
        );
        assert_eq!(
            record.message_failure[0].uri,
            "mailto:auth-reports@example.com",
        );
    }

    #[test]
    fn parse_b_2_4() {
        // https://www.rfc-editor.org/rfc/rfc7489#appendix-B.2.4
        const B_2_4: &str = r#"v=DMARC1; p=quarantine;
                       rua=mailto:dmarc-feedback@example.com,
                       mailto:tld-test@thirdparty.example.net!10m;
                       pct=25"#;
        let record = Record::from_str(B_2_4).unwrap();
        assert_eq!(record.policy, Policy::Quarantine);
        assert_eq!(record.rate, 25);
        assert_eq!(
            record.aggregate_feedback[0].uri,
            "mailto:dmarc-feedback@example.com",
        );
        assert_eq!(
            record.aggregate_feedback[1].uri,
            "mailto:tld-test@thirdparty.example.net",
        );
        assert_eq!(record.aggregate_feedback[1].size, Some(10_485_760));
    }
}
