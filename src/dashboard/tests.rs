//! Tests of the dashboard's pure helpers.

use super::escape::script_safe;
use super::view::host_of;

#[test]
fn a_destination_is_host_and_port_only() {
    assert_eq!(
        host_of("http://homeassistant.lan:8123/api/webhook/SECRET-ID"),
        "homeassistant.lan:8123"
    );
    assert_eq!(host_of("https://user:pw@ha.lan/x?token=1"), "ha.lan");
    assert_eq!(host_of("ha.lan/api/webhook/x"), "ha.lan");
}

#[test]
fn json_in_a_script_cannot_close_it() {
    assert!(!script_safe(r#"{"name":"</script><b>"}"#).contains('<'));
}
