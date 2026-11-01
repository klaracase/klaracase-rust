//! Closed vocabularies.
//!
//! The first group are request vocabularies. They are typed strictly, so a typo in `mode` or
//! `branch` is a compile error and not a 422 after a round trip. The second group are response
//! vocabularies. Responses hold them as [`Open<E>`](crate::Open), so a value added by a later API
//! release stays readable as the server's own string.
//!
//! Every enum is `#[non_exhaustive]`: a later release can add a value.

use crate::open::wire_enum;

/// The error of `FromStr` for the vocabulary enums.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{value}` is not a valid {kind}")]
pub struct UnknownValue {
    kind: &'static str,
    value: String,
}

impl UnknownValue {
    pub(crate) fn new(kind: &'static str, value: &str) -> Self {
        Self {
            kind,
            value: value.to_owned(),
        }
    }
}

wire_enum! {
    /// Retrieval mode.
    Mode {
        /// Requires all query terms; not reranked.
        Keyword => "keyword",
        /// Pure vector search.
        Semantic => "semantic",
        /// Semantic and keyword blended. The server default.
        Hybrid => "hybrid",
    }
}

wire_enum! {
    /// A Gerichtsbarkeit (court jurisdiction). It is not a legal subject area.
    Branch {
        /// Ordinary courts.
        Ordentlich => "ordentlich",
        /// Administrative courts.
        Verwaltung => "verwaltung",
        /// Social courts.
        Sozial => "sozial",
        /// Labour courts.
        Arbeit => "arbeit",
        /// Finance courts.
        Finanz => "finanz",
        /// Constitutional courts.
        Verfassung => "verfassung",
        /// The Bundespatentgericht alone.
        Patent => "patent",
        /// Unknown.
        Unbekannt => "unbekannt",
    }
}

wire_enum! {
    /// The document type of a decision.
    DocumentType {
        /// Urteil.
        Urteil => "Urteil",
        /// Beschluss.
        Beschluss => "Beschluss",
        /// Verfügung.
        Verfuegung => "Verfügung",
        /// Sonstige.
        Sonstige => "Sonstige",
        /// Gerichtsbescheid.
        Gerichtsbescheid => "Gerichtsbescheid",
        /// Schlussanträge.
        Schlussantraege => "Schlussanträge",
    }
}

wire_enum! {
    /// A statute for the `cited_statutes` filter.
    Statute {
        /// Strafgesetzbuch.
        StGB => "StGB",
        /// Strafprozessordnung.
        StPO => "StPO",
        /// Insolvenzordnung.
        InsO => "InsO",
        /// Datenschutz-Grundverordnung.
        DSGVO => "DSGVO",
        /// General Data Protection Regulation.
        GDPR => "GDPR",
        /// Bundesdatenschutzgesetz.
        BDSG => "BDSG",
        /// Baugesetzbuch.
        BauGB => "BauGB",
        /// Bundesbaugesetz.
        BBauG => "BBauG",
        /// Baunutzungsverordnung.
        BauNVO => "BauNVO",
        /// Straßenverkehrsgesetz.
        StVG => "StVG",
        /// Straßenverkehrs-Ordnung.
        StVO => "StVO",
    }
}

wire_enum! {
    /// Which neighbours of a decision to list.
    Direction {
        /// Decisions this one cites.
        Outgoing => "outgoing",
        /// Decisions citing this one.
        Incoming => "incoming",
        /// Both. The server default.
        Both => "both",
    }
}

wire_enum! {
    /// How far to follow citations.
    Depth {
        /// Direct neighbours. The server default.
        One => "1",
        /// Neighbours of neighbours, capped at 50 decisions.
        Two => "2",
    }
}

wire_enum! {
    /// An artifact type for the facet counts.
    FacetType {
        /// Court decisions.
        Decision => "decision",
        /// Statutes.
        Law => "law",
    }
}

wire_enum! {
    /// A filterable field whose values `corpus().facets` lists.
    FacetField {
        /// Court names.
        CourtName => "court_name",
        /// Court branches.
        CourtBranch => "court_branch",
        /// Document types.
        DocumentType => "document_type",
        /// Jurisdictions.
        Jurisdiction => "jurisdiction",
        /// Law types.
        LawType => "law_type",
        /// Norms cited by decisions. Needs the `norm` parameter.
        CitedNorms => "cited_norms",
    }
}

wire_enum! {
    /// A jurisdiction code of a law: German federal and state law, EU law, or international law.
    Jurisdiction {
        /// German law.
        De => "DE",
        /// EU law.
        Eu => "EU",
        /// International law.
        Xi => "XI",
    }
}

wire_enum! {
    /// The verdict on one citation.
    ///
    /// `NotFound` means the citation was not found in the corpus. `VerifiedCorpus` shows that the
    /// cited source exists. It does not show that a quote or a legal proposition is correct.
    Verdict {
        /// The cited decision or statute exists in the corpus.
        VerifiedCorpus => "verified_corpus",
        /// An external source confirmed the citation.
        VerifiedExternal => "verified_external",
        /// Not found in the corpus.
        NotFound => "not_found",
        /// The citation points outside what the corpus covers.
        OutOfCoverage => "out_of_coverage",
        /// More than one source fits.
        Ambiguous => "ambiguous",
        /// The citation cannot be checked.
        Unverifiable => "unverifiable",
        /// The source exists but a detail differs.
        Discrepancy => "discrepancy",
    }
}

wire_enum! {
    /// The state of a citation check run.
    RunStatus {
        /// Queued.
        Pending => "pending",
        /// The input is being gated.
        Gating => "gating",
        /// Citations are being extracted.
        Extracting => "extracting",
        /// Citations are being resolved against the corpus.
        Resolving => "resolving",
        /// External sources are being checked.
        CheckingExternal => "checking_external",
        /// Finished.
        Complete => "complete",
        /// Refused.
        Rejected => "rejected",
        /// Ended in an error.
        Failed => "failed",
    }
}

wire_enum! {
    /// The state of an uploaded document. `OcrPending` is not final.
    DocumentStatus {
        /// The file arrived.
        Uploaded => "uploaded",
        /// The text is being extracted.
        Extracting => "extracting",
        /// Text recognition on scanned pages is pending.
        OcrPending => "ocr_pending",
        /// The text is extracted.
        Ready => "ready",
        /// Extraction failed.
        Failed => "failed",
    }
}

wire_enum! {
    /// Why extraction of an uploaded document failed.
    DocumentErrorCode {
        /// The text is longer than a check takes.
        DocumentTooLong => "document_too_long",
        /// The format is known but not supported yet.
        UnsupportedFormatPending => "unsupported_format_pending",
        /// The file is damaged.
        CorruptedDocument => "corrupted_document",
        /// The archive expands beyond a safe size.
        ZipBombRejected => "zip_bomb_rejected",
        /// The PDF is encrypted.
        EncryptedPdf => "encrypted_pdf",
        /// The PDF holds scans without a text layer.
        ScannedPdf => "scanned_pdf",
        /// The text layer of the PDF is garbled.
        PdfTextGarbled => "pdf_text_garbled",
        /// Extraction failed for another reason.
        ExtractionFailed => "extraction_failed",
    }
}

wire_enum! {
    /// What a reference points at.
    ReferenceType {
        /// A court decision.
        Case => "case",
        /// A statute.
        Statute => "statute",
    }
}

wire_enum! {
    /// How far a citation edge resolved.
    ResolutionStatus {
        /// The other end is in the corpus.
        Resolved => "resolved",
        /// More than one decision fits.
        UnresolvedAmbiguous => "unresolved_ambiguous",
        /// Resolution has not run yet.
        UnresolvedPending => "unresolved_pending",
        /// No decision fits.
        UnresolvedNoMatch => "unresolved_no_match",
        /// The other end is outside what the corpus covers.
        UnresolvedOutOfCoverage => "unresolved_out_of_coverage",
    }
}

wire_enum! {
    /// The kind of a search hit.
    ArtifactType {
        /// A court decision.
        Decision => "DECISION",
        /// A statute.
        Law => "LAW",
    }
}

wire_enum! {
    /// How sure the ranker is about a hit.
    Confidence {
        /// High.
        High => "high",
        /// Medium.
        Medium => "medium",
        /// Low.
        Low => "low",
    }
}

wire_enum! {
    /// The state of a norm text.
    Lifecycle {
        /// The text is in force and confirmed by its source.
        Current => "current",
        /// The law was repealed.
        Repealed => "repealed",
        /// The source no longer serves the law.
        Unconfirmed => "unconfirmed",
    }
}

wire_enum! {
    /// How much text a data package carries per decision.
    TextMode {
        /// Excerpts of about 700 characters.
        Excerpt => "excerpt",
        /// Whole judgments, cut at `max_chars`.
        FullText => "full_text",
    }
}

wire_enum! {
    /// Why a 402 `insufficient_credits` call could not be paid. Held as [`Open`](crate::Open)
    /// because the server may add reasons. Treat a reason you do not know like
    /// [`ApiBudgetNotAvailable`](Self::ApiBudgetNotAvailable).
    InsufficientCreditsReason {
        /// The organization may load a prepaid API budget, and what it holds does not cover the
        /// call. An organization admin can top it up.
        ApiBudgetEmpty => "api_budget_empty",
        /// The organization has no API budget to draw on, or its budget is on hold, and topping up
        /// is not possible from its plan.
        ApiBudgetNotAvailable => "api_budget_not_available",
        /// Reserved for a refusal where the API budget plays no part. The API does not send it
        /// today.
        PlanCreditsExhausted => "plan_credits_exhausted",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_values_round_trip() {
        assert_eq!(Mode::Hybrid.as_str(), "hybrid");
        assert_eq!(
            "Schlussanträge".parse::<DocumentType>(),
            Ok(DocumentType::Schlussantraege)
        );
        assert_eq!(Depth::Two.to_string(), "2");
        assert!("nope".parse::<Branch>().is_err());
    }

    #[test]
    fn request_enums_speak_serde() {
        assert_eq!(serde_json::to_string(&Statute::StGB).unwrap(), "\"StGB\"");
        assert_eq!(
            serde_json::from_str::<DocumentType>("\"Verfügung\"").unwrap(),
            DocumentType::Verfuegung
        );
        assert!(serde_json::from_str::<Mode>("\"fuzzy\"").is_err());
    }
}
