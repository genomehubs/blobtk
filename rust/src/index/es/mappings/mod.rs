pub mod assembly;
pub mod attribute;
pub mod common;
pub mod feature;

pub trait MappingBuilder {
    fn build(&self) -> common::Mappings;
}

pub struct AttributeMappingBuilder;

impl MappingBuilder for AttributeMappingBuilder {
    fn build(&self) -> common::Mappings {
        attribute::attribute_index_mappings()
    }
}

pub fn attribute_index_mappings() -> common::Mappings {
    AttributeMappingBuilder.build()
}

pub struct FeatureMappingBuilder;

impl MappingBuilder for FeatureMappingBuilder {
    fn build(&self) -> common::Mappings {
        feature::feature_index_mappings()
    }
}

pub fn feature_index_mappings() -> common::Mappings {
    FeatureMappingBuilder.build()
}
