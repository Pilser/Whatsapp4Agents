use waproto::whatsapp as wa;

pub fn make_text_message(text: &str) -> wa::Message {
    wa::Message {
        conversation: Some(text.to_string()),
        ..Default::default()
    }
}

pub fn make_reply_message(text: &str, reply_to_id: &str) -> wa::Message {
    let context = wa::ContextInfo {
        stanza_id: Some(reply_to_id.to_string()),
        ..Default::default()
    };
    wa::Message {
        extended_text_message: Some(Box::new(wa::message::ExtendedTextMessage {
            text: Some(text.to_string()),
            context_info: Some(Box::new(context)),
            ..Default::default()
        })),
        ..Default::default()
    }
}

pub fn make_reaction_message(target_id: &str, emoji: &str) -> wa::Message {
    let key = wa::MessageKey {
        id: Some(target_id.to_string()),
        ..Default::default()
    };
    wa::Message {
        reaction_message: Some(Box::new(wa::message::ReactionMessage {
            key: Some(key),
            text: Some(emoji.to_string()),
            ..Default::default()
        })),
        ..Default::default()
    }
}

pub fn make_edit_message(original_id: &str, new_text: &str) -> wa::Message {
    let context = wa::ContextInfo {
        stanza_id: Some(original_id.to_string()),
        ..Default::default()
    };
    wa::Message {
        extended_text_message: Some(Box::new(wa::message::ExtendedTextMessage {
            text: Some(new_text.to_string()),
            context_info: Some(Box::new(context)),
            ..Default::default()
        })),
        ..Default::default()
    }
}

pub fn make_delete_message(target_id: &str) -> wa::Message {
    let key = wa::MessageKey {
        id: Some(target_id.to_string()),
        ..Default::default()
    };
    let protocol_msg = wa::message::ProtocolMessage {
        key: Some(key),
        r#type: Some(wa::message::protocol_message::Type::Revoke as i32),
        ..Default::default()
    };
    wa::Message {
        protocol_message: Some(Box::new(protocol_msg)),
        ..Default::default()
    }
}
