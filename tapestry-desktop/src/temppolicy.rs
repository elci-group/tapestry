//! TempCheq-derived temperature policy (Gate 3, directive §12).
//!
//! TempCheq provides heuristic priors, not measured truth. Runtime consumes
//! the prior, may adjust within the band, and logs every adjustment.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Low,
    Moderate,
    High,
}

impl Band {
    pub fn contains(self, t: f32) -> bool {
        let (lo, hi) = self.range();
        t >= lo && t <= hi
    }

    /// (inclusive_low, inclusive_high)
    pub fn range(self) -> (f32, f32) {
        match self {
            Band::Low => (0.0, 0.3),
            Band::Moderate => (0.4, 0.7),
            Band::High => (0.8, 1.0),
        }
    }

    fn clamp_value(self, t: f32) -> f32 {
        let (lo, hi) = self.range();
        t.clamp(lo, hi)
    }
}

/// One versioned policy entry: a prior with provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyEntry {
    pub task_class: String,
    pub band: Band,
    pub recommended_initial: f32,
    /// 0.0..1.0 — heuristics are not measured truth, so confidence matters.
    pub confidence: f32,
    pub evidence_origin: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyTable {
    pub version: u32,
    pub entries: Vec<PolicyEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("no policy prior for task class {0:?}")]
    NoPrior(String),
}

/// Every adjustment is a first-class loggable record (§12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adjustment {
    pub task_class: String,
    pub original: f32,
    pub selected: f32,
    pub band: Band,
    pub reason: String,
}

impl std::fmt::Display for Adjustment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "temp[{}]: {} -> {} (band {:?}, reason: {})",
            self.task_class, self.original, self.selected, self.band, self.reason
        )
    }
}

/// The heuristic prior table (versioned; empirics supersede it later).
pub fn default_table() -> PolicyTable {
    let entry = |task_class: &str, band: Band, rec: f32| PolicyEntry {
        task_class: task_class.into(),
        band,
        recommended_initial: rec,
        confidence: 0.6,
        evidence_origin: "tempcheq-heuristic".into(),
    };
    PolicyTable {
        version: 1,
        entries: vec![
            entry("deterministic_extraction", Band::Low, 0.2),
            entry("classification", Band::Low, 0.2),
            entry("visual_critique", Band::Moderate, 0.6),
            entry("concept_generation", Band::High, 0.9),
            entry("evaluation", Band::Low, 0.3),
        ],
    }
}

impl PolicyTable {
    pub fn prior_for(&self, task_class: &str) -> Option<&PolicyEntry> {
        self.entries.iter().find(|e| e.task_class == task_class)
    }

    /// Apply a requested temperature within the policy band, clamping and
    /// recording the adjustment. Out-of-band requests are clamped, never
    /// refused — but the clamp is always logged.
    pub fn adjust(
        &self,
        task_class: &str,
        requested: f32,
        reason: &str,
    ) -> Result<Adjustment, PolicyError> {
        let prior = self
            .prior_for(task_class)
            .ok_or_else(|| PolicyError::NoPrior(task_class.to_owned()))?;
        let selected = prior.band.clamp_value(requested);
        Ok(Adjustment {
            task_class: task_class.to_owned(),
            original: requested,
            selected,
            band: prior.band,
            reason: reason.to_owned(),
        })
    }

    /// Initial temperature for a class from the prior.
    pub fn initial(&self, task_class: &str) -> Result<f32, PolicyError> {
        self.prior_for(task_class)
            .map(|e| e.recommended_initial)
            .ok_or_else(|| PolicyError::NoPrior(task_class.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priors_exist_for_all_default_classes() {
        let table = default_table();
        assert_eq!(table.initial("concept_generation").unwrap(), 0.9);
        assert_eq!(table.initial("evaluation").unwrap(), 0.3);
        assert!(matches!(
            table.initial("unknown_class"),
            Err(PolicyError::NoPrior(_))
        ));
    }

    #[test]
    fn in_band_adjustment_passes_through() {
        let table = default_table();
        let adj = table
            .adjust("visual_critique", 0.5, "operator preference")
            .unwrap();
        assert_eq!(adj.selected, 0.5);
        assert_eq!(adj.band, Band::Moderate);
    }

    #[test]
    fn out_of_band_request_is_clamped_into_band() {
        let table = default_table();
        let adj = table
            .adjust("classification", 0.95, "agent asked for hot sampling")
            .unwrap();
        assert!(adj.band.contains(adj.selected));
        assert_eq!(adj.selected, 0.3); // clamped to Low band ceiling
        assert_eq!(adj.original, 0.95);
    }

    #[test]
    fn adjustment_is_loggable() {
        let table = default_table();
        let adj = table.adjust("concept_generation", 1.0, "creative").unwrap();
        let rendered = adj.to_string();
        assert!(rendered.contains("concept_generation"));
        assert!(rendered.contains("creative"));
    }

    #[test]
    fn table_is_serde_roundtrip() {
        let table = default_table();
        let back: PolicyTable =
            serde_json::from_str(&serde_json::to_string(&table).unwrap()).unwrap();
        assert_eq!(table, back);
    }
}
