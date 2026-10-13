//! MCP clients routinely send integer parameters as JSON strings. The page
//! fields must accept both a JSON number and a numeric string; a non-numeric
//! string must still be rejected, and a misspelled field must fail loudly
//! rather than being dropped.

use flute2_mcp::tools::customers::CustomersList;
use serde_json::json;

#[test]
fn page_fields_accept_string_numerics() {
    let p: CustomersList = serde_json::from_value(json!({ "page_index": "1", "page_size": "50" }))
        .expect("stringified numerics must deserialize");
    assert_eq!(p.page_index, Some(1));
    assert_eq!(p.page_size, Some(50));
}

#[test]
fn page_fields_still_accept_native_numerics() {
    let p: CustomersList = serde_json::from_value(json!({ "page_index": 1, "page_size": 50 }))
        .expect("native numerics must still deserialize");
    assert_eq!(p.page_index, Some(1));
    assert_eq!(p.page_size, Some(50));
}

#[test]
fn page_fields_reject_non_numeric_strings() {
    let r = serde_json::from_value::<CustomersList>(json!({ "page_size": "fifty" }));
    assert!(r.is_err(), "a non-numeric string must still be rejected");
}

#[test]
fn page_size_outside_one_to_a_hundred_is_refused() {
    for bad in [json!(0), json!(101), json!("0")] {
        let r = serde_json::from_value::<CustomersList>(json!({ "page_size": bad }));
        assert!(r.is_err(), "page_size {bad} must be refused");
    }
    for ok in [1, 100] {
        let p: CustomersList = serde_json::from_value(json!({ "page_size": ok })).unwrap();
        assert_eq!(p.page_size, Some(ok));
    }
}

/// A dropped address field can silently remove AVS coverage, so an unknown
/// field is an error rather than a no-op.
#[test]
fn customer_create_rejects_a_misspelled_field() {
    use flute2_mcp::tools::customers::CustomerCreate;
    let r = serde_json::from_value::<CustomerCreate>(json!({
        "first_name": "Ada",
        "last_name": "Lovelace",
        "billing_zip": "80202"
    }));
    assert!(
        r.is_err(),
        "an unknown billing field must be rejected, not silently dropped"
    );
}
