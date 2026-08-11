use wardnetd_data::vendor_catalog::{lookup_mdns_service, mdns_service_types};

use crate::mdns_observer::browse_query;

#[test]
fn browse_query_appends_the_local_domain() {
    // The catalog stores bare service types; the mDNS browse query wants them
    // fully qualified. Building the daemon and binding multicast is out of scope
    // for a unit test (see `mdns_advertiser` tests), so the query construction
    // is what we lock down here.
    assert_eq!(browse_query("_govee._tcp"), "_govee._tcp.local.");
}

#[test]
fn every_browsed_type_round_trips_to_a_vendor() {
    // The observer browses exactly the catalogued service types and records the
    // fully-qualified `ty_domain` it gets back. That value must resolve to a
    // vendor through the same catalog, or the observation would be stored yet
    // name nobody — pin the whole loop end to end.
    let types = mdns_service_types();
    assert!(!types.is_empty(), "nothing to browse for");
    for ty in types {
        let query = browse_query(ty);
        assert!(
            lookup_mdns_service(&query).is_some(),
            "browse query {query} does not resolve back to a vendor",
        );
    }
}
