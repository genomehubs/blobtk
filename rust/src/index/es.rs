//! Elasticsearch index module
//! This module provides functionality for managing Elasticsearch indices, including creating, updating, and deleting indices, as well as handling index mappings and settings.
//! It also includes utilities for indexing documents and performing search operations.
//! The module is designed to work with the Elasticsearch client and provides a high-level interface for interacting with Elasticsearch indices.
//! The main components of this module include:
//! - `IndexManager`: A struct responsible for managing Elasticsearch indices, including creating, updating, and deleting indices, as well as handling index mappings and settings.
//! - `DocumentIndexer`: A struct responsible for indexing documents into Elasticsearch indices, including handling bulk indexing operations and managing document IDs.
//! - `SearchClient`: A struct responsible for performing search operations on Elasticsearch indices, including handling search queries and managing search results.
//! - `IndexConfig`: A struct representing the configuration for an Elasticsearch index, including settings and mappings.
//! - `IndexError`: An enum representing possible errors that can occur during index management and document indexing operations.
//!
//! This module is designed to be flexible and extensible, allowing for easy integration with different Elasticsearch versions and configurations. It also includes error handling and logging to ensure that any issues encountered during index management and document indexing operations are properly reported and handled.
//! The module is structured to promote separation of concerns and maintainability, with clear interfaces for each component and comprehensive documentation to guide users in utilizing the provided functionality effectively.
//! The module also includes unit tests to verify the correctness of the implemented functionality and ensure that any changes made to the codebase do not introduce regressions or unintended side effects. These tests cover various scenarios, including successful index creation, document indexing, and search operations, as well as error handling for invalid configurations and failed operations. By providing a robust set of tests, this module aims to maintain high code quality and reliability while facilitating ongoing development and maintenance efforts.
//!
//! Overall, this module serves as a critical component for managing Elasticsearch indices and facilitating efficient document indexing and search operations, while also providing a solid foundation for future enhancements and integrations with other components of the system.

pub mod builders;
pub mod client;
pub mod config;
pub mod indexer;
pub mod manager;
pub mod mappings;
pub mod models;
