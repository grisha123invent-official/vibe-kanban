use axum::{extract::ws::Message, response::sse::Event};
use json_patch::Patch;
use serde::{Deserialize, Serialize};

pub const EV_STDOUT: &str = "stdout";
pub const EV_STDERR: &str = "stderr";
pub const EV_JSON_PATCH: &str = "json_patch";
pub const EV_SESSION_ID: &str = "session_id";
pub const EV_MESSAGE_ID: &str = "message_id";
pub const EV_READY: &str = "ready";
pub const EV_FINISHED: &str = "finished";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LogMsg {
    Stdout(String),
    Stderr(String),
    JsonPatch(Patch),
    SessionId(String),
    MessageId(String),
    Ready,
    Finished,
}

impl LogMsg {
    pub fn name(&self) -> &'static str {
        match self {
            LogMsg::Stdout(_) => EV_STDOUT,
            LogMsg::Stderr(_) => EV_STDERR,
            LogMsg::JsonPatch(_) => EV_JSON_PATCH,
            LogMsg::SessionId(_) => EV_SESSION_ID,
            LogMsg::MessageId(_) => EV_MESSAGE_ID,
            LogMsg::Ready => EV_READY,
            LogMsg::Finished => EV_FINISHED,
        }
    }

    pub fn to_sse_event(&self) -> Event {
        match self {
            LogMsg::Stdout(s) => Event::default().event(EV_STDOUT).data(s.clone()),
            LogMsg::Stderr(s) => Event::default().event(EV_STDERR).data(s.clone()),
            LogMsg::JsonPatch(patch) => {
                let data = serde_json::to_string(patch).unwrap_or_else(|_| "[]".to_string());
                Event::default().event(EV_JSON_PATCH).data(data)
            }
            LogMsg::SessionId(s) => Event::default().event(EV_SESSION_ID).data(s.clone()),
            LogMsg::MessageId(s) => Event::default().event(EV_MESSAGE_ID).data(s.clone()),
            LogMsg::Ready => Event::default().event(EV_READY).data(""),
            LogMsg::Finished => Event::default().event(EV_FINISHED).data(""),
        }
    }

    /// Convert LogMsg to WebSocket message with fallback error handling
    ///
    /// This method mirrors the behavior of the original logmsg_to_ws function
    /// but with better error handling than unwrap().
    pub fn to_ws_message_unchecked(&self) -> Message {
        // Finished and Ready use special JSON formats for frontend compatibility
        let json = match self {
            LogMsg::Ready => r#"{"Ready":true}"#.to_string(),
            LogMsg::Finished => r#"{"finished":true}"#.to_string(),
            _ => serde_json::to_string(self)
                .unwrap_or_else(|_| r#"{"error":"serialization_failed"}"#.to_string()),
        };

        Message::Text(json.into())
    }

    /// Rough size accounting for your byte‑budgeted history.
    pub fn approx_bytes(&self) -> usize {
        const OVERHEAD: usize = 8;
        match self {
            LogMsg::Stdout(s) => EV_STDOUT.len() + s.len() + OVERHEAD,
            LogMsg::Stderr(s) => EV_STDERR.len() + s.len() + OVERHEAD,
            LogMsg::JsonPatch(patch) => {
                let json_len = serde_json::to_string(patch).map(|s| s.len()).unwrap_or(2);
                EV_JSON_PATCH.len() + json_len + OVERHEAD
            }
            LogMsg::SessionId(s) => EV_SESSION_ID.len() + s.len() + OVERHEAD,
            LogMsg::MessageId(s) => EV_MESSAGE_ID.len() + s.len() + OVERHEAD,
            LogMsg::Ready => EV_READY.len() + OVERHEAD,
            LogMsg::Finished => EV_FINISHED.len() + OVERHEAD,
        }
    }

    /// Returns true if this variant is expected in a raw stdout/stderr stream.
    /// Used to filter unexpected variants in WebSocket raw-log streaming.
    pub fn is_raw_stream_variant(&self) -> bool {
        matches!(self, LogMsg::Stdout(_) | LogMsg::Stderr(_) | LogMsg::Finished)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── name() ──────────────────────────────────────────────────────────────

    #[test]
    fn name_returns_correct_event_string_for_every_variant() {
        assert_eq!(LogMsg::Stdout("x".into()).name(), EV_STDOUT);
        assert_eq!(LogMsg::Stderr("x".into()).name(), EV_STDERR);
        assert_eq!(LogMsg::JsonPatch(Default::default()).name(), EV_JSON_PATCH);
        assert_eq!(LogMsg::SessionId("s".into()).name(), EV_SESSION_ID);
        assert_eq!(LogMsg::MessageId("m".into()).name(), EV_MESSAGE_ID);
        assert_eq!(LogMsg::Ready.name(), EV_READY);
        assert_eq!(LogMsg::Finished.name(), EV_FINISHED);
    }

    // ── to_ws_message_unchecked() ────────────────────────────────────────────

    #[test]
    fn ws_message_finished_uses_lowercase_finished_key() {
        let msg = LogMsg::Finished.to_ws_message_unchecked();
        let Message::Text(text) = msg else {
            panic!("expected Text message");
        };
        let v: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
        assert_eq!(v["finished"], true, "finished key must be lowercase bool");
    }

    #[test]
    fn ws_message_ready_uses_ready_key() {
        let msg = LogMsg::Ready.to_ws_message_unchecked();
        let Message::Text(text) = msg else {
            panic!("expected Text message");
        };
        let v: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
        assert_eq!(v["Ready"], true, "Ready key must be present");
    }

    #[test]
    fn ws_message_stdout_roundtrips_content() {
        let payload = "hello world";
        let msg = LogMsg::Stdout(payload.into()).to_ws_message_unchecked();
        let Message::Text(text) = msg else {
            panic!("expected Text message");
        };
        let v: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
        assert_eq!(v["Stdout"], payload);
    }

    #[test]
    fn ws_message_stderr_roundtrips_content() {
        let payload = "error output";
        let msg = LogMsg::Stderr(payload.into()).to_ws_message_unchecked();
        let Message::Text(text) = msg else {
            panic!("expected Text message");
        };
        let v: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
        assert_eq!(v["Stderr"], payload);
    }

    // ── approx_bytes() ───────────────────────────────────────────────────────

    #[test]
    fn approx_bytes_accounts_for_content_length() {
        let short = LogMsg::Stdout("hi".into()).approx_bytes();
        let long = LogMsg::Stdout("hello world this is longer".into()).approx_bytes();
        assert!(
            long > short,
            "longer content should produce larger approx_bytes"
        );
    }

    #[test]
    fn approx_bytes_finished_and_ready_are_small() {
        // Finished/Ready carry no payload so they should be small
        assert!(LogMsg::Finished.approx_bytes() < 32);
        assert!(LogMsg::Ready.approx_bytes() < 32);
    }

    // ── is_raw_stream_variant() ───────────────────────────────────────────────

    #[test]
    fn raw_stream_variants_are_stdout_stderr_finished() {
        assert!(LogMsg::Stdout("".into()).is_raw_stream_variant());
        assert!(LogMsg::Stderr("".into()).is_raw_stream_variant());
        assert!(LogMsg::Finished.is_raw_stream_variant());
    }

    #[test]
    fn non_raw_stream_variants_are_rejected() {
        assert!(!LogMsg::JsonPatch(Default::default()).is_raw_stream_variant());
        assert!(!LogMsg::SessionId("s".into()).is_raw_stream_variant());
        assert!(!LogMsg::MessageId("m".into()).is_raw_stream_variant());
        assert!(!LogMsg::Ready.is_raw_stream_variant());
    }
}
