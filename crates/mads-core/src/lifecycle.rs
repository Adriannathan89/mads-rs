//! Application lifecycle hooks, state transitions, and failure rollback.

use std::future::Future;
use std::pin::Pin;

use crate::{ApplicationContext, Diagnostic, Error, MADS010, MADS011, Result};

enum HookGroup {
    Infrastructure(&'static str),
    Application,
}

struct RegisteredHook {
    group: HookGroup,
    sequence: usize,
    hook: Box<dyn LifecycleHook>,
}

/// The lifecycle state of an application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleState {
    /// The application has been built but has not started.
    Created,
    /// The application is starting lifecycle hooks.
    Starting,
    /// The application has started successfully.
    Running,
    /// The application is stopping lifecycle hooks.
    Stopping,
    /// The application has stopped and cannot be restarted.
    Stopped,
}

/// The asynchronous result returned by a lifecycle hook.
pub type LifecycleFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// Starts and stops an application resource.
pub trait LifecycleHook: Send + Sync {
    /// Returns a stable name used in lifecycle diagnostics.
    fn name(&self) -> &str;

    /// Starts this resource using the completed application context.
    fn start<'a>(&'a self, context: &'a ApplicationContext) -> LifecycleFuture<'a>;

    /// Stops this resource using the completed application context.
    fn stop<'a>(&'a self, context: &'a ApplicationContext) -> LifecycleFuture<'a>;
}

pub(crate) enum LifecycleRegistration {
    Infrastructure {
        owner: &'static str,
        hook: Box<dyn LifecycleHook>,
    },
    Application {
        hook: Box<dyn LifecycleHook>,
    },
}

/// A native provider value accompanied by lifecycle hooks registered for it.
pub struct LifecycleResource<T> {
    value: T,
    registrations: Vec<LifecycleRegistration>,
}

impl<T> LifecycleResource<T> {
    /// Creates a lifecycle resource without any hooks.
    pub fn new(value: T) -> Self {
        Self {
            value,
            registrations: Vec::new(),
        }
    }

    /// Adds an infrastructure hook owned by the named framework component.
    #[must_use]
    pub fn with_infrastructure_hook<H>(mut self, owner: &'static str, hook: H) -> Self
    where
        H: LifecycleHook + 'static,
    {
        self.registrations
            .push(LifecycleRegistration::Infrastructure {
                owner,
                hook: Box::new(hook),
            });
        self
    }

    /// Adds an application hook after previously authored registrations.
    #[must_use]
    pub fn with_application_hook<H>(mut self, hook: H) -> Self
    where
        H: LifecycleHook + 'static,
    {
        self.registrations.push(LifecycleRegistration::Application {
            hook: Box::new(hook),
        });
        self
    }

    pub(crate) fn into_parts(self) -> (T, Vec<LifecycleRegistration>) {
        (self.value, self.registrations)
    }
}

/// Coordinates ordered lifecycle hooks for one application.
pub struct LifecycleManager {
    state: LifecycleState,
    hooks: Vec<RegisteredHook>,
    next_sequence: usize,
}

impl LifecycleManager {
    /// Creates a manager in the created state with no lifecycle hooks.
    pub fn new() -> Self {
        Self {
            state: LifecycleState::Created,
            hooks: Vec::new(),
            next_sequence: 0,
        }
    }

    /// Adds a lifecycle hook that starts after previously registered hooks.
    pub fn add_hook<H>(&mut self, hook: H) -> &mut Self
    where
        H: LifecycleHook + 'static,
    {
        self.add_registration(LifecycleRegistration::Application {
            hook: Box::new(hook),
        });
        self
    }

    /// Adds an infrastructure hook owned by the named framework component.
    #[doc(hidden)]
    pub fn add_infrastructure_hook<H>(&mut self, owner: &'static str, hook: H) -> &mut Self
    where
        H: LifecycleHook + 'static,
    {
        self.add_registration(LifecycleRegistration::Infrastructure {
            owner,
            hook: Box::new(hook),
        });
        self
    }

    pub(crate) fn add_boxed_infrastructure_hook(
        &mut self,
        owner: &'static str,
        hook: Box<dyn LifecycleHook>,
    ) {
        self.add_registration(LifecycleRegistration::Infrastructure { owner, hook });
    }

    pub(crate) fn add_registration(&mut self, registration: LifecycleRegistration) {
        let (group, hook) = match registration {
            LifecycleRegistration::Infrastructure { owner, hook } => {
                (HookGroup::Infrastructure(owner), hook)
            }
            LifecycleRegistration::Application { hook } => (HookGroup::Application, hook),
        };
        self.hooks.push(RegisteredHook {
            group,
            sequence: self.next_sequence,
            hook,
        });
        self.next_sequence += 1;
    }

    /// Returns the manager's current lifecycle state.
    pub const fn state(&self) -> LifecycleState {
        self.state
    }

    /// Starts infrastructure hooks by owner before application hooks in registration order.
    #[allow(clippy::result_large_err)]
    pub async fn start(&mut self, context: &ApplicationContext) -> Result<()> {
        if self.state != LifecycleState::Created {
            return Err(invalid_transition(self.state, "start"));
        }

        self.state = LifecycleState::Starting;
        self.hooks
            .sort_by(|left, right| match (&left.group, &right.group) {
                (HookGroup::Infrastructure(left_owner), HookGroup::Infrastructure(right_owner)) => {
                    left_owner
                        .cmp(right_owner)
                        .then(left.sequence.cmp(&right.sequence))
                }
                (HookGroup::Infrastructure(_), HookGroup::Application) => std::cmp::Ordering::Less,
                (HookGroup::Application, HookGroup::Infrastructure(_)) => {
                    std::cmp::Ordering::Greater
                }
                (HookGroup::Application, HookGroup::Application) => {
                    left.sequence.cmp(&right.sequence)
                }
            });
        let mut started: Vec<usize> = Vec::new();

        for index in 0..self.hooks.len() {
            let hook = self.hooks[index].hook.as_ref();
            if let Err(error) = hook.start(context).await {
                let mut diagnostic = hook_failure_diagnostic(hook.name(), "startup");
                for started_index in started.into_iter().rev() {
                    let rollback_hook = self.hooks[started_index].hook.as_ref();
                    if let Err(rollback_error) = rollback_hook.stop(context).await {
                        diagnostic = diagnostic.with_suggestion(format!(
                            "rollback hook {} failed: {rollback_error}",
                            rollback_hook.name()
                        ));
                    }
                }
                self.state = LifecycleState::Stopped;
                return Err(Error::with_source(diagnostic, error));
            }
            started.push(index);
        }

        self.state = LifecycleState::Running;
        Ok(())
    }

    /// Stops all hooks in reverse successful startup order.
    #[allow(clippy::result_large_err)]
    pub async fn shutdown(&mut self, context: &ApplicationContext) -> Result<()> {
        if self.state != LifecycleState::Running {
            return Err(invalid_transition(self.state, "shutdown"));
        }

        self.state = LifecycleState::Stopping;
        let mut failure = None;

        for registration in self.hooks.iter().rev() {
            let hook = registration.hook.as_ref();
            if let Err(error) = hook.stop(context).await
                && failure.is_none()
            {
                failure = Some(hook_failure(hook.name(), "shutdown", error));
            }
        }

        self.state = LifecycleState::Stopped;
        failure.map_or(Ok(()), Err)
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

fn invalid_transition(state: LifecycleState, operation: &str) -> Error {
    Error::new(
        Diagnostic::new(
            MADS010,
            "invalid lifecycle transition",
            format!("cannot {operation} an application while it is {state:?}"),
        )
        .with_subject(operation),
    )
}

fn hook_failure(name: &str, operation: &str, error: Error) -> Error {
    Error::with_source(hook_failure_diagnostic(name, operation), error)
}

fn hook_failure_diagnostic(name: &str, operation: &str) -> Diagnostic {
    Diagnostic::new(
        MADS011,
        "lifecycle hook failed",
        format!("lifecycle hook failed during {operation}"),
    )
    .with_subject(name)
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use super::*;
    use crate::ProviderContribution;

    struct Value;

    struct NamedHook(&'static str);

    impl LifecycleHook for NamedHook {
        fn name(&self) -> &str {
            self.0
        }

        fn start<'a>(&'a self, _: &'a ApplicationContext) -> LifecycleFuture<'a> {
            Box::pin(async { Ok(()) })
        }

        fn stop<'a>(&'a self, _: &'a ApplicationContext) -> LifecycleFuture<'a> {
            Box::pin(async { Ok(()) })
        }
    }

    #[test]
    fn resource_preserves_hook_order_and_erases_the_native_value() {
        let resource = LifecycleResource::new(Value)
            .with_infrastructure_hook("zeta", NamedHook("first"))
            .with_application_hook(NamedHook("second"))
            .with_infrastructure_hook("alpha", NamedHook("third"));

        let (provider, registrations) = ProviderContribution::from_resource(resource).into_parts();

        assert_eq!(provider.as_ref().type_id(), TypeId::of::<Value>());
        assert_eq!(registrations.len(), 3);
        assert!(matches!(
            &registrations[0],
            LifecycleRegistration::Infrastructure { owner: "zeta", hook }
                if hook.name() == "first"
        ));
        assert!(matches!(
            &registrations[1],
            LifecycleRegistration::Application { hook } if hook.name() == "second"
        ));
        assert!(matches!(
            &registrations[2],
            LifecycleRegistration::Infrastructure { owner: "alpha", hook }
                if hook.name() == "third"
        ));
    }
}
