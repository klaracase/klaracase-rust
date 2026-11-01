//! Checks the citations in a text and prints one line per finding.
//!
//! Run it with a real token: `KLARACASE_API_TOKEN=<token> cargo run --example citation_check`.
//! A check costs at least two credits.
//!
//! A `not_found` verdict means the citation was not found in the corpus. It is not a claim that the
//! citation is fabricated. `verified_corpus` shows that the cited source exists. It does not show
//! that a quote or a legal proposition is correct.

use klaracase::{CreateCitationCheckParams, Error, Klaracase, Verdict};

const TEXT: &str = "Der Bundesgerichtshof hat im Urteil vom 15.11.2001 zum Az. 1 StR 185/01 \
entschieden, dass § 266 StGB eine Vermögensbetreuungspflicht voraussetzt. Die Revision ist daher \
nach § 349 Abs. 2 StPO zu verwerfen.";

#[tokio::main]
async fn main() -> Result<(), Error> {
    let client = Klaracase::from_env()?;

    let run = match client
        .citation_checks()
        .create(CreateCitationCheckParams::from_text(TEXT))
        .await
    {
        Ok(run) => run,
        Err(Error::InsufficientCredits { cost, balance, .. }) => {
            eprintln!("not enough credits: the check costs {cost:?}, the balance is {balance:?}");
            return Ok(());
        }
        Err(other) => return Err(other),
    };

    for finding in &run.findings {
        let note = match finding.verdict.as_str() {
            Verdict::VERIFIED_CORPUS => "exists in the corpus",
            Verdict::NOT_FOUND => "not found in the corpus (this is not a claim of fabrication)",
            Verdict::OUT_OF_COVERAGE => "outside what the corpus covers",
            Verdict::AMBIGUOUS => "more than one source fits",
            Verdict::DISCREPANCY => "the source exists but a detail differs",
            _ => "needs a human look",
        };
        println!(
            "[{}..{}] {} -> {} ({note})",
            finding.start_offset, finding.end_offset, finding.raw_text, finding.verdict
        );
    }
    println!("run {} is {}", run.id, run.status);
    Ok(())
}
