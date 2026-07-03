# Message Format: waos → ZeroClaw

When a WhatsApp message arrives, waos builds a JSON payload and sends it as the `prompt` field in a JSON-RPC `session/prompt` call to zeroclaw. Below are the exact JSON bodies for each scenario.

---

## Direct Message

```json
{
  "channel": "waos.whatsapp-web",
  "account": "my_account",
  "chat_id": "521234567890@s.whatsapp.net",
  "customer": "jose1",
  "customer_jid": "521234567890@s.whatsapp.net",
  "message": "hi",
  "message_id": "ABGG123...",
  "message_type": "direct",
  "timestamp": 1719876543,
  "reply_to": null
}
```

---

## Direct Message with Reply

When the user is replying to a previous message sent by the agent:

```json
{
  "channel": "waos.whatsapp-web",
  "account": "my_account",
  "chat_id": "521234567890@s.whatsapp.net",
  "customer": "jose1",
  "customer_jid": "521234567890@s.whatsapp.net",
  "message": "yes please",
  "message_id": "ABGG456...",
  "message_type": "direct",
  "timestamp": 1719876550,
  "reply_to": {
    "message_id": "ABGG789...",
    "message": "I can help you with that",
    "sender": "agent"
  }
}
```

---

## Group @Mention

When the agent is @mentioned in a group:

```json
{
  "channel": "waos.whatsapp-web",
  "account": "my_account",
  "group": "Family Group (1234567890-12345@g.us)",
  "group_jid": "1234567890-12345@g.us",
  "customer": "maria",
  "customer_jid": "521234567891@s.whatsapp.net",
  "message": "@agent what do you think?",
  "message_id": "ABGG111...",
  "message_type": "group_mention",
  "timestamp": 1719876543,
  "mentions": ["521234567891@s.whatsapp.net"],
  "reply_to": null
}
```

---

## Group Reply

When a user quotes/replies to the agent's message in a group:

```json
{
  "channel": "waos.whatsapp-web",
  "account": "my_account",
  "group": "Family Group (1234567890-12345@g.us)",
  "group_jid": "1234567890-12345@g.us",
  "customer": "maria",
  "customer_jid": "521234567891@s.whatsapp.net",
  "message": "that makes sense",
  "message_id": "ABGG222...",
  "message_type": "group_reply",
  "timestamp": 1719876543,
  "mentions": [],
  "reply_to": {
    "message_id": "ABGG333...",
    "message": "Here are the pros and cons...",
    "sender": "agent"
  }
}
```

---

## Field Reference

| Field | Type | Always | Description |
|-------|------|--------|-------------|
| `channel` | string | yes | Always `"waos.whatsapp-web"` |
| `account` | string | yes | WhatsApp account alias |
| `chat_id` | string | DMs | Sender's JID |
| `group` | string | groups | `"GroupName (group_jid)"` |
| `group_jid` | string | groups | Group JID (`xxx@g.us`) |
| `customer` | string | yes | Display name (saved name > push name) |
| `customer_jid` | string | yes | Sender's JID |
| `message` | string | yes | Message text content |
| `message_id` | string | yes | WhatsApp message ID |
| `message_type` | string | yes | `"direct"`, `"group_mention"`, or `"group_reply"` |
| `timestamp` | number | yes | Unix timestamp |
| `mentions` | string[] | groups | JIDs of @mentioned participants |
| `reply_to` | object\|null | yes | `null` or `{ message_id, message, sender }` |

### `reply_to` object

Present when the incoming message is a reply/quote to the agent's earlier message.

| Field | Type | Description |
|-------|------|-------------|
| `message_id` | string | The quoted message's ID |
| `message` | string | The quoted message's text |
| `sender` | string | Always `"agent"` |
