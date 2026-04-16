use crate::dns::blocklist_downloader::parse_blocklist_body;

#[test]
fn parse_hosts_file_format() {
    let body = "\
0.0.0.0 ads.example.com
127.0.0.1 tracker.example.com
0.0.0.0 banner.example.org
";
    let domains = parse_blocklist_body(body);
    assert_eq!(domains.len(), 3);
    assert!(domains.contains(&"ads.example.com".to_owned()));
    assert!(domains.contains(&"tracker.example.com".to_owned()));
    assert!(domains.contains(&"banner.example.org".to_owned()));
}

#[test]
fn parse_domain_list() {
    let body = "\
ads.example.com
tracker.example.com
malware.example.org
";
    let domains = parse_blocklist_body(body);
    assert_eq!(domains.len(), 3);
    assert!(domains.contains(&"ads.example.com".to_owned()));
    assert!(domains.contains(&"tracker.example.com".to_owned()));
    assert!(domains.contains(&"malware.example.org".to_owned()));
}

#[test]
fn parse_mixed_with_comments() {
    let body = "\
# This is a hosts file
! Another comment style
[Adblock Plus 2.0]

0.0.0.0 ads.example.com
# inline comment
tracker.example.com

0.0.0.0 banner.example.org
";
    let domains = parse_blocklist_body(body);
    assert_eq!(domains.len(), 3);
    assert!(domains.contains(&"ads.example.com".to_owned()));
    assert!(domains.contains(&"tracker.example.com".to_owned()));
    assert!(domains.contains(&"banner.example.org".to_owned()));
}

#[test]
fn parse_deduplicates() {
    let body = "\
0.0.0.0 ads.example.com
0.0.0.0 ads.example.com
ads.example.com
";
    let domains = parse_blocklist_body(body);
    assert_eq!(domains.len(), 1);
    assert!(domains.contains(&"ads.example.com".to_owned()));
}

#[test]
fn parse_ignores_complex_rules() {
    let body = "\
||ads.example.com^
||analytics.*^
/tracking\\.js/
||blocked.com^$dnstype=A
@@||allowed.example.com^
simple.example.com
";
    let domains = parse_blocklist_body(body);
    // Only plain domain blocks without modifiers and allow=false are included.
    // ||ads.example.com^ => DomainBlock, no modifiers, allow=false => included
    // ||analytics.*^ => Pattern (wildcard) => excluded
    // /tracking\.js/ => Regex => excluded
    // ||blocked.com^$dnstype=A => DomainBlock with modifiers => excluded
    // @@||allowed.example.com^ => DomainBlock, allow=true => excluded
    // simple.example.com => DomainBlock, no modifiers, allow=false => included
    assert_eq!(domains.len(), 2);
    assert!(domains.contains(&"ads.example.com".to_owned()));
    assert!(domains.contains(&"simple.example.com".to_owned()));
}

#[test]
fn parse_empty_body() {
    let domains = parse_blocklist_body("");
    assert!(domains.is_empty());
}
