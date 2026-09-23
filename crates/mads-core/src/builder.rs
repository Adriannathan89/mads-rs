//! Explicit application construction and lifecycle ownership.

use std::any::TypeId;
use std::collections::VecDeque;

use crate::auto_configuration::{
    self, AutoConfigurationApplyContext, AutoConfigurationDescriptor, AutoConfigurationInputs,
};
use crate::{
    ApplicationContext, ApplicationGraph, AutoConfigurationReport, Catalog, Config,
    ConstructionContext, ConstructionPlan, ConstructionStep, Diagnostic, Error, GraphAnalysis,
    LifecycleHook, LifecycleManager, LifecycleState, MADS006, MADS008, Module, ModuleGraph,
    ProviderContribution, ProviderDescriptor, ProviderRegistry, Result,
    graph::{
        SatisfiedProvider, analyze_catalog, analyze_descriptors, build_module_graph,
        select_scoped_providers, validate_module_catalog,
    },
};

/// Builds an application by explicitly providing and constructing providers.
pub struct MadsBuilder {
    config: Config,
    registry: ProviderRegistry,
    satisfied: Vec<SatisfiedProvider>,
    auto_configuration_inputs: AutoConfigurationInputs,
    lifecycle: LifecycleManager,
    root: Option<ModuleRoot>,
}

impl MadsBuilder {
    /// Creates a builder with configuration available to provider constructors and resolvers.
    pub fn new(config: Config) -> Self {
        let mut registry = ProviderRegistry::new();
        registry
            .insert(config.clone())
            .expect("a new provider registry cannot already contain configuration");

        Self {
            config,
            registry,
            satisfied: vec![SatisfiedProvider::provided::<Config>()],
            auto_configuration_inputs: AutoConfigurationInputs::default(),
            lifecycle: LifecycleManager::new(),
            root: None,
        }
    }

    /// Selects the root module for scoped analysis and construction.
    #[allow(clippy::result_large_err)]
    pub fn root<M: Module>(&mut self) -> Result<&mut Self> {
        if let Some(root) = &self.root {
            return Err(root_already_selected_error(
                root.type_name,
                std::any::type_name::<M>(),
            ));
        }
        self.root = Some(ModuleRoot::of::<M>());
        Ok(self)
    }

    /// Provides a concrete application-scoped value.
    #[allow(clippy::result_large_err)]
    pub fn provide<T>(&mut self, value: T) -> Result<&mut Self>
    where
        T: Send + Sync + 'static,
    {
        self.registry.insert(value)?;
        self.satisfied.push(SatisfiedProvider::provided::<T>());
        Ok(self)
    }

    /// Constructs exactly one statically declared provider using currently provided dependencies.
    #[allow(clippy::result_large_err)]
    pub async fn construct<T>(&mut self) -> Result<&mut Self>
    where
        T: Send + Sync + 'static,
    {
        let descriptor = Catalog::provider_for::<T>()?;
        let contribution = self.invoke_provider(descriptor).await?;
        self.apply_provider_contribution(descriptor, contribution)?;
        self.satisfied
            .push(SatisfiedProvider::preconstructed::<T>());
        Ok(self)
    }

    /// Analyzes the complete provider graph without invoking constructors.
    pub fn analyze(&self) -> GraphAnalysis {
        self.analyze_builder().public
    }

    /// Registers a private input for an official auto-configuration integration.
    #[doc(hidden)]
    pub fn __auto_configuration_input<T: Send + Sync + 'static>(
        &mut self,
        identifier: &'static str,
        input: T,
    ) -> bool {
        self.auto_configuration_inputs.insert(identifier, input)
    }

    /// Registers a hook that runs when the completed application starts and stops.
    pub fn lifecycle_hook<H>(&mut self, hook: H) -> &mut Self
    where
        H: LifecycleHook + 'static,
    {
        self.lifecycle.add_hook(hook);
        self
    }

    /// Registers a framework-owned infrastructure lifecycle hook.
    #[doc(hidden)]
    pub fn __infrastructure_lifecycle_hook<H>(&mut self, owner: &'static str, hook: H) -> &mut Self
    where
        H: LifecycleHook + 'static,
    {
        self.lifecycle.add_infrastructure_hook(owner, hook);
        self
    }

    /// Validates and automatically constructs the complete application graph.
    #[allow(clippy::result_large_err)]
    pub async fn build(mut self) -> Result<Mads> {
        let BuilderAnalysis {
            public,
            selected,
            failure,
        } = self.analyze_builder();
        if !public.is_valid() {
            return Err(build_analysis_error(public, failure));
        }
        let (graph, construction_plan, auto_configurations, module_graph) =
            public.into_valid_parts()?;

        for descriptor in selected {
            let context = AutoConfigurationApplyContext::new(
                descriptor.identifier(),
                &self.config,
                &self.auto_configuration_inputs,
                module_graph.as_ref(),
            );
            let contribution = (descriptor.applier())(&context)?;
            let (provider, hooks) = contribution.into_parts();
            self.registry.insert_erased(
                descriptor.output_type_id(),
                descriptor.output_type_name(),
                provider,
            )?;
            for hook in hooks {
                self.lifecycle
                    .add_boxed_infrastructure_hook(descriptor.identifier(), hook);
            }
        }

        for step in construction_plan.steps() {
            let descriptor = step.descriptor();
            let contribution = self
                .invoke_provider(descriptor)
                .await
                .map_err(|source| provider_construction_error(step, &graph, source))?;
            self.apply_provider_contribution(descriptor, contribution)?;
        }

        Ok(Mads {
            context: ApplicationContext::new(self.registry, self.config),
            lifecycle: self.lifecycle,
            graph,
            construction_plan,
            auto_configurations,
            module_graph,
        })
    }

    fn analyze_builder(&self) -> BuilderAnalysis {
        let providers = Catalog::providers();
        let Some(root) = &self.root else {
            return self.analyze_complete_catalog(&providers);
        };

        let modules = Catalog::modules();
        let mut module_graph = match build_module_graph(root.type_id, &modules) {
            Ok(module_graph) => module_graph,
            Err(error) => {
                return BuilderAnalysis {
                    public: GraphAnalysis::invalid(error.diagnostics().to_vec()),
                    selected: Vec::new(),
                    failure: None,
                };
            }
        };
        let initial_scope = select_scoped_providers(&module_graph, &providers, &self.satisfied);
        let auto_configuration = auto_configuration::analyze_parts(
            &auto_configuration::descriptors(),
            &initial_scope.descriptors,
            &self.satisfied,
            &self.config,
            &self.auto_configuration_inputs,
            Some(&module_graph),
        );
        let mut satisfied = self.satisfied.clone();
        satisfied.extend(auto_configuration.virtual_satisfied);

        let scoped = select_scoped_providers(&module_graph, &providers, &satisfied);
        let mut covered_missing = auto_configuration.covered_missing;
        for type_id in &scoped.covered_missing {
            if !covered_missing.contains(type_id) {
                covered_missing.push(*type_id);
            }
        }
        let mut public = analyze_descriptors(&scoped.descriptors, &satisfied, &covered_missing);
        public.prepend_diagnostics(scoped.diagnostics);
        public.append_diagnostics(auto_configuration.diagnostics);
        public.auto_configurations = auto_configuration.reports;
        module_graph.set_provider_ownership(scoped.ownership);
        public.set_module_graph(module_graph);

        BuilderAnalysis {
            public,
            selected: auto_configuration.selected,
            failure: auto_configuration.failure,
        }
    }

    async fn invoke_provider(
        &self,
        descriptor: &'static ProviderDescriptor,
    ) -> Result<ProviderContribution> {
        let context = ConstructionContext::new(&self.registry, &self.config);
        if let Some(constructor) = descriptor.lifecycle_constructor() {
            constructor(&context).await
        } else {
            (descriptor.constructor())(&context)
                .await
                .map(ProviderContribution::from_provider)
        }
    }

    fn apply_provider_contribution(
        &mut self,
        descriptor: &'static ProviderDescriptor,
        contribution: ProviderContribution,
    ) -> Result<()> {
        let (provider, registrations) = contribution.into_parts();
        self.registry
            .insert_erased(descriptor.type_id(), descriptor.type_name(), provider)?;
        for registration in registrations {
            self.lifecycle.add_registration(registration);
        }
        Ok(())
    }

    fn analyze_complete_catalog(
        &self,
        providers: &[&'static crate::ProviderDescriptor],
    ) -> BuilderAnalysis {
        let module_diagnostics = validate_module_catalog(&Catalog::modules())
            .err()
            .map_or_else(Vec::new, |error| error.diagnostics().to_vec());
        let auto_configuration = auto_configuration::analyze_parts(
            &auto_configuration::descriptors(),
            providers,
            &self.satisfied,
            &self.config,
            &self.auto_configuration_inputs,
            None,
        );
        let mut satisfied = self.satisfied.clone();
        satisfied.extend(auto_configuration.virtual_satisfied);

        let mut public = analyze_catalog(&satisfied, &auto_configuration.covered_missing);
        public.prepend_diagnostics(module_diagnostics);
        public.auto_configurations = auto_configuration.reports;
        public.append_diagnostics(auto_configuration.diagnostics);

        BuilderAnalysis {
            public,
            selected: auto_configuration.selected,
            failure: auto_configuration.failure,
        }
    }
}

struct ModuleRoot {
    type_id: TypeId,
    type_name: &'static str,
}

impl ModuleRoot {
    fn of<M: Module>() -> Self {
        Self {
            type_id: TypeId::of::<M>(),
            type_name: std::any::type_name::<M>(),
        }
    }
}

struct BuilderAnalysis {
    public: GraphAnalysis,
    selected: Vec<&'static AutoConfigurationDescriptor>,
    failure: Option<Error>,
}

/// An explicitly constructed application and its lifecycle state.
pub struct Mads {
    context: ApplicationContext,
    lifecycle: LifecycleManager,
    graph: ApplicationGraph,
    construction_plan: ConstructionPlan,
    auto_configurations: Vec<AutoConfigurationReport>,
    module_graph: Option<ModuleGraph>,
}

impl Mads {
    /// Creates a builder with empty configuration.
    pub fn builder() -> MadsBuilder {
        Self::builder_with_config(Config::empty())
    }

    /// Creates a builder with caller-supplied configuration.
    pub fn builder_with_config(config: Config) -> MadsBuilder {
        MadsBuilder::new(config)
    }

    /// Returns the application's current lifecycle state.
    pub const fn state(&self) -> LifecycleState {
        self.lifecycle.state()
    }

    /// Returns the immutable application context.
    pub const fn context(&self) -> &ApplicationContext {
        &self.context
    }

    /// Returns the immutable graph validated before construction.
    pub const fn graph(&self) -> &ApplicationGraph {
        &self.graph
    }

    /// Returns the deterministic provider construction plan that was executed.
    pub const fn construction_plan(&self) -> &ConstructionPlan {
        &self.construction_plan
    }

    /// Returns reports for official auto-configurations evaluated before the build.
    pub fn auto_configurations(&self) -> &[AutoConfigurationReport] {
        &self.auto_configurations
    }

    /// Returns the rooted module graph, or `None` for a complete-catalog build.
    pub const fn module_graph(&self) -> Option<&ModuleGraph> {
        self.module_graph.as_ref()
    }

    /// Starts registered lifecycle hooks.
    #[allow(clippy::result_large_err)]
    pub async fn start(&mut self) -> Result<()> {
        self.lifecycle.start(&self.context).await
    }

    /// Stops registered lifecycle hooks in reverse registration order.
    #[allow(clippy::result_large_err)]
    pub async fn shutdown(&mut self) -> Result<()> {
        self.lifecycle.shutdown(&self.context).await
    }
}

fn root_already_selected_error(existing: &'static str, attempted: &'static str) -> Error {
    Error::new(
        Diagnostic::new(
            MADS008,
            "application root already selected",
            format!("a builder already selected `{existing}` and cannot select another root"),
        )
        .with_subject(attempted),
    )
}

fn build_analysis_error(public: GraphAnalysis, failure: Option<Error>) -> Error {
    let GraphAnalysis { diagnostics, .. } = public;
    if let Some(failure) = failure {
        let primary = failure.diagnostic().clone();
        let mut primary_removed = false;
        let related = diagnostics.into_iter().filter(|diagnostic| {
            if !primary_removed && *diagnostic == primary {
                primary_removed = true;
                false
            } else {
                true
            }
        });
        return failure.with_related_diagnostics(related);
    }

    let mut diagnostics = diagnostics.into_iter();
    let primary = diagnostics
        .next()
        .expect("invalid analysis has diagnostics");
    Error::from_diagnostics(primary, diagnostics)
}

fn provider_construction_error(
    step: &ConstructionStep,
    graph: &ApplicationGraph,
    source: Error,
) -> Error {
    let mut diagnostic = Diagnostic::new(
        MADS006,
        "provider construction failed",
        "a provider constructor returned an error",
    )
    .with_subject(step.type_name())
    .with_location(step.location());
    if let Some(path) = consumer_path(graph, step) {
        diagnostic = diagnostic.with_suggestion(format!("construction path: {path}"));
    }
    Error::with_source(diagnostic, source)
}

fn consumer_path(graph: &ApplicationGraph, failing_step: &ConstructionStep) -> Option<String> {
    if !graph
        .dependencies
        .iter()
        .any(|edge| edge.dependency_type_id == failing_step.type_id())
    {
        return None;
    }

    let mut roots = graph
        .providers
        .iter()
        .enumerate()
        .filter(|(_, provider)| {
            !graph
                .dependencies
                .iter()
                .any(|edge| edge.dependency_type_id == provider.type_id)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    roots.sort_by(|left, right| provider_order(graph, *left, *right));

    let mut visited = vec![false; graph.providers.len()];
    let mut queue = VecDeque::new();
    for root in roots {
        visited[root] = true;
        queue.push_back(vec![root]);
    }

    while let Some(path) = queue.pop_front() {
        let provider = *path.last().expect("paths are never empty");
        if graph.providers[provider].type_id == failing_step.type_id() {
            return Some(
                path.iter()
                    .map(|index| graph.providers[*index].type_name)
                    .collect::<Vec<_>>()
                    .join(" -> "),
            );
        }

        let mut dependencies = graph
            .dependencies
            .iter()
            .filter(|edge| edge.provider_type_id == graph.providers[provider].type_id)
            .filter_map(|edge| {
                graph
                    .providers
                    .iter()
                    .position(|candidate| candidate.type_id == edge.dependency_type_id)
            })
            .collect::<Vec<_>>();
        dependencies.sort_by(|left, right| provider_order(graph, *left, *right));
        for dependency in dependencies {
            if !visited[dependency] {
                visited[dependency] = true;
                let mut path = path.clone();
                path.push(dependency);
                queue.push_back(path);
            }
        }
    }

    None
}

fn provider_order(graph: &ApplicationGraph, left: usize, right: usize) -> std::cmp::Ordering {
    let left = &graph.providers[left];
    let right = &graph.providers[right];
    left.type_name
        .cmp(right.type_name)
        .then_with(|| left.origin.cmp(&right.origin))
        .then_with(|| match (left.location, right.location) {
            (Some(left), Some(right)) => left
                .file
                .cmp(right.file)
                .then_with(|| left.line.cmp(&right.line))
                .then_with(|| left.column.cmp(&right.column)),
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
}
