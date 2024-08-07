//! Permute is a framework and a DSL to describe data transformations.
//! It is designed to be used in a data processing pipeline,
//! where data is transformed from one form to another.
//! Descriptory YAML files are used to describe the data, formats, transformations, validation
//! and other aspects of the data processing.
//! The decision to use YAML was made to make it possible to describe the data transformations
//! independently of the programming language used to implement the transformations, to
//! restrict the complexity of the transformations to a declarative form, to make a sandbox
//! that allows to use only specific sets of operations without access to any
//! restricted or sensitive resources. Basically, this allows to describe the data processing
//! on UI, without the need to write any code.

/// Module to register expected fields in the YAML file, and also describe the provided
/// types and formats, so that framework can validate correctness and compatibility.
pub mod domain;

/// Module that reads the YAML file and converts it to the internal representation
/// that can be used by the framework to perform analysis.
pub mod project;

/// Module to allow writing expressions in the YAML file, that can be used to
/// calculate values or transform the data.
pub mod expr;

/// Module that contains the contract, which is similar to [crate::domain::Domain] as it contains
/// all items, types, and other information
/// from YAML files. This is used to validate the configuration against
/// actual [crate::domain::Domain].
pub mod contract;

/// Module to aid user in understanding errors and providing hints on how to fix them.
pub mod error_expl;

#[cfg(test)]
pub fn init_log() {
    use log::*;

    flexi_logger::Logger::with(LevelFilter::Trace)
        .format(format)
        .start()
        .unwrap();

    fn format(
        write: &mut dyn std::io::Write,
        _: &mut flexi_logger::DeferredNow,
        record: &Record,
    ) -> std::io::Result<()> {
        write.write_all(
            format!(
                "[{} {}:{}] {} - {}",
                record.level(),
                record.file().unwrap_or_default(),
                record.line().unwrap_or_default(),
                record.module_path().unwrap_or_default(),
                record.args()
            )
            .as_bytes(),
        )
    }
}
