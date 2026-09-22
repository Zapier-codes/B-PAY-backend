//! Renders the legal/reference wording of a Bill of Exchange as plain text,
//! keyed by jurisdiction. This is a starting point, not a substitute for
//! legal review — wording, stamp-duty notices, and required disclosures
//! vary by country and should be confirmed with local counsel before use.
//! For `Jurisdiction::Decentralized`, "legal review" instead means: get
//! sign-off from whoever owns the platform terms/smart contract that this
//! instrument's enforceability actually rests on.

use crate::model::{BillOfExchange, Tenor};

fn tenor_text(tenor: &Tenor) -> String {
    match tenor {
        Tenor::OnDemand => "at sight".to_string(),
        Tenor::FixedDate(d) => format!("on {}", d),
        Tenor::DaysAfterSight(n) => format!("at {} days after sight", n),
        Tenor::DaysAfterDate(n) => format!("at {} days after date", n),
    }
}

fn format_amount(minor_units: i64, currency: &str) -> String {
    format!("{}.{:02} {}", minor_units / 100, minor_units % 100, currency)
}

/// Renders the instrument body text. `boe` should already be `Executed`
/// (i.e. `content_hash` populated) for this to represent a finalized
/// instrument rather than a draft preview.
pub fn render_text(boe: &BillOfExchange) -> String {
    let amount = format_amount(boe.amount_minor_units, &boe.currency);
    let tenor = tenor_text(&boe.tenor);

    let jurisdiction_notice = match boe.jurisdiction {
        crate::model::Jurisdiction::International => {
            "Drawn under the UNCITRAL Convention on International Bills of Exchange and \
             International Promissory Notes (1988)."
                .to_string()
        }
        crate::model::Jurisdiction::UnitedKingdom => {
            "Drawn under the Bills of Exchange Act 1882.".to_string()
        }
        crate::model::Jurisdiction::UnitedStates => "Drawn under UCC Article 3.".to_string(),
        crate::model::Jurisdiction::Nigeria => {
            "Drawn under the Bills of Exchange Act (Nigeria).".to_string()
        }
        crate::model::Jurisdiction::India => {
            "Drawn under the Negotiable Instruments Act, 1881 (India).".to_string()
        }
        crate::model::Jurisdiction::Decentralized => match &boe.crypto_signal {
            Some(signal) => format!(
                "No national jurisdiction claimed. Verified via cryptographic signal \
                 (Ed25519 public key {}), anchored on the counterparties' smart-contract \
                 platform rather than a national court system. Enforceability is defined \
                 by that platform's terms, not by this text.",
                signal.public_key_hex
            ),
            None => "No national jurisdiction claimed. Cryptographic signal not yet \
                     computed — this instrument is not executed and has no verifiable \
                     anchor yet."
                .to_string(),
        },
    };

    format!(
        "BILL OF EXCHANGE\n\
         Reference: {instrument_id}\n\
         {jurisdiction_notice}\n\n\
         {date} at {place}\n\n\
         Pay {tenor} to the order of {payee_name}, {payee_address},\n\
         the sum of {amount}.\n\n\
         To: {drawee_name}, {drawee_address}\n\
         From (Drawer): {drawer_name}, {drawer_address}\n\n\
         Content hash: {content_hash}\n",
        instrument_id = boe.instrument_id,
        jurisdiction_notice = jurisdiction_notice,
        date = boe.date_of_issue,
        place = boe.place_of_issue,
        tenor = tenor,
        payee_name = boe.payee.full_name,
        payee_address = boe.payee.address,
        amount = amount,
        drawee_name = boe.drawee.full_name,
        drawee_address = boe.drawee.address,
        drawer_name = boe.drawer.full_name,
        drawer_address = boe.drawer.address,
        content_hash = boe.content_hash.as_deref().unwrap_or("<not yet executed>"),
    )
}
