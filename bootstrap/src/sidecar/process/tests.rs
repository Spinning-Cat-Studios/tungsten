//! Protocol parsing and serialization tests for the sidecar process layer.

use super::protocol::{Request, Response, SuggestionItem};

// ═══════════════════════════════════════════════════════════════════════
// Request parsing tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_start_session() {
    let json = r#"{"v":1,"type":"start_session","error":"SIGSEGV in constructor"}"#;
    let req: Request = serde_json::from_str(json).unwrap();
    match req {
        Request::StartSession { error, file } => {
            assert_eq!(error, "SIGSEGV in constructor");
            assert!(file.is_none());
        }
        _ => panic!("expected StartSession"),
    }
}

#[test]
fn test_parse_start_session_with_file() {
    let json = r#"{"v":1,"type":"start_session","error":"crash","file":"test.tg"}"#;
    let req: Request = serde_json::from_str(json).unwrap();
    match req {
        Request::StartSession { error, file } => {
            assert_eq!(error, "crash");
            assert_eq!(file.as_deref(), Some("test.tg"));
        }
        _ => panic!("expected StartSession"),
    }
}

#[test]
fn test_parse_suggest() {
    let json = r#"{"v":1,"type":"suggest","error":"type mismatch"}"#;
    let req: Request = serde_json::from_str(json).unwrap();
    assert!(matches!(req, Request::Suggest { .. }));
}

#[test]
fn test_parse_report_outcome() {
    let json = r#"{"v":1,"type":"report_outcome","command":"check-fold","helped":true}"#;
    let req: Request = serde_json::from_str(json).unwrap();
    match req {
        Request::ReportOutcome { command, helped } => {
            assert_eq!(command, "check-fold");
            assert!(helped);
        }
        _ => panic!("expected ReportOutcome"),
    }
}

#[test]
fn test_parse_end_session() {
    let json = r#"{"v":1,"type":"end_session"}"#;
    let req: Request = serde_json::from_str(json).unwrap();
    assert!(matches!(req, Request::EndSession { .. }));
}

#[test]
fn test_parse_shutdown() {
    let json = r#"{"v":1,"type":"shutdown"}"#;
    let req: Request = serde_json::from_str(json).unwrap();
    assert!(matches!(req, Request::Shutdown));
}

#[test]
fn test_parse_unknown_type_fails() {
    let json = r#"{"v":1,"type":"unknown_command"}"#;
    let result = serde_json::from_str::<Request>(json);
    assert!(result.is_err());
}

#[test]
fn test_parse_ignores_unknown_fields() {
    let json = r#"{"v":1,"type":"suggest","error":"test","extra_field":42}"#;
    let req: Request = serde_json::from_str(json).unwrap();
    assert!(matches!(req, Request::Suggest { .. }));
}

// ═══════════════════════════════════════════════════════════════════════
// Response serialization tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_response_serialization_session_started() {
    let resp = Response::session_started("abc-123".to_string());
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"type\":\"session_started\""));
    assert!(json.contains("\"session_id\":\"abc-123\""));
    assert!(json.contains("\"v\":1"));
}

#[test]
fn test_response_serialization_suggestions() {
    let resp = Response::suggestions(vec![SuggestionItem {
        command: "check-fold".to_string(),
        cost: 3,
        relevance: 0.9,
        reason: "test".to_string(),
    }]);
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"type\":\"suggestions\""));
    assert!(json.contains("\"check-fold\""));
}

#[test]
fn test_response_serialization_error() {
    let resp = Response::error("something went wrong");
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"type\":\"error\""));
    assert!(json.contains("something went wrong"));
}
