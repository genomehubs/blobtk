use crate::index::es::models::attribute_builder::merge_attribute_documents;
use crate::index::es::models::documents::{AttributeDocument, FeatureDocument};
use crate::parse::busco::{BlockSetMetrics, BuscoIdTracker};
use std::collections::HashMap;

/// Tallies busco status counts per sequence and per window
pub struct BuscoCountAggregator {
    pub seq_counts: HashMap<String, HashMap<String, HashMap<String, usize>>>,
    pub window_counts: HashMap<String, HashMap<String, HashMap<String, usize>>>,
    pub assembly_counts: HashMap<String, HashMap<String, usize>>, // NEW: lineage -> category -> count
    missing_count: usize,
}

impl BuscoCountAggregator {
    pub fn new() -> Self {
        BuscoCountAggregator {
            seq_counts: HashMap::new(),
            window_counts: HashMap::new(),
            assembly_counts: HashMap::new(),
            missing_count: 0,
        }
    }

    pub fn add_missing(&mut self) {
        self.missing_count += 1;
    }

    /// Increment assembly-level count for a category
    pub fn add_to_assembly(&mut self, lineage: &str, category: &str) {
        self.assembly_counts
            .entry(lineage.to_string())
            .or_insert_with(HashMap::new)
            .entry(category.to_string())
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }

    /// Increment count for a sequence+lineage+status
    pub fn add_to_sequence(&mut self, seq_id: &str, lineage: &str, status: &str) {
        self.seq_counts
            .entry(seq_id.to_string())
            .or_insert_with(HashMap::new)
            .entry(lineage.to_string())
            .or_insert_with(HashMap::new)
            .entry(status.to_string())
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }

    /// Increment count for a window+lineage+status
    pub fn add_to_window(&mut self, window_id: &str, lineage: &str, status: &str) {
        self.window_counts
            .entry(window_id.to_string())
            .or_insert_with(HashMap::new)
            .entry(lineage.to_string())
            .or_insert_with(HashMap::new)
            .entry(status.to_string())
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }
}

/// Cache for AttributeDocument creation (dedup by attribute key / document id)
pub struct AttributeDocumentCache {
    pub documents: HashMap<String, AttributeDocument>,
}

impl AttributeDocumentCache {
    pub fn new() -> Self {
        AttributeDocumentCache {
            documents: HashMap::new(),
        }
    }

    pub fn register_existing(&mut self, document: AttributeDocument) {
        self.documents.insert(document.name.clone(), document);
    }

    pub fn merge_or_insert(
        &mut self,
        candidate: AttributeDocument,
    ) -> Result<Option<AttributeDocument>, crate::error::Error> {
        if let Some(existing) = self.documents.get(&candidate.name) {
            let merged = merge_attribute_documents(existing, &candidate);
            let existing_json = serde_json::to_value(existing)?;
            let merged_json = serde_json::to_value(&merged)?;
            if existing_json == merged_json {
                Ok(None)
            } else {
                self.documents.insert(merged.name.clone(), merged.clone());
                Ok(Some(merged))
            }
        } else {
            self.documents
                .insert(candidate.name.clone(), candidate.clone());
            Ok(Some(candidate))
        }
    }
}

/// Global import state (persists across sequence→busco→window parsing)
pub struct ImportState {
    pub sequences: HashMap<String, FeatureDocument>,
    pub busco_counts: BuscoCountAggregator,
    pub synteny_metrics_by_seq: HashMap<String, BlockSetMetrics>,
    pub synteny_metrics_by_window: HashMap<String, BlockSetMetrics>,
    pub busco_id_tracker: BuscoIdTracker,
    pub attribute_doc_cache: AttributeDocumentCache,
    pub assembly_id: String,
    pub taxon_id: String,
}

impl ImportState {
    pub fn new(assembly_id: String, taxon_id: String) -> Self {
        ImportState {
            sequences: HashMap::new(),
            busco_counts: BuscoCountAggregator::new(),
            busco_id_tracker: BuscoIdTracker::new(),
            attribute_doc_cache: AttributeDocumentCache::new(),
            assembly_id,
            synteny_metrics_by_seq: HashMap::new(),
            synteny_metrics_by_window: HashMap::new(),
            taxon_id,
        }
    }
}
