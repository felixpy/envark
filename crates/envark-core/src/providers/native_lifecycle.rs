use super::*;
use crate::{
    model::{Inventory, Settings},
    operations::{ActionRequest, PlanItem},
};

#[derive(Debug, Clone)]
pub(crate) enum Plan {
    Language(language_lifecycle::Plan),
    Ollama(ollama_lifecycle::Plan),
}

macro_rules! dispatch {
    ($self:expr, $method:ident) => {
        match $self {
            Plan::Language(plan) => plan.$method(),
            Plan::Ollama(plan) => plan.$method(),
        }
    };
}

impl Plan {
    pub(crate) fn item(&self) -> PlanItem {
        dispatch!(self, item)
    }
    pub(crate) fn warnings(&self) -> Vec<String> {
        dispatch!(self, warnings)
    }
    pub(crate) fn kind(&self) -> &'static str {
        dispatch!(self, kind)
    }
    pub(crate) fn providers(&self) -> Vec<ProviderId> {
        dispatch!(self, providers)
    }
    pub(crate) async fn execute(self, ctx: &Context, use_trash: bool) -> Result<(u64, String)> {
        match self {
            Self::Language(plan) => plan.execute(ctx, use_trash).await,
            Self::Ollama(plan) => plan.execute(ctx, use_trash).await,
        }
    }
}

pub(crate) async fn prepare(
    ctx: &Context,
    request: ActionRequest,
    inventory: &Inventory,
    settings: &Settings,
) -> Result<Option<Plan>> {
    if language_lifecycle::handles(&request, inventory) {
        return language_lifecycle::prepare(ctx, request, inventory, settings)
            .await
            .map(Plan::Language)
            .map(Some);
    }
    if ollama_lifecycle::handles(&request, inventory) {
        return ollama_lifecycle::prepare(ctx, request, inventory, settings)
            .await
            .map(Plan::Ollama)
            .map(Some);
    }
    Ok(None)
}

pub(crate) fn owns_tool(tool: &Tool) -> bool {
    language_lifecycle::owns_tool(tool) || ollama_lifecycle::owns_tool(tool)
}

pub(crate) async fn latest(ctx: &Context, tool: &Tool) -> Result<String> {
    if language_lifecycle::owns_tool(tool) {
        language_lifecycle::latest(ctx, tool).await
    } else {
        ollama_lifecycle::latest(ctx, tool).await
    }
}
