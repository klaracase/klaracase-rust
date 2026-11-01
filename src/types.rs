//! Closed vocabularies for request parameters.
//!
//! Request parameters are typed strictly, so a typo in `mode` or `branch` is a compile error and not
//! a 422 after a round trip. The enums are `#[non_exhaustive]`: a later release can add a value.
//! Response fields that carry such words stay plain `String` and never fail on a new value.

use std::fmt;

macro_rules! wire_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident => $wire:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// The value as it goes on the wire.
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = UnknownValue;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($wire => Ok(Self::$variant),)+
                    other => Err(UnknownValue { kind: stringify!($name), value: other.to_owned() }),
                }
            }
        }
    };
}

/// The error of `FromStr` for the parameter enums.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{value}` is not a valid {kind}")]
pub struct UnknownValue {
    kind: &'static str,
    value: String,
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
}
