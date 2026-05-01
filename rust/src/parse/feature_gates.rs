//! Feature gates for taxonomy improvements
//!
//! These gates allow controlled rollout and A/B testing of experimental fixes.
//! Each gate defaults to `false` (disabled) to preserve existing behavior.
//!
//! Use in YAML config:
//! ```yaml
//! experimental_fixes:
//!   bracket_name_stripping: true
//!   genus_rank_filtering: true
//!   multiatch_candidate_ranking: true
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Feature gates for taxonomy improvements
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ExperimentalFixes {
    /// Fix #1: Strip brackets from incertae sedis names like [Family] sp.
    /// When enabled, extracts actual family name and looks it up properly
    /// instead of creating a malformed genus node.
    /// Issue: Bracketed family names create wrong-ranked genus nodes
    /// Location: src/parse.rs:268-275
    #[serde(default)]
    pub bracket_name_stripping: bool,

    /// Fix #2: Filter genus selection by rank instead of using .first()
    /// When enabled, prefers genus entries over species/synonyms when
    /// multiple candidates exist in id_map.
    /// Issue: Non-deterministic parent selection for "Nosema sp." patterns
    /// Location: src/parse.rs:276-280
    #[serde(default)]
    pub genus_rank_filtering: bool,

    /// Fix #3: Rank MultiMatch candidates by match quality
    /// When enabled, sorts candidates by rank match and name class priority
    /// instead of arbitrary order.
    /// Issue: Wrong ancestor context for taxa with multiple synonyms
    /// Location: src/parse/lookup.rs:600-630
    #[serde(default)]
    pub multimatch_candidate_ranking: bool,
}

impl ExperimentalFixes {
    /// Check if all fixes are enabled
    pub fn all_enabled(&self) -> bool {
        self.bracket_name_stripping
            && self.genus_rank_filtering
            && self.multimatch_candidate_ranking
    }

    /// Check if any fixes are enabled
    pub fn any_enabled(&self) -> bool {
        self.bracket_name_stripping
            || self.genus_rank_filtering
            || self.multimatch_candidate_ranking
    }

    /// Return a summary of enabled gates
    pub fn summary(&self) -> String {
        let mut enabled = vec![];
        if self.bracket_name_stripping {
            enabled.push("bracket_name_stripping");
        }
        if self.genus_rank_filtering {
            enabled.push("genus_rank_filtering");
        }
        if self.multimatch_candidate_ranking {
            enabled.push("multimatch_candidate_ranking");
        }

        if enabled.is_empty() {
            "No experimental fixes enabled".to_string()
        } else {
            format!("Experimental fixes enabled: {}", enabled.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_disabled_by_default() {
        let gates = ExperimentalFixes::default();
        assert!(!gates.bracket_name_stripping);
        assert!(!gates.genus_rank_filtering);
        assert!(!gates.multimatch_candidate_ranking);
    }

    #[test]
    fn test_all_enabled() {
        let gates = ExperimentalFixes {
            bracket_name_stripping: true,
            genus_rank_filtering: true,
            multimatch_candidate_ranking: true,
        };
        assert!(gates.all_enabled());
        assert!(gates.any_enabled());
    }

    #[test]
    fn test_partially_enabled() {
        let gates = ExperimentalFixes {
            bracket_name_stripping: true,
            genus_rank_filtering: false,
            multimatch_candidate_ranking: false,
        };
        assert!(!gates.all_enabled());
        assert!(gates.any_enabled());
    }

    #[test]
    fn test_summary() {
        let gates = ExperimentalFixes {
            bracket_name_stripping: true,
            genus_rank_filtering: false,
            multimatch_candidate_ranking: true,
        };
        let summary = gates.summary();
        assert!(summary.contains("bracket_name_stripping"));
        assert!(summary.contains("multimatch_candidate_ranking"));
        assert!(!summary.contains("genus_rank_filtering"));
    }
}
