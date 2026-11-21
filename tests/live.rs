//! What the live filter reads from PipeWire without a server.

use ::voice_filter::live::{metadata_node_name, smart_target};

#[test]
fn reads_the_node_name_of_a_default_device() {
    assert_eq!(
        metadata_node_name(r#"{"name":"alsa_input.usb-mic"}"#).as_deref(),
        Some("alsa_input.usb-mic")
    );
    // as `pw-metadata` shows it, with spaces and an escaped quote
    assert_eq!(
        metadata_node_name(r#"{ "name" : "a \"b\" c" }"#).as_deref(),
        Some(r#"a "b" c"#)
    );
    assert_eq!(metadata_node_name("{}"), None);
    assert_eq!(metadata_node_name(r#"{"name":"unterminated"#), None);
    assert_eq!(metadata_node_name(r#"{"name":5}"#), None);
}

#[test]
fn targets_the_smart_filter_by_node_name() {
    assert_eq!(
        smart_target("alsa_input.usb-mic"),
        r#"{"node.name":"alsa_input.usb-mic"}"#
    );
    assert_eq!(smart_target(r#"a"b\c"#), r#"{"node.name":"a\"b\\c"}"#);
}
