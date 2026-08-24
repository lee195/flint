//! The cookbook (doc 01): a build-time-pinned tier table that maps hardware to exactly
//! ONE recommended model. No catalog, no quantization jargon, no parameter UI — the
//! choice is made for the user. Staleness is honest: `COOKBOOK_AS_OF` is shown in the UI.

use crate::types::{AgentCapability, ModelDescriptor, ProbeResult, Recommendation, Tier};

/// "recommendations as of 2026-08" — the table is the product and it rots monthly (doc 01);
/// it is refreshed via app updates, never silently fetched.
pub const COOKBOOK_AS_OF: &str = "2026-08";

/// Build-time pinned table — exactly one entry per tier, in tier order (8 → 64 GB).
/// Top tier measured on this machine (2026-08-24); lower tiers pinned-but-untested until
/// real hardware appears (doc 01's maintenance stance).
const TABLE: &[ModelDescriptor] = &[
    ModelDescriptor {
        tag: "qwen3:4b",
        family: "Qwen3",
        size_gb: 2.9,
        license: "Apache-2.0",
        source: "https://ollama.com/library/qwen3",
        num_ctx: 8192,
        think: false,
        agent: AgentCapability::Locked,
    },
    ModelDescriptor {
        tag: "qwen3:8b",
        family: "Qwen3",
        size_gb: 5.2,
        license: "Apache-2.0",
        source: "https://ollama.com/library/qwen3",
        num_ctx: 8192,
        think: false,
        agent: AgentCapability::Locked,
    },
    ModelDescriptor {
        tag: "qwen3:14b",
        family: "Qwen3",
        size_gb: 9.0,
        license: "Apache-2.0",
        source: "https://ollama.com/library/qwen3",
        num_ctx: 16384,
        think: false,
        agent: AgentCapability::Locked,
    },
    ModelDescriptor {
        tag: "qwen3.6:latest",
        family: "Qwen3.6",
        size_gb: 22.3,
        license: "Apache-2.0",
        source: "https://ollama.com/library/qwen3.6",
        num_ctx: 16384,
        think: false,
        agent: AgentCapability::Locked,
    },
];

/// The single recommendation for a tier.
pub fn model_for_tier(tier: Tier) -> &'static ModelDescriptor {
    match tier {
        Tier::Gb8 => &TABLE[0],
        Tier::Gb16 => &TABLE[1],
        Tier::Gb32 => &TABLE[2],
        Tier::Gb64 => &TABLE[3],
    }
}

pub fn find_by_tag(tag: &str) -> Option<&'static ModelDescriptor> {
    TABLE.iter().find(|d| d.tag == tag)
}

/// Full recommendation for the probe, with `already_installed` resolved against the
/// engine's current model list.
pub fn recommendation(probe: &ProbeResult, installed_tags: &[String]) -> Option<Recommendation> {
    let tier = probe.tier?;
    let model = model_for_tier(tier);
    Some(Recommendation {
        probe: probe.clone(),
        model: Some(model.clone()),
        as_of: COOKBOOK_AS_OF,
        already_installed: installed_tags.iter().any(|t| t == &model.tag),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_for(ram_gb: f64) -> ProbeResult {
        ProbeResult {
            chip: "Apple M5 Pro".into(),
            ram_gb,
            tier: Tier::of_ram_gb(ram_gb),
            supported: true,
            unsupported_reason: None,
        }
    }

    #[test]
    fn table_is_complete_and_locked() {
        assert_eq!(TABLE.len(), 4);
        for d in TABLE {
            assert!(!d.tag.is_empty());
            assert!(d.think == false, "thinking is off for the no-params chat pane");
            assert_eq!(d.agent, AgentCapability::Locked, "agent mode is Phase 2");
            assert!(d.num_ctx >= 8192);
        }
        // one model per tier
        for tier in [Tier::Gb8, Tier::Gb16, Tier::Gb32, Tier::Gb64] {
            assert_eq!(model_for_tier(tier).tag, TABLE[tier as usize].tag);
        }
    }

    #[test]
    fn each_tier_maps_to_its_model() {
        assert_eq!(model_for_tier(Tier::Gb8).tag, "qwen3:4b");
        assert_eq!(model_for_tier(Tier::Gb16).tag, "qwen3:8b");
        assert_eq!(model_for_tier(Tier::Gb32).tag, "qwen3:14b");
        assert_eq!(model_for_tier(Tier::Gb64).tag, "qwen3.6:latest");
    }

    #[test]
    fn recommendation_flags_installed() {
        let p = probe_for(64.0);
        let rec = recommendation(&p, &[]).unwrap();
        assert_eq!(rec.model.unwrap().tag, "qwen3.6:latest");
        assert!(!rec.already_installed);
        assert_eq!(rec.as_of, COOKBOOK_AS_OF);

        let rec2 = recommendation(&p, &["qwen3.6:latest".to_string()]).unwrap();
        assert!(rec2.already_installed);
    }

    #[test]
    fn unsupported_hardware_has_no_recommendation() {
        let p = ProbeResult {
            chip: "Intel Core i7".into(),
            ram_gb: 16.0,
            tier: None,
            supported: false,
            unsupported_reason: Some("Flint needs Apple Silicon (M-series).".into()),
        };
        assert!(recommendation(&p, &[]).is_none());
    }

    #[test]
    fn below_floor_has_no_recommendation() {
        let p = probe_for(4.0);
        assert!(recommendation(&p, &[]).is_none());
    }
}
