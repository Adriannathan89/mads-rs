//! Pure validation and rendering for MADS's minimal application starter.
//!
//! This module deliberately performs no filesystem, process, network, Cargo, or
//! registry work. Publication is added separately, after every name and template
//! failure can be reported without touching a destination.

mod name;
mod publish;
mod template;

pub use self::{
    name::{ProjectName, ProjectNameError},
    publish::{ScaffoldError, publish_project},
    template::{GENERATED_FILES, RenderedFile, RenderedProject, TemplateError, render_project},
};
