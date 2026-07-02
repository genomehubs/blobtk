use crate::error::Error;
use crate::index::es::models::IndexDocument;
use std::collections::HashMap;
use std::path::Path;

pub trait DocumentBuilder<D: IndexDocument> {
    fn build_from_processed_data(
        &self,
        processed: &HashMap<String, HashMap<String, String>>,
    ) -> Result<Vec<D>, Error>;
    fn build_from_yaml(&self, cfg_path: &Path) -> Result<Vec<D>, Error>;
    fn build_from_tsv(&self, tsv_path: &Path, flavour: &str) -> Result<Vec<D>, Error>;
}

pub mod feature;
