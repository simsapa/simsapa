//! Tests for the shared model-list update procedure
//! (`backend/src/provider_models_update.rs`): source parsing/filtering, the
//! FR-A6 merge semantics, and the FR-A8 default free-model heuristic, all
//! against fixture snapshots — no network.

use anyhow::anyhow;

use simsapa_backend::app_settings::{ModelEntry, ModelOrigin, Provider, ProviderName};
use simsapa_backend::provider_models_update::{
    FetchedModel, ModelsDevData, apply_provider_update, is_chat_model, merge_provider_models,
    parse_openrouter_json, parse_sambanova_json, pick_default_model,
};

static MODELSDEV_FIXTURE: &str = include_str!("data/modelsdev-fixture.json");
static OPENROUTER_FIXTURE: &str = include_str!("data/openrouter-native-fixture.json");
static SAMBANOVA_FIXTURE: &str = include_str!("data/sambanova-native-fixture.json");

fn fetched(id: &str) -> FetchedModel {
    FetchedModel {
        id: id.to_string(),
        cost_in: None,
        cost_out: None,
        release: None,
        context: None,
        reasoning: None,
    }
}

fn entry(name: &str, enabled: bool, origin: ModelOrigin, stale: bool) -> ModelEntry {
    ModelEntry {
        model_name: name.to_string(),
        enabled,
        origin,
        stale,
        reasoning: None,
    }
}

fn test_provider(name: ProviderName, models: Vec<ModelEntry>) -> Provider {
    Provider {
        name,
        description: String::new(),
        enabled: true,
        api_key_env_var_name: String::new(),
        api_key_value: None,
        models,
    }
}

// ---- filtering -------------------------------------------------------------

#[test]
fn chat_filter_denylist_and_modalities() {
    let text = vec!["text".to_string()];
    let image = vec!["image".to_string()];

    assert!(is_chat_model("gemini-flash-latest", Some(&text), Some(&text)));
    // Denylisted ids are rejected regardless of modalities.
    assert!(!is_chat_model("gemini-embedding-001", Some(&text), Some(&text)));
    assert!(!is_chat_model("Whisper-Large-v3", None, None));
    assert!(!is_chat_model("llama-guard-4", None, None));
    // Non-text output is rejected when modalities are published.
    assert!(!is_chat_model("veo-3.1", Some(&text), Some(&image)));
    // Without modality data only the denylist applies.
    assert!(is_chat_model("Meta-Llama-3.3-70B-Instruct", None, None));
}

#[test]
fn models_dev_provider_models_filters_and_normalizes() {
    let data = ModelsDevData::parse(MODELSDEV_FIXTURE).unwrap();
    let google = data.provider_models("google").unwrap();
    let ids: Vec<&str> = google.iter().map(|m| m.id.as_str()).collect();

    assert!(ids.contains(&"gemini-flash-latest"));
    assert!(ids.contains(&"gemini-2.5-flash"));
    assert!(ids.contains(&"gemini-2.5-pro"));
    // Denylisted (embed) and non-text-output (video) models are filtered out.
    assert!(!ids.contains(&"gemini-embedding-001"));
    assert!(!ids.contains(&"veo-3.1"));

    let flash = google.iter().find(|m| m.id == "gemini-flash-latest").unwrap();
    assert_eq!(flash.cost_in, Some(0.3));
    assert_eq!(flash.cost_out, Some(2.5));
    assert_eq!(flash.release.as_deref(), Some("2025-09-25"));
    assert_eq!(flash.context, Some(1048576));

    // The reasoning flag is carried through when published.
    let pro = google.iter().find(|m| m.id == "gemini-2.5-pro").unwrap();
    assert_eq!(pro.reasoning, Some(true));
    let flash25 = google.iter().find(|m| m.id == "gemini-2.5-flash").unwrap();
    assert_eq!(flash25.reasoning, Some(false));
}

#[test]
fn models_dev_missing_provider_key_is_an_error() {
    let data = ModelsDevData::parse(MODELSDEV_FIXTURE).unwrap();
    // A missing key must be an Err (caller keeps the existing list), never
    // an empty Vec (which would wipe all fetched models on merge).
    assert!(data.provider_models("sambanova").is_err());
}

#[test]
fn openrouter_parse_keeps_free_chat_models_only() {
    let models = parse_openrouter_json(OPENROUTER_FIXTURE).unwrap();
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();

    assert!(ids.contains(&"meta-llama/llama-3.3-70b-instruct:free"));
    assert!(ids.contains(&"qwen/qwen3-coder:free"));
    // Non-:free, denylisted, and non-text-output entries are dropped.
    assert!(!ids.contains(&"openai/gpt-4o"));
    assert!(!ids.contains(&"acme/legal-embedder:free"));
    assert!(!ids.contains(&"acme/pixel-painter:free"));

    // Unix `created` is normalized to an ISO date; pricing strings parse.
    let qwen = models.iter().find(|m| m.id == "qwen/qwen3-coder:free").unwrap();
    assert_eq!(qwen.release.as_deref(), Some("2025-06-15"));
    assert_eq!(qwen.cost_in, Some(0.0));
    assert_eq!(qwen.context, Some(262144));

    // "reasoning" in supported_parameters marks a reasoning model.
    assert_eq!(qwen.reasoning, Some(true));
    let llama = models.iter().find(|m| m.id == "meta-llama/llama-3.3-70b-instruct:free").unwrap();
    assert_eq!(llama.reasoning, Some(false));
}

#[test]
fn sambanova_parse_is_bare_ids_with_no_metadata() {
    let models = parse_sambanova_json(SAMBANOVA_FIXTURE).unwrap();
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();

    assert!(ids.contains(&"Meta-Llama-3.3-70B-Instruct"));
    assert!(ids.contains(&"DeepSeek-V3-0324"));
    assert!(!ids.contains(&"Whisper-Large-v3"));

    for m in &models {
        assert!(m.cost_in.is_none() && m.release.is_none() && m.context.is_none());
    }
}

// ---- merge semantics (FR-A6) -----------------------------------------------

#[test]
fn merge_refreshes_and_seeds_the_reasoning_flag() {
    let existing = vec![
        entry("model-a", true, ModelOrigin::Fetched, false),
        entry("user-c", true, ModelOrigin::User, false),
    ];
    let mut a = fetched("model-a");
    a.reasoning = Some(true);
    let mut e = fetched("model-e");
    e.reasoning = Some(false);
    // user-c is absent upstream; a source with no reasoning data (None) must
    // not erase a previously known value either.
    let upstream = vec![a, e];

    let (merged, _stats) = merge_provider_models(&existing, &upstream);
    let get = |n: &str| merged.iter().find(|m| m.model_name == n).unwrap();

    // Surviving model refreshed from the source; new model seeded from it.
    assert_eq!(get("model-a").reasoning, Some(true));
    assert_eq!(get("model-e").reasoning, Some(false));
    // Absent upstream: keeps what was known (here: unknown).
    assert_eq!(get("user-c").reasoning, None);
}

#[test]
fn merge_add_remove_stale_and_enabled_preserved() {
    let existing = vec![
        entry("model-a", true, ModelOrigin::Fetched, false),
        entry("model-b", false, ModelOrigin::Fetched, false),
        entry("user-c", true, ModelOrigin::User, false),
        entry("user-d", false, ModelOrigin::User, true),
    ];
    // Upstream still has a, lost b, lost user-c, regained user-d, added e.
    let upstream = vec![fetched("model-a"), fetched("user-d"), fetched("model-e")];

    let (merged, stats) = merge_provider_models(&existing, &upstream);

    let names: Vec<&str> = merged.iter().map(|m| m.model_name.as_str()).collect();
    // Surviving entries keep their order; new fetched models append at the end.
    assert_eq!(names, vec!["model-a", "user-c", "user-d", "model-e"]);

    let get = |n: &str| merged.iter().find(|m| m.model_name == n).unwrap();
    // Surviving fetched model keeps its enabled state.
    assert!(get("model-a").enabled);
    // User model absent upstream: kept, enabled preserved, flagged stale.
    assert!(get("user-c").enabled);
    assert!(get("user-c").stale);
    // User model found upstream again: stale flag cleared.
    assert!(!get("user-d").stale);
    // New fetched model: appended disabled, fetched origin.
    assert!(!get("model-e").enabled);
    assert_eq!(get("model-e").origin, ModelOrigin::Fetched);

    assert_eq!(stats.added, 1);
    assert_eq!(stats.removed, 1); // model-b
    assert_eq!(stats.staled, 1); // user-c (user-d was already stale)
}

#[test]
fn merge_with_empty_existing_appends_all_disabled() {
    let (merged, stats) = merge_provider_models(&[], &[fetched("m1"), fetched("m2")]);
    assert_eq!(merged.len(), 2);
    assert!(merged.iter().all(|m| !m.enabled && m.origin == ModelOrigin::Fetched));
    assert_eq!(stats.added, 2);
}

#[test]
fn fetch_failure_keeps_list_unchanged() {
    let models = vec![
        entry("model-a", true, ModelOrigin::Fetched, false),
        entry("user-b", false, ModelOrigin::User, false),
    ];
    let mut provider = test_provider(ProviderName::Gemini, models.clone());

    let report = apply_provider_update(&mut provider, Err(anyhow!("network error")), true);

    assert_eq!(provider.models, models);
    assert!(report.error.as_deref().unwrap_or("").contains("network error"));
    assert_eq!(report.added, 0);
    assert_eq!(report.removed, 0);
    assert!(report.default_enabled.is_none());
}

// ---- default free-model heuristic (FR-A8) -----------------------------------

#[test]
fn heuristic_openrouter_picks_newest_free_model() {
    let models = parse_openrouter_json(OPENROUTER_FIXTURE).unwrap();
    // Both survivors are zero-cost `:free`; qwen3-coder has the newer release.
    assert_eq!(pick_default_model(&models).as_deref(), Some("qwen/qwen3-coder:free"));
}

#[test]
fn heuristic_gemini_prefers_budget_family_alias() {
    let data = ModelsDevData::parse(MODELSDEV_FIXTURE).unwrap();
    let google = data.provider_models("google").unwrap();
    // No zero-cost Google models (free tier is rate-limit-based) → budget
    // family (flash), and the alias id wins over versioned flash ids.
    assert_eq!(pick_default_model(&google).as_deref(), Some("gemini-flash-latest"));
}

#[test]
fn heuristic_anthropic_enables_nothing() {
    let data = ModelsDevData::parse(MODELSDEV_FIXTURE).unwrap();
    let anthropic = data.provider_models("anthropic").unwrap();
    // Paid-only, no budget-family ids → no default.
    assert_eq!(pick_default_model(&anthropic), None);
}

#[test]
fn heuristic_zero_cost_from_models_dev_costs() {
    let candidates = vec![
        FetchedModel {
            id: "paid-model".into(),
            cost_in: Some(1.0),
            cost_out: Some(5.0),
            release: Some("2026-01-01".into()),
            context: Some(100_000),
            reasoning: None,
        },
        FetchedModel {
            id: "gratis-model".into(),
            cost_in: Some(0.0),
            cost_out: Some(0.0),
            release: Some("2025-01-01".into()),
            context: Some(8_192),
            reasoning: None,
        },
    ];
    // Zero-cost beats budget-family/newer paid models.
    assert_eq!(pick_default_model(&candidates).as_deref(), Some("gratis-model"));
}

#[test]
fn heuristic_context_breaks_release_ties() {
    let candidates = vec![
        FetchedModel {
            id: "a:free".into(),
            cost_in: Some(0.0),
            cost_out: Some(0.0),
            release: Some("2026-01-01".into()),
            context: Some(32_000),
            reasoning: None,
        },
        FetchedModel {
            id: "b:free".into(),
            cost_in: Some(0.0),
            cost_out: Some(0.0),
            release: Some("2026-01-01".into()),
            context: Some(128_000),
            reasoning: None,
        },
    ];
    assert_eq!(pick_default_model(&candidates).as_deref(), Some("b:free"));
}

// ---- apply_provider_update with defaults ------------------------------------

#[test]
fn apply_defaults_enables_exactly_the_picked_model() {
    let mut provider = test_provider(ProviderName::OpenRouter, vec![]);
    let upstream = parse_openrouter_json(OPENROUTER_FIXTURE).unwrap();

    let report = apply_provider_update(&mut provider, Ok(upstream), true);

    assert_eq!(report.default_enabled.as_deref(), Some("qwen/qwen3-coder:free"));
    let enabled: Vec<&str> = provider
        .models
        .iter()
        .filter(|m| m.enabled)
        .map(|m| m.model_name.as_str())
        .collect();
    assert_eq!(enabled, vec!["qwen/qwen3-coder:free"]);
}

#[test]
fn no_defaults_applied_in_app_mode() {
    let mut provider = test_provider(ProviderName::OpenRouter, vec![]);
    let upstream = parse_openrouter_json(OPENROUTER_FIXTURE).unwrap();

    let report = apply_provider_update(&mut provider, Ok(upstream), false);

    assert!(report.default_enabled.is_none());
    assert!(provider.models.iter().all(|m| !m.enabled));
    assert_eq!(report.added, 2);
}

#[test]
fn apply_defaults_never_disables_user_enabled_models() {
    let mut provider = test_provider(
        ProviderName::OpenRouter,
        vec![entry(
            "meta-llama/llama-3.3-70b-instruct:free",
            true,
            ModelOrigin::Fetched,
            false,
        )],
    );
    let upstream = parse_openrouter_json(OPENROUTER_FIXTURE).unwrap();

    apply_provider_update(&mut provider, Ok(upstream), true);

    let llama = provider
        .models
        .iter()
        .find(|m| m.model_name == "meta-llama/llama-3.3-70b-instruct:free")
        .unwrap();
    assert!(llama.enabled, "user-enabled model must stay enabled");
}
