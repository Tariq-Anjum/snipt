use sms_types::*;

/// Serde round-trip must be byte-stable for canonical (types.ts key order) JSON.
fn assert_byte_stable<T>(json: &str)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let v: T = serde_json::from_str(json).expect("deserialise");
    let out = serde_json::to_string(&v).expect("serialise");
    assert_eq!(out, json);
}

#[test]
fn raw_sms_shapes_round_trip() {
    // Shapes of every optional-field combination used by seedData.ts.
    assert_byte_stable::<RawSms>(
        r#"{"id":1001,"threadId":101,"sender":"AX-HDFCBK-S","body":"Sent Rs.450.50","timestampMs":1763600000000,"isKnownContact":false,"isOutgoing":false,"simSlot":0,"isRead":true,"protocol":"SMS"}"#,
    );
    assert_byte_stable::<RawSms>(
        r#"{"id":1009,"threadId":104,"sender":"+919845012345","contactName":"Aarav Mehta","body":"Yes!","timestampMs":1,"isKnownContact":true,"isOutgoing":true,"simSlot":0,"isRead":true,"isStarred":false,"protocol":"RCS","rcsStatus":"read"}"#,
    );
    assert_byte_stable::<RawSms>(
        r#"{"id":1021,"threadId":113,"sender":"INDIGO-RCS","contactName":"IndiGo Airlines (RCS)","body":"BLR → DEL","timestampMs":2,"isKnownContact":false,"isOutgoing":false,"simSlot":1,"isRead":false,"protocol":"RCS","rcsRichCard":{"title":"IndiGo 6E-2041 · BLR → DEL","subtitle":"PNR XK94M2","actionLabel":"Boarding Pass Saved","actionMeta":"Verified RCS Business"}}"#,
    );
}

#[test]
fn sender_kind_tagging() {
    assert_byte_stable::<SenderKind>(
        r#"{"type":"DltHeader","prefix":"VM","entity":"AMAZON","suffix":null}"#,
    );
    assert_byte_stable::<SenderKind>(r#"{"type":"Contact"}"#);
    assert_byte_stable::<SenderKind>(r#"{"type":"Alphanumeric","value":"INDIGO-RCS"}"#);
    assert_byte_stable::<PaymentMethod>(r#""Credit Card""#);
    assert_byte_stable::<BudgetCategoryId>(r#""Food & Dining""#);
    assert_byte_stable::<ClassificationReason>(r#""otp_or_reminder""#);
}
