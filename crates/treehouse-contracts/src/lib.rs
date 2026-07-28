pub mod compatibility;
pub mod contract_definition;
pub mod contract_migration;
pub mod contract_observer;
pub mod contract_registry;
pub mod contract_validator;
pub mod drift;
pub mod validation;

pub use compatibility::{compare_schema, FieldChange};
pub use contract_definition::{
    generate_contract_tests, ApiContract, ConsumerDependency, ContractTest, ContractTestKind,
    DataContract, EventContract, FieldContract, OwnershipContract, OwnershipDeclaration,
    ProcessContract, ProvidedInterfaces, SchemaContract, SubsystemContract,
};
pub use contract_migration::{build_migration_plan, ContractMigrationPlan, ContractMigrationStep};
pub use contract_observer::{
    detect_subsystem_contract_drift, ContractDrift, ContractDriftKind, ContractDriftReport,
    ObservedContractReality, OwnershipWriteObservation,
};
pub use contract_registry::{
    CompatibilityRecord, ContractChangeImpact, ContractPublication, ContractRegistry,
};
pub use contract_validator::{
    validate_api_contract, validate_schema_payload, ContractValidationIssue, ContractValidationKind,
};
pub use drift::detect_contract_drift;
pub use validation::validate_required_fields;
