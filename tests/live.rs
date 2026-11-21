//! What the live filter reads from PipeWire without a server.

use ::voice_filter::live::metadata_node_name;

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
