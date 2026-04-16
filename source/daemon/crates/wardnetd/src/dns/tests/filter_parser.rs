use hickory_proto::rr::RecordType;

use crate::dns::filter_parser::{ParseError, ParsedRule, parse_line};

fn parse_ok(line: &str) -> ParsedRule {
    parse_line(line)
        .unwrap_or_else(|e| panic!("parse `{line}` failed: {e}"))
        .unwrap_or_else(|| panic!("parse `{line}` returned None"))
}

fn parse_skip(line: &str) {
    match parse_line(line) {
        Ok(None) => {}
        Ok(Some(r)) => panic!("expected skip but got rule: {r:?}"),
        Err(e) => panic!("expected skip but got error: {e}"),
    }
}

fn parse_err(line: &str) -> ParseError {
    parse_line(line)
        .err()
        .unwrap_or_else(|| panic!("expected parse error on `{line}`"))
}

// ----- Comments / blanks -----

#[test]
fn blank_line_is_skipped() {
    parse_skip("");
    parse_skip("   ");
}

#[test]
fn hash_comment_is_skipped() {
    parse_skip("# this is a comment");
    parse_skip("   #leading whitespace");
}

#[test]
fn bang_comment_is_skipped() {
    parse_skip("! adblock-style comment");
}

#[test]
fn adblock_header_is_skipped() {
    parse_skip("[Adblock Plus 2.0]");
}

// ----- Adblock-style -----

#[test]
fn adblock_style_domain_block() {
    let rule = parse_ok("||ads.example.com^");
    match rule {
        ParsedRule::DomainBlock { domain, allow, .. } => {
            assert_eq!(domain, "ads.example.com");
            assert!(!allow);
        }
        other => panic!("wrong rule variant: {other:?}"),
    }
}

#[test]
fn adblock_style_without_trailing_separator() {
    let rule = parse_ok("||ads.example.com");
    if let ParsedRule::DomainBlock { domain, .. } = rule {
        assert_eq!(domain, "ads.example.com");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn adblock_style_wildcard_becomes_pattern() {
    let rule = parse_ok("||*.example.com^");
    match rule {
        ParsedRule::Pattern { regex, .. } => {
            assert!(regex.is_match("foo.example.com"));
            assert!(regex.is_match("bar.baz.example.com"));
            assert!(!regex.is_match("other.com"));
        }
        other => panic!("wrong rule variant: {other:?}"),
    }
}

#[test]
fn adblock_style_wildcard_in_middle() {
    let rule = parse_ok("||analytics.*^");
    if let ParsedRule::Pattern { regex, .. } = rule {
        assert!(regex.is_match("analytics.com"));
        assert!(regex.is_match("analytics.example.co.uk"));
        assert!(!regex.is_match("tracking.com"));
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn adblock_exception_sets_allow() {
    let rule = parse_ok("@@||safe.example.com^");
    if let ParsedRule::DomainBlock { domain, allow, .. } = rule {
        assert_eq!(domain, "safe.example.com");
        assert!(allow);
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn adblock_domain_is_lowercased() {
    let rule = parse_ok("||ADS.EXAMPLE.COM^");
    if let ParsedRule::DomainBlock { domain, .. } = rule {
        assert_eq!(domain, "ads.example.com");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn adblock_empty_after_pipes_errors() {
    let err = parse_err("||");
    assert!(matches!(err, ParseError::Empty));
}

// ----- Hosts-file -----

#[test]
fn hosts_file_zero_block() {
    let rule = parse_ok("0.0.0.0 ads.example.com");
    if let ParsedRule::DomainBlock { domain, allow, .. } = rule {
        assert_eq!(domain, "ads.example.com");
        assert!(!allow);
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn hosts_file_loopback_block() {
    let rule = parse_ok("127.0.0.1 tracker.example.com");
    if let ParsedRule::DomainBlock { domain, .. } = rule {
        assert_eq!(domain, "tracker.example.com");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn hosts_file_ipv6_zero_block() {
    let rule = parse_ok("::  ads.example.com");
    if let ParsedRule::DomainBlock { domain, .. } = rule {
        assert_eq!(domain, "ads.example.com");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn hosts_file_localhost_is_skipped() {
    parse_skip("127.0.0.1 localhost");
    parse_skip("127.0.0.1 localhost.localdomain");
    parse_skip("0.0.0.0 broadcasthost");
}

#[test]
fn hosts_file_rejects_non_sentinel_ip() {
    let err = parse_err("192.168.1.1 example.com");
    assert!(matches!(err, ParseError::InvalidHostsIp(_)));
}

#[test]
fn hosts_file_last_token_is_domain() {
    // Hosts lines can have aliases; we take the last token.
    let rule = parse_ok("0.0.0.0 ads.example.com alias.example");
    if let ParsedRule::DomainBlock { domain, .. } = rule {
        assert_eq!(domain, "alias.example");
    } else {
        panic!("wrong variant");
    }
}

// ----- Bare domain -----

#[test]
fn bare_domain_block() {
    let rule = parse_ok("ads.example.com");
    if let ParsedRule::DomainBlock { domain, .. } = rule {
        assert_eq!(domain, "ads.example.com");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn bare_domain_trailing_dot_trimmed() {
    let rule = parse_ok("ads.example.com.");
    if let ParsedRule::DomainBlock { domain, .. } = rule {
        assert_eq!(domain, "ads.example.com");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn single_token_without_dot_is_rejected() {
    let err = parse_err("notadomain");
    assert!(matches!(err, ParseError::Unrecognized(_)));
}

#[test]
fn invalid_characters_rejected() {
    let err = parse_err("bad$domain");
    assert!(matches!(
        err,
        ParseError::Unrecognized(_) | ParseError::InvalidDomain(_)
    ));
}

// ----- Regex -----

#[test]
fn regex_rule() {
    let rule = parse_ok(r"/tracker\d+\.example\.com/");
    match rule {
        ParsedRule::Regex { regex, .. } => {
            assert!(regex.is_match("tracker1.example.com"));
            assert!(regex.is_match("tracker42.example.com"));
            assert!(!regex.is_match("example.com"));
        }
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn regex_with_modifiers() {
    let rule = parse_ok(r"/foo/$dnstype=AAAA");
    if let ParsedRule::Regex {
        modifiers, regex, ..
    } = rule
    {
        assert_eq!(
            modifiers.dns_types.as_deref(),
            Some(&[RecordType::AAAA][..])
        );
        assert!(regex.is_match("foo"));
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn regex_invalid_pattern_errors() {
    let err = parse_err(r"/[unclosed/");
    assert!(matches!(err, ParseError::InvalidRegex { .. }));
}

#[test]
fn regex_empty_pattern_errors() {
    let err = parse_err("//");
    assert!(matches!(err, ParseError::Empty));
}

#[test]
fn regex_exception_sets_allow() {
    let rule = parse_ok("@@/foo/");
    if let ParsedRule::Regex { allow, .. } = rule {
        assert!(allow);
    } else {
        panic!("wrong variant");
    }
}

// ----- Modifiers -----

#[test]
fn dnstype_single() {
    let rule = parse_ok("||analytics.example.com^$dnstype=AAAA");
    assert_eq!(
        rule.modifiers().dns_types.as_deref(),
        Some(&[RecordType::AAAA][..])
    );
}

#[test]
fn dnstype_multiple_pipe_separated() {
    let rule = parse_ok("||analytics.example.com^$dnstype=A|AAAA");
    let types = rule.modifiers().dns_types.as_ref().unwrap();
    assert_eq!(types, &vec![RecordType::A, RecordType::AAAA]);
}

#[test]
fn dnstype_case_insensitive() {
    let rule = parse_ok("||x.com^$dnstype=aaaa");
    assert_eq!(
        rule.modifiers().dns_types.as_deref(),
        Some(&[RecordType::AAAA][..])
    );
}

#[test]
fn dnstype_invalid_value_errors() {
    let err = parse_err("||x.com^$dnstype=FOOBAR");
    assert!(matches!(err, ParseError::InvalidModifierValue { .. }));
}

#[test]
fn dnsrewrite_ipv4() {
    let rule = parse_ok("||internal.corp^$dnsrewrite=192.168.1.50");
    assert_eq!(
        rule.modifiers().rewrite_to.map(|ip| ip.to_string()),
        Some("192.168.1.50".to_owned())
    );
}

#[test]
fn dnsrewrite_ipv6() {
    let rule = parse_ok("||internal.corp^$dnsrewrite=fe80::1");
    assert!(rule.modifiers().rewrite_to.is_some());
}

#[test]
fn dnsrewrite_invalid_ip_errors() {
    let err = parse_err("||x.com^$dnsrewrite=notanip");
    assert!(matches!(err, ParseError::InvalidRewrite(_)));
}

#[test]
fn client_cidr() {
    let rule = parse_ok("||ads.com^$client=192.168.1.0/24");
    let clients = rule.modifiers().clients.as_ref().unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].to_string(), "192.168.1.0/24");
}

#[test]
fn client_bare_ip_becomes_host_network() {
    let rule = parse_ok("||ads.com^$client=192.168.1.100");
    let clients = rule.modifiers().clients.as_ref().unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].prefix(), 32);
}

#[test]
fn client_multiple_pipe_separated() {
    let rule = parse_ok("||ads.com^$client=192.168.1.0/24|10.0.0.5");
    let clients = rule.modifiers().clients.as_ref().unwrap();
    assert_eq!(clients.len(), 2);
}

#[test]
fn client_invalid_errors() {
    let err = parse_err("||x.com^$client=notanip");
    assert!(matches!(err, ParseError::InvalidClient(_)));
}

#[test]
fn important_flag() {
    let rule = parse_ok("||ads.com^$important");
    assert!(rule.modifiers().important);
}

#[test]
fn important_with_value_errors() {
    let err = parse_err("||x.com^$important=yes");
    assert!(matches!(err, ParseError::InvalidModifierValue { .. }));
}

#[test]
fn multiple_modifiers_comma_separated() {
    let rule = parse_ok("||ads.com^$dnstype=AAAA,important,client=10.0.0.0/8");
    let m = rule.modifiers();
    assert!(m.important);
    assert_eq!(m.dns_types.as_deref(), Some(&[RecordType::AAAA][..]));
    assert_eq!(m.clients.as_ref().unwrap().len(), 1);
}

#[test]
fn unknown_modifier_errors() {
    let err = parse_err("||x.com^$whatever");
    assert!(matches!(err, ParseError::UnknownModifier(_)));
}

#[test]
fn modifier_without_value_errors() {
    let err = parse_err("||x.com^$dnstype");
    assert!(matches!(err, ParseError::MissingModifierValue(_)));
}

// ----- Misc -----

#[test]
fn rule_modifiers_is_empty_default() {
    let rule = parse_ok("||ads.example.com^");
    assert!(rule.modifiers().is_empty());
}

#[test]
fn rule_modifiers_not_empty_with_important() {
    let rule = parse_ok("||ads.example.com^$important");
    assert!(!rule.modifiers().is_empty());
}

#[test]
fn is_allow_returns_exception_flag() {
    let block = parse_ok("||a.com^");
    let allow = parse_ok("@@||a.com^");
    assert!(!block.is_allow());
    assert!(allow.is_allow());
}
