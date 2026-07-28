pub mod compatibility;
pub mod drift;
pub mod validation;

pub use compatibility::{compare_schema, FieldChange};
pub use drift::detect_contract_drift;
pub use validation::validate_required_fields;
