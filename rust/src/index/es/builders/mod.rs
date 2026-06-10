use crate::error::Error;
use crate::index::es::models::IndexDocument;
use std::path::Path;

pub trait DocumentBuilder<D: IndexDocument> {
    fn build_from_yaml(&self, cfg_path: &Path) -> Result<Vec<D>, Error>;
    fn build_from_tsv(&self, tsv_path: &Path) -> Result<Vec<D>, Error>;
}
