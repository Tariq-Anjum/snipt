//! Writes the debug fixtures used by the Android debug-only MMS entry point
//! (docs/ANDROID-NOTES.md S15):
//!   cargo run -p sms-mms --features fixture --example make_fixture -- <out-dir>
//! -> notification.pdu (M-Notification.ind), retrieve.pdu (M-Retrieve.conf
//!    with SMIL + UTF-8 text containing an OTP + a 96x64 PNG).

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&dir)?;
    let from = "+919812345678/TYPE=PLMN";
    let n = sms_mms::fixture::notification_ind(
        "SNIPT-FIX-1",
        from,
        "http://mmsc.invalid/snipt/1",
        172_800,
    );
    let r = sms_mms::fixture::retrieve_conf(
        "SNIPT-FIX-1",
        from,
        "Your OTP is 482913. Photo from the trip attached.",
        &sms_mms::fixture::png(96, 64),
    );
    for (name, bytes) in [("notification.pdu", &n), ("retrieve.pdu", &r)] {
        sms_mms::parse(bytes).expect("fixture parses");
        std::fs::write(dir.join(name), bytes)?;
        println!("{} {} bytes", dir.join(name).display(), bytes.len());
    }
    Ok(())
}
