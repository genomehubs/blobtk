//! FeatureDocument builder implementation takes processed data emitted by validate_record/validate_record_from_map and an makes a featureDocument
//! Top level fields are popultated from validated entries with a subset also included as attributes
//! Also emits Vec<AttributeDocument> for the attributes of the feature, which are indexed separately in the attribute index

use crate::error::Error;
use crate::index::es::builders::DocumentBuilder;
use crate::index::es::models::{documents::FeatureDocument, IndexDocument};
use std::collections::HashMap;

pub struct FeatureDocumentBuilder;

impl DocumentBuilder<FeatureDocument> for FeatureDocumentBuilder {
    fn build_from_processed_data(
        &self,
        processed: &HashMap<String, HashMap<String, String>>,
    ) -> Result<Vec<FeatureDocument>, Error> {
        dbg!(&processed);
        let feature_document = FeatureDocument {
            feature_id: "feat1".to_string(),
            primary_type: "gene".to_string(),
            start: 100,
            end: 500,
            length: 400,
            strand: Some(1),
            parent_feature_id: None,
            container_ids: None,
            sequence_id: "chr1".to_string(),
            sequence_length: 1000000,
            assembly_id: "asm1".to_string(),
            taxon_id: "9606".to_string(),
            ancestors: None,
            file_id: None,
            analysis_id: None,
            attributes: None,
            identifiers: None,
        };
        // Implementation goes here
        unimplemented!()
    }

    fn build_from_yaml(&self, cfg_path: &std::path::Path) -> Result<Vec<FeatureDocument>, Error> {
        // Implementation goes here
        unimplemented!()
    }

    fn build_from_tsv(
        &self,
        tsv_path: &std::path::Path,
        flavour: &str,
    ) -> Result<Vec<FeatureDocument>, Error> {
        // Implementation goes here
        if flavour == "bed" {
            // Parse BED TSV format
            unimplemented!()
        } else if flavour == "busco" {
            // Parse BUSCO TSV format
            unimplemented!()
        } else if flavour == "gff" {
            // Parse GFF TSV format
            unimplemented!()
        } else {
            Err(Error::UnsupportedFileType(flavour.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use crate::validation::validator::GenomeHubsValidator;
    use std::collections::HashMap;

    #[test]
    fn test_build_from_processed_data() {
        // Setup minimal processed data for a feature
        let mut processed = HashMap::new();
        let mut feature_data = HashMap::new();
        feature_data.insert("feature_id".to_string(), "feat1".to_string());
        feature_data.insert("primary_type".to_string(), "gene".to_string());
        feature_data.insert("start".to_string(), "100".to_string());
        feature_data.insert("end".to_string(), "500".to_string());
        feature_data.insert("length".to_string(), "400".to_string());
        feature_data.insert("sequence_id".to_string(), "chr1".to_string());
        feature_data.insert("sequence_length".to_string(), "1000000".to_string());
        feature_data.insert("assembly_id".to_string(), "asm1".to_string());
        feature_data.insert("taxon_id".to_string(), "9606".to_string());
        processed.insert("feat1".to_string(), feature_data);

        let builder = FeatureDocumentBuilder;
        let result = builder.build_from_processed_data(&processed);
        assert!(result.is_ok());
        let documents = result.unwrap();
        assert_eq!(documents.len(), 1);
        let doc = &documents[0];
        assert_eq!(doc.feature_id, "feat1");
        assert_eq!(doc.primary_type, "gene");
        assert_eq!(doc.start, 100);
        assert_eq!(doc.end, 500);
        assert_eq!(doc.length, 400);
        assert_eq!(doc.sequence_id, "chr1");
        assert_eq!(doc.sequence_length, 1000000);
        assert_eq!(doc.assembly_id, "asm1");
        assert_eq!(doc.taxon_id, "9606");

        // Additional assertions can be added to check optional fields and attributes

        // Test with missing required fields to check error handling
        let mut incomplete_processed = HashMap::new();
        let mut incomplete_feature_data = HashMap::new();
        incomplete_feature_data.insert("feature_id".to_string(), "feat2".to_string());
        incomplete_processed.insert("feat2".to_string(), incomplete_feature_data);
        let result = builder.build_from_processed_data(&incomplete_processed);
        assert!(result.is_err());
    }

    // Additional tests for build_from_yaml and build_from_tsv can be implemented similarly, using test YAML and TSV files with known content to verify correct parsing and document creation.

    #[test]
    fn test_build_from_yaml() {
        // Implementation of test for build_from_yaml goes here
        unimplemented!()
    }

    #[test]
    fn test_build_from_tsv() {
        // Implementation of test for build_from_tsv goes here
        unimplemented!()
    }
}
