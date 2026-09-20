//! Rooted provider selection and module-boundary enforcement.

use std::any::TypeId;
use std::collections::{HashSet, VecDeque};

use crate::{
    Catalog, Diagnostic, MADS009, ModuleDescriptor, ProviderDescriptor, ProviderVisibility,
};

use super::{ModuleGraph, ProviderOwnership, SatisfiedProvider};

pub(crate) struct ScopedProviderCatalog {
    pub(crate) descriptors: Vec<&'static ProviderDescriptor>,
    pub(crate) ownership: Vec<ProviderOwnership>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) covered_missing: Vec<TypeId>,
}

pub(crate) fn select_scoped_providers(
    graph: &ModuleGraph,
    descriptors: &[&'static ProviderDescriptor],
    satisfied: &[SatisfiedProvider],
) -> ScopedProviderCatalog {
    let modules = Catalog::modules();
    let satisfied_types = satisfied
        .iter()
        .map(|provider| provider.type_id)
        .collect::<HashSet<_>>();
    let mut queue = VecDeque::new();

    for (descriptor_index, descriptor) in descriptors.iter().copied().enumerate() {
        if satisfied_types.contains(&descriptor.type_id()) {
            continue;
        }
        let owner = owner_of(descriptor.namespace(), &modules);
        if owner.is_some_and(|owner| graph.is_reachable(owner.type_id())) {
            queue.push_back(WorkItem {
                descriptor_index,
                context: owner.map(ModuleDescriptor::type_id),
                dependency_path: vec![descriptor.type_name()],
            });
        }
    }

    let mut processed = HashSet::new();
    let mut selected_indices = HashSet::new();
    let mut selected: Vec<&'static ProviderDescriptor> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut covered_missing = Vec::new();

    while let Some(work) = queue.pop_front() {
        if !processed.insert((work.descriptor_index, work.context)) {
            continue;
        }
        let descriptor = descriptors[work.descriptor_index];
        if selected_indices.insert(work.descriptor_index) {
            selected.push(descriptor);
        }

        for dependency in descriptor.dependencies() {
            let dependency_type = dependency.type_id();
            if satisfied_types.contains(&dependency_type) {
                continue;
            }
            let mut matching_descriptor_found = false;
            let mut admissible = Vec::new();
            let mut boundary_failures = Vec::new();
            let mut owned_outside_scope = false;
            for (dependency_index, dependency_descriptor) in descriptors
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, descriptor)| descriptor.type_id() == dependency_type)
            {
                matching_descriptor_found = true;
                let mut dependency_path = work.dependency_path.clone();
                dependency_path.push(dependency_descriptor.type_name());
                let owner = owner_of(dependency_descriptor.namespace(), &modules);

                match (work.context, owner) {
                    (context, None) => admissible.push(WorkItem {
                        descriptor_index: dependency_index,
                        context,
                        dependency_path,
                    }),
                    (Some(context), Some(owner)) if owner.type_id() == context => {
                        admissible.push(WorkItem {
                            descriptor_index: dependency_index,
                            context: Some(owner.type_id()),
                            dependency_path,
                        });
                    }
                    (Some(context), Some(owner))
                        if can_access(
                            graph,
                            context,
                            owner,
                            dependency_descriptor.visibility(),
                        ) =>
                    {
                        admissible.push(WorkItem {
                            descriptor_index: dependency_index,
                            context: Some(owner.type_id()),
                            dependency_path,
                        });
                    }
                    (Some(context), Some(owner)) => {
                        boundary_failures.push((
                            context,
                            owner,
                            dependency_descriptor,
                            dependency_path,
                        ));
                    }
                    (None, Some(owner)) if graph.is_reachable(owner.type_id()) => {
                        admissible.push(WorkItem {
                            descriptor_index: dependency_index,
                            context: Some(owner.type_id()),
                            dependency_path,
                        });
                    }
                    (None, Some(_)) => {
                        owned_outside_scope = true;
                    }
                }
            }
            if admissible.is_empty() {
                let boundary_failed = !boundary_failures.is_empty();
                diagnostics.extend(boundary_failures.into_iter().map(
                    |(context, owner, dependency_descriptor, dependency_path)| {
                        boundary_error(
                            graph,
                            context,
                            owner,
                            dependency_descriptor,
                            &dependency_path,
                        )
                    },
                ));
                if matching_descriptor_found
                    && (owned_outside_scope || boundary_failed)
                    && !covered_missing.contains(&dependency_type)
                {
                    covered_missing.push(dependency_type);
                }
            } else {
                queue.extend(admissible);
            }
        }
    }

    let mut ownership_with_types = Vec::new();
    for descriptor in &selected {
        if ownership_with_types
            .iter()
            .any(|(type_id, _)| *type_id == descriptor.type_id())
        {
            continue;
        }
        ownership_with_types.push({
            let owner = owner_of(descriptor.namespace(), &modules);
            (
                descriptor.type_id(),
                ProviderOwnership {
                    provider_type_name: descriptor.type_name(),
                    module_type_name: owner.map(ModuleDescriptor::type_name),
                },
            )
        });
    }
    for provider in satisfied {
        if ownership_with_types
            .iter()
            .all(|(type_id, _)| *type_id != provider.type_id)
        {
            ownership_with_types.push((
                provider.type_id,
                ProviderOwnership {
                    provider_type_name: provider.type_name,
                    module_type_name: None,
                },
            ));
        }
    }
    ownership_with_types.sort_by(|(_, left), (_, right)| {
        left.provider_type_name
            .cmp(right.provider_type_name)
            .then_with(|| left.module_type_name.cmp(&right.module_type_name))
    });

    ScopedProviderCatalog {
        descriptors: selected,
        ownership: ownership_with_types
            .into_iter()
            .map(|(_, ownership)| ownership)
            .collect(),
        diagnostics,
        covered_missing,
    }
}

fn can_access(
    graph: &ModuleGraph,
    context: TypeId,
    owner: &ModuleDescriptor,
    visibility: ProviderVisibility,
) -> bool {
    context == owner.type_id()
        || (visibility == ProviderVisibility::Public
            && (graph.directly_imports(context, owner.type_id())
                || owner.is_global() && graph.is_reachable(owner.type_id())))
}

fn owner_of(
    namespace: Option<&str>,
    modules: &[&'static ModuleDescriptor],
) -> Option<&'static ModuleDescriptor> {
    let namespace = namespace?;
    modules
        .iter()
        .copied()
        .filter(|module| {
            module
                .namespace()
                .is_some_and(|owner| namespace_contains(owner, namespace))
        })
        .max_by_key(|module| module.namespace().map_or(0, str::len))
}

fn namespace_contains(parent: &str, child: &str) -> bool {
    child == parent
        || child
            .strip_prefix(parent)
            .is_some_and(|suffix| suffix.starts_with("::"))
}

fn boundary_error(
    graph: &ModuleGraph,
    context: TypeId,
    owner: &ModuleDescriptor,
    dependency: &ProviderDescriptor,
    dependency_path: &[&str],
) -> Diagnostic {
    let context = graph
        .modules()
        .iter()
        .find(|module| module.type_id() == context)
        .expect("provider work context is always a reachable module");
    let condition = if dependency.visibility() != ProviderVisibility::Public {
        "the target provider does not have unrestricted `pub` visibility"
    } else {
        "the requesting module does not declare the target owner as a direct import"
    };

    Diagnostic::new(
        MADS009,
        "inaccessible module provider",
        format!(
            "{condition}; requester `{}` ({}) cannot access owner `{}` ({})",
            context.type_name(),
            context.namespace(),
            owner.type_name(),
            owner.namespace().unwrap_or("<unknown>")
        ),
    )
    .with_subject(dependency.type_name())
    .with_location(dependency.location())
    .with_suggestion(format!("dependency path: {}", dependency_path.join(" -> ")))
}

struct WorkItem {
    descriptor_index: usize,
    context: Option<TypeId>,
    dependency_path: Vec<&'static str>,
}
