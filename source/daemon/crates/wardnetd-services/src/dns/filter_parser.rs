//! Parser for AdGuard-compatible DNS filter rules.
//!
//! Supports the following input formats:
//! - **Adblock-style**: `||example.com^`, `@@||allow.com^`, with `*` wildcards
//!   and `^` separator. Modifiers attach after `$`.
//! - **Hosts-file**: `0.0.0.0 ads.example.com`, `127.0.0.1 tracker.com`. The
//!   first token must be a sentinel IP (`0.0.0.0`, `127.0.0.1`, `::`, `::1`);
//!   `localhost` lines are silently skipped.
//! - **Domains-only**: bare `example.com` (one per line, must contain a dot).
//! - **Regex**: `/pattern/` or `/pattern/$modifiers`.
//!
//! DNS-specific modifiers supported: `$dnstype=A|AAAA`, `$dnsrewrite=1.2.3.4`,
//! `$client=192.168.1.0/24`, `$important`.
//!
//! Comments (`#`, `!`) and `AdblockPlus` headers (`[Adblock Plus 2.0]`) are
//! recognised and produce `Ok(None)` from [`parse_line`].

use std::net::IpAddr;
use std::str::FromStr;

use hickory_proto::rr::RecordType;
use ipnetwork::IpNetwork;
use regex::Regex;
use thiserror::Error;

/// Parse error for a single rule line.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("rule is empty after trimming")]
    Empty,
    #[error("hosts-file rule must use 0.0.0.0, 127.0.0.1, ::, or ::1 (got `{0}`)")]
    InvalidHostsIp(String),
    #[error("invalid domain: `{0}`")]
    InvalidDomain(String),
    #[error("invalid regex `{pattern}`: {error}")]
    InvalidRegex { pattern: String, error: String },
    #[error("unknown modifier: `{0}`")]
    UnknownModifier(String),
    #[error("invalid `{name}` modifier value: `{value}`")]
    InvalidModifierValue { name: String, value: String },
    #[error("modifier `{0}` requires a value")]
    MissingModifierValue(String),
    #[error("invalid CIDR or IP for $client: `{0}`")]
    InvalidClient(String),
    #[error("invalid rewrite IP for $dnsrewrite: `{0}`")]
    InvalidRewrite(String),
    #[error("unrecognized rule format: `{0}`")]
    Unrecognized(String),
}

/// A successfully parsed filter rule.
#[derive(Debug, Clone)]
pub enum ParsedRule {
    /// Plain domain rule (subdomain match implied). Fast-path eligible when
    /// `modifiers.is_empty()` and `allow == false` (or the build step pushes
    /// it into the allow set when `allow == true`).
    DomainBlock {
        domain: String,
        modifiers: RuleModifiers,
        allow: bool,
    },
    /// Wildcard pattern rule (`||analytics.*^`). Compiled to a case-insensitive
    /// anchored regex.
    Pattern {
        regex: Regex,
        source: String,
        modifiers: RuleModifiers,
        allow: bool,
    },
    /// Regex rule (`/regex/`).
    Regex {
        regex: Regex,
        source: String,
        modifiers: RuleModifiers,
        allow: bool,
    },
}

impl ParsedRule {
    /// Return a reference to the rule's modifiers.
    #[must_use]
    pub fn modifiers(&self) -> &RuleModifiers {
        match self {
            Self::DomainBlock { modifiers, .. }
            | Self::Pattern { modifiers, .. }
            | Self::Regex { modifiers, .. } => modifiers,
        }
    }

    /// Whether this rule is an exception (`@@`).
    #[must_use]
    pub fn is_allow(&self) -> bool {
        match self {
            Self::DomainBlock { allow, .. }
            | Self::Pattern { allow, .. }
            | Self::Regex { allow, .. } => *allow,
        }
    }
}

/// Modifiers on a rule. All optional; absent means "no constraint".
#[derive(Debug, Clone, Default)]
pub struct RuleModifiers {
    /// `$dnstype=A|AAAA` — match only these record types.
    pub dns_types: Option<Vec<RecordType>>,
    /// `$dnsrewrite=1.2.3.4` — synthesize a response with this IP.
    pub rewrite_to: Option<IpAddr>,
    /// `$client=192.168.1.0/24|10.0.0.5` — restrict to these clients.
    pub clients: Option<Vec<IpNetwork>>,
    /// `$important` — overrides `@@` exceptions.
    pub important: bool,
}

impl RuleModifiers {
    /// Whether no modifiers are set (rule is "plain").
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dns_types.is_none()
            && self.rewrite_to.is_none()
            && self.clients.is_none()
            && !self.important
    }
}

/// Parse a single rule line.
///
/// - Returns `Ok(None)` for blank lines, comments (`#`, `!`), `AdblockPlus`
///   headers (`[...]`), and silently-ignored hosts entries (`localhost`).
/// - Returns `Ok(Some(rule))` for any successfully parsed rule.
/// - Returns `Err(...)` for malformed input.
///
/// # Errors
///
/// Returns [`ParseError`] when the line cannot be classified as one of the
/// supported rule formats or contains malformed modifiers.
#[allow(clippy::too_many_lines)] // Linear classifier — splitting hurts readability.
pub fn parse_line(raw: &str) -> Result<Option<ParsedRule>, ParseError> {
    let line = raw.trim();

    if line.is_empty() || line.starts_with('#') || line.starts_with('!') || line.starts_with('[') {
        return Ok(None);
    }

    let (allow, body) = if let Some(rest) = line.strip_prefix("@@") {
        (true, rest)
    } else {
        (false, line)
    };

    // Regex rule: /pattern/ optionally followed by $modifiers.
    if let Some(after_open) = body.strip_prefix('/') {
        if let Some(close_offset) = after_open.find('/') {
            let pattern = &after_open[..close_offset];
            let after = &after_open[close_offset + 1..];
            let modifiers = if let Some(mod_str) = after.strip_prefix('$') {
                parse_modifiers(mod_str)?
            } else if after.is_empty() {
                RuleModifiers::default()
            } else {
                return Err(ParseError::Unrecognized(line.to_owned()));
            };
            if pattern.is_empty() {
                return Err(ParseError::Empty);
            }
            let regex =
                Regex::new(&format!("(?i){pattern}")).map_err(|e| ParseError::InvalidRegex {
                    pattern: pattern.to_owned(),
                    error: e.to_string(),
                })?;
            return Ok(Some(ParsedRule::Regex {
                regex,
                source: line.to_owned(),
                modifiers,
                allow,
            }));
        }
        return Err(ParseError::Unrecognized(line.to_owned()));
    }

    // Adblock-style: ||domain^[$modifiers]
    if let Some(after) = body.strip_prefix("||") {
        let (rule_body, mod_str) = match after.find('$') {
            Some(idx) => (&after[..idx], &after[idx + 1..]),
            None => (after, ""),
        };
        let modifiers = parse_modifiers(mod_str)?;
        let pat = rule_body.strip_suffix('^').unwrap_or(rule_body);
        if pat.is_empty() {
            return Err(ParseError::Empty);
        }
        if pat.contains('*') {
            let regex = compile_wildcard(pat)?;
            return Ok(Some(ParsedRule::Pattern {
                regex,
                source: line.to_owned(),
                modifiers,
                allow,
            }));
        }
        let domain = normalize_domain(pat)?;
        return Ok(Some(ParsedRule::DomainBlock {
            domain,
            modifiers,
            allow,
        }));
    }

    // Wildcard-domain format (e.g. OISD `*.example.com`): semantically
    // equivalent to a bare `example.com` block because our filter's
    // `DomainBlock` already matches the domain itself and all subdomains.
    // Only recognised when the remainder is a plain domain — any extra
    // wildcards force the line down the regex/pattern path instead.
    if let Some(stripped) = body.strip_prefix("*.")
        && !stripped.contains('*')
        && !stripped.contains('/')
        && stripped.contains('.')
        && is_valid_domain(stripped)
    {
        let normalized = normalize_domain(stripped)?;
        return Ok(Some(ParsedRule::DomainBlock {
            domain: normalized,
            modifiers: RuleModifiers::default(),
            allow,
        }));
    }

    // Hosts-file: <ip> <domain> [aliases...]
    let tokens: Vec<&str> = body.split_whitespace().collect();
    if tokens.len() >= 2 {
        match tokens[0] {
            "0.0.0.0" | "127.0.0.1" | "::" | "::1" => {}
            other => return Err(ParseError::InvalidHostsIp(other.to_owned())),
        }
        let domain = tokens[tokens.len() - 1];
        if domain.eq_ignore_ascii_case("localhost")
            || domain.eq_ignore_ascii_case("localhost.localdomain")
            || domain.eq_ignore_ascii_case("broadcasthost")
        {
            return Ok(None);
        }
        let normalized = normalize_domain(domain)?;
        return Ok(Some(ParsedRule::DomainBlock {
            domain: normalized,
            modifiers: RuleModifiers::default(),
            allow,
        }));
    }

    // Bare domain (must contain a dot).
    if tokens.len() == 1 && body.contains('.') && is_valid_domain(body) {
        let normalized = normalize_domain(body)?;
        return Ok(Some(ParsedRule::DomainBlock {
            domain: normalized,
            modifiers: RuleModifiers::default(),
            allow,
        }));
    }

    Err(ParseError::Unrecognized(line.to_owned()))
}

fn parse_modifiers(s: &str) -> Result<RuleModifiers, ParseError> {
    let mut mods = RuleModifiers::default();
    if s.is_empty() {
        return Ok(mods);
    }
    for raw_part in s.split(',') {
        let part = raw_part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, value) = match part.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (part, None),
        };
        match name {
            "important" => {
                if value.is_some() {
                    return Err(ParseError::InvalidModifierValue {
                        name: "important".to_owned(),
                        value: value.unwrap_or("").to_owned(),
                    });
                }
                mods.important = true;
            }
            "dnstype" => {
                let v =
                    value.ok_or_else(|| ParseError::MissingModifierValue("dnstype".to_owned()))?;
                let mut types = Vec::new();
                for t in v.split('|') {
                    let t = t.trim();
                    if t.is_empty() {
                        return Err(ParseError::InvalidModifierValue {
                            name: "dnstype".to_owned(),
                            value: v.to_owned(),
                        });
                    }
                    let rt = RecordType::from_str(&t.to_ascii_uppercase()).map_err(|_| {
                        ParseError::InvalidModifierValue {
                            name: "dnstype".to_owned(),
                            value: t.to_owned(),
                        }
                    })?;
                    types.push(rt);
                }
                mods.dns_types = Some(types);
            }
            "dnsrewrite" => {
                let v = value
                    .ok_or_else(|| ParseError::MissingModifierValue("dnsrewrite".to_owned()))?;
                let ip: IpAddr = v
                    .parse()
                    .map_err(|_| ParseError::InvalidRewrite(v.to_owned()))?;
                mods.rewrite_to = Some(ip);
            }
            "client" => {
                let v =
                    value.ok_or_else(|| ParseError::MissingModifierValue("client".to_owned()))?;
                let mut nets = Vec::new();
                for c in v.split('|') {
                    let c = c.trim();
                    if c.is_empty() {
                        return Err(ParseError::InvalidClient(v.to_owned()));
                    }
                    // Parse as CIDR; bare IPs become /32 or /128.
                    let net: IpNetwork = if c.contains('/') {
                        c.parse()
                            .map_err(|_| ParseError::InvalidClient(c.to_owned()))?
                    } else {
                        let ip: IpAddr = c
                            .parse()
                            .map_err(|_| ParseError::InvalidClient(c.to_owned()))?;
                        IpNetwork::from(ip)
                    };
                    nets.push(net);
                }
                mods.clients = Some(nets);
            }
            other => return Err(ParseError::UnknownModifier(other.to_owned())),
        }
    }
    Ok(mods)
}

fn compile_wildcard(pat: &str) -> Result<Regex, ParseError> {
    let mut s = String::with_capacity(pat.len() + 8);
    s.push_str("(?i)^");
    for ch in pat.chars() {
        if ch == '*' {
            s.push_str(".*");
        } else if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            s.push(ch);
        } else {
            // Escape any other character (unlikely in domain rules).
            for esc in regex::escape(&ch.to_string()).chars() {
                s.push(esc);
            }
        }
    }
    s.push('$');
    Regex::new(&s).map_err(|e| ParseError::InvalidRegex {
        pattern: pat.to_owned(),
        error: e.to_string(),
    })
}

fn normalize_domain(s: &str) -> Result<String, ParseError> {
    let trimmed = s.trim_end_matches('.');
    if !is_valid_domain(trimmed) {
        return Err(ParseError::InvalidDomain(s.to_owned()));
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn is_valid_domain(s: &str) -> bool {
    if s.is_empty() || s.len() > 253 {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}
