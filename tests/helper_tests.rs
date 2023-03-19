#[cfg(all(feature = "std", feature = "common"))]
mod helper_tests {
    use mavlink::{common::MavMessage, Message};

    #[test]
    fn test_get_default_message_from_id() {
        let message_name = "PING";
        let meta = MavMessage::meta_from_name(message_name);
        let meta = meta.unwrap();
        assert!(meta.id == 4, "Invalid id for message name: PING");
        let message = &meta.default;
        if !matches!(message, MavMessage::PING(_)) {
            unreachable!("Invalid message type.")
        }
        assert!(
            message.meta().name == message_name,
            "Message name does not match"
        );
    }
}
