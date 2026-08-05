#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::collections::HashMap;

use crate::model::{
    OpenRouterModel, ProviderBrand, ProviderId, ProviderModelOption, ReasoningEffort, SpeedMode,
};

pub type ProviderCatalogs = HashMap<ProviderId, Vec<ProviderModelOption>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderBrandMeta {
    pub id: ProviderBrand,
    pub name: &'static str,
    pub runtime: ProviderId,
    pub color: &'static str,
    pub model_prefix: Option<&'static str>,
}

pub const PROVIDER_BRANDS: [ProviderBrandMeta; 7] = [
    ProviderBrandMeta {
        id: ProviderBrand::Openai,
        name: "OpenAI",
        runtime: ProviderId::Codex,
        color: "#10a37f",
        model_prefix: None,
    },
    ProviderBrandMeta {
        id: ProviderBrand::Anthropic,
        name: "Anthropic",
        runtime: ProviderId::Claude,
        color: "#d97757",
        model_prefix: None,
    },
    ProviderBrandMeta {
        id: ProviderBrand::Google,
        name: "Google",
        runtime: ProviderId::Gemini,
        color: "#4285f4",
        model_prefix: None,
    },
    ProviderBrandMeta {
        id: ProviderBrand::Xai,
        name: "xAI",
        runtime: ProviderId::Openrouter,
        color: "#111111",
        model_prefix: Some("x-ai/"),
    },
    ProviderBrandMeta {
        id: ProviderBrand::Moonshot,
        name: "Moonshot AI",
        runtime: ProviderId::Kimi,
        color: "#5b5bd6",
        model_prefix: None,
    },
    ProviderBrandMeta {
        id: ProviderBrand::Opencode,
        name: "OpenCode",
        runtime: ProviderId::Opencode,
        color: "#fab283",
        model_prefix: None,
    },
    ProviderBrandMeta {
        id: ProviderBrand::Openrouter,
        name: "OpenRouter",
        runtime: ProviderId::Openrouter,
        color: "#6d5dfc",
        model_prefix: None,
    },
];

pub fn brand_meta(brand: ProviderBrand) -> ProviderBrandMeta {
    PROVIDER_BRANDS
        .into_iter()
        .find(|item| item.id == brand)
        .unwrap_or(PROVIDER_BRANDS[6])
}

pub fn runtime_for_brand(brand: ProviderBrand) -> ProviderId {
    brand_meta(brand).runtime
}

fn named_model(
    id: &str,
    name: &str,
    is_default: bool,
    reasoning: Vec<ReasoningEffort>,
) -> ProviderModelOption {
    ProviderModelOption {
        id: id.to_owned(),
        name: name.to_owned(),
        description: None,
        is_default,
        legacy: false,
        default_reasoning: reasoning
            .contains(&ReasoningEffort::Medium)
            .then_some(ReasoningEffort::Medium),
        reasoning,
        speeds: vec![SpeedMode::Standard],
        default_speed: SpeedMode::Standard,
        context_length: None,
    }
}

fn configured_default_model(provider: &str) -> ProviderModelOption {
    let mut model = named_model(
        "default",
        &format!("Use {provider} setting"),
        true,
        Vec::new(),
    );
    model.description = Some(format!(
        "Onyx does not pass --model; {provider} uses the model selected by its own account and configuration."
    ));
    model
}

pub fn fallback_catalogs() -> ProviderCatalogs {
    HashMap::from([
        (
            ProviderId::Codex,
            vec![configured_default_model("Codex CLI")],
        ),
        (
            ProviderId::Claude,
            vec![configured_default_model("Claude Code")],
        ),
        (
            ProviderId::Gemini,
            vec![
                named_model("auto", "Auto · Gemini CLI", true, vec![]),
                named_model("pro", "Pro · Gemini CLI", false, vec![]),
                named_model("flash", "Flash · Gemini CLI", false, vec![]),
                named_model("flash-lite", "Flash Lite · Gemini CLI", false, vec![]),
            ],
        ),
        (
            ProviderId::Kimi,
            vec![configured_default_model("Kimi Code")],
        ),
        (
            ProviderId::Opencode,
            vec![configured_default_model("OpenCode")],
        ),
    ])
}

pub fn models_for_brand(
    brand: ProviderBrand,
    catalogs: &ProviderCatalogs,
    openrouter_models: &[OpenRouterModel],
) -> Vec<ProviderModelOption> {
    let meta = brand_meta(brand);
    if meta.runtime == ProviderId::Openrouter {
        return openrouter_models
            .iter()
            .filter(|model| {
                meta.model_prefix
                    .is_none_or(|prefix| model.id.starts_with(prefix))
            })
            .enumerate()
            .map(|(index, model)| ProviderModelOption {
                id: model.id.clone(),
                name: model.name.clone(),
                description: model.description.clone(),
                is_default: index == 0,
                legacy: false,
                reasoning: Vec::new(),
                default_reasoning: None,
                speeds: vec![SpeedMode::Standard],
                default_speed: SpeedMode::Standard,
                context_length: model.context_length,
            })
            .collect();
    }
    catalogs.get(&meta.runtime).cloned().unwrap_or_else(|| {
        fallback_catalogs()
            .remove(&meta.runtime)
            .unwrap_or_default()
    })
}

pub fn selected_or_default(models: &[ProviderModelOption]) -> Option<ProviderModelOption> {
    models
        .iter()
        .find(|model| model.is_default)
        .or_else(|| models.first())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xai_catalog_only_contains_xai_models() {
        let models = vec![
            OpenRouterModel {
                id: "x-ai/grok-4".into(),
                name: "Grok 4".into(),
                description: None,
                context_length: Some(256_000),
                prompt_price: None,
                completion_price: None,
                input_modalities: vec![],
                output_modalities: vec![],
            },
            OpenRouterModel {
                id: "openai/gpt-5".into(),
                name: "GPT-5".into(),
                description: None,
                context_length: None,
                prompt_price: None,
                completion_price: None,
                input_modalities: vec![],
                output_modalities: vec![],
            },
        ];
        let result = models_for_brand(ProviderBrand::Xai, &fallback_catalogs(), &models);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "x-ai/grok-4");
    }
}
