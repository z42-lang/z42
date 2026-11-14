//! Immutable semantic-role bindings, validated before component instances exist.
use crate::components::abi::{ComponentId, ComponentInterface, ExecutionApi};
use super::catalog::Factory;
use anyhow::{ensure, Result};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionRole {
    Baseline,
    Optimizing,
}
impl ExecutionRole {
    fn index(self) -> usize {
        match self { Self::Baseline => 0, Self::Optimizing => 1 }
    }
}

#[derive(Clone, Default)]
pub(super) struct ExecutionBindings {
    providers: [Option<ComponentId>; 2],
}
impl ExecutionBindings {
    pub(super) fn new(bindings: impl IntoIterator<Item = (ExecutionRole, ComponentId)>) -> Result<Self> {
        let mut result = Self::default();
        for (role, provider) in bindings {
            let slot = &mut result.providers[role.index()];
            ensure!(slot.is_none(), "ambiguous execution role {role:?}");
            *slot = Some(provider);
        }
        Ok(result)
    }

    pub(super) fn provider(&self, role: ExecutionRole) -> Option<ComponentId> {
        self.providers[role.index()]
    }

    pub(super) fn validate(&self, factories: &[Arc<Factory>]) -> Result<()> {
        for provider in self.providers.iter().flatten() {
            let factory = factories.iter().find(|f| f.id == *provider)
                .ok_or_else(|| anyhow::anyhow!("execution provider {provider} is missing"))?;
            let spec = factory.interfaces.iter().find(|s| s.id == ExecutionApi::ID)
                .ok_or_else(|| anyhow::anyhow!("execution provider {provider} has no execution interface"))?;
            ensure!(spec.version == ExecutionApi::VERSION
                && spec.size as usize == std::mem::size_of::<ExecutionApi>()
                && spec.alignment as usize == std::mem::align_of::<ExecutionApi>(),
                "execution provider {provider} has an incompatible execution interface");
        }
        Ok(())
    }
}
