//! Session continuation and lineage resolution (Gate 5 / directive §36).
//!
//! The second acceptance demonstration: `tapestry "Make the selected
//! concept darker"` must know which artefact was selected without the user
//! repeating its identifier. Selections live in the knowledge journal;
//! this module resolves them into a continuation.

use crate::knowledge::{KnowledgeJournal, NodeKind, Relationship};

/// A resolved continuation: which session/artefact to build upon, and the
/// actionable remainder of the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continuation {
    pub parent_session_raw: String,
    pub parent_artifact_raw: String,
    pub directive: String,
}

/// Anaphora / directive markers that make a prompt a continuation of the
/// most recent session rather than a fresh request.
const CONTINUATION_MARKERS: &[&str] = &[
    "make it",
    "make that",
    "make the selected",
    "the selected",
    "that one",
    "same but",
    "darker",
    "lighter",
];

/// Lowercase-leading verbs also indicate continuation ("darken the hero").
const LOWERCASE_VERBS: &[&str] = &["adjust", "change", "darken", "lighten", "refine"];

pub fn resolve_continuation(journal: &KnowledgeJournal, prompt: &str) -> Option<Continuation> {
    if !is_continuation_prompt(prompt) {
        return None;
    }
    let session_node = most_recent_session(journal)?;
    let session_id = session_node.id.clone();
    let artifact_id = selected_artifact(journal, &session_id)
        .or_else(|| first_artifact_of_session(journal, &session_id))?;
    Some(Continuation {
        parent_session_raw: session_id,
        parent_artifact_raw: artifact_id,
        directive: strip_directive(prompt),
    })
}

fn is_continuation_prompt(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    if CONTINUATION_MARKERS.iter().any(|m| lower.contains(m)) {
        return true;
    }
    let trimmed = lower.trim_start();
    // Fresh prompts are usually capitalised sentences or noun phrases;
    // a bare lowercase directive verb implies "continue working".
    LOWERCASE_VERBS
        .iter()
        .any(|v| trimmed.starts_with(v) && trimmed[v.len()..].starts_with(' '))
}

fn most_recent_session(journal: &KnowledgeJournal) -> Option<crate::knowledge::KnowledgeNode> {
    // Node ids embed time-sortable ids, so max id == most recent.
    journal
        .nodes()
        .iter()
        .filter(|n| n.kind == NodeKind::Session)
        .max_by(|a, b| a.id.cmp(&b.id))
        .cloned()
}

/// The artefact a human explicitly selected in this session
/// (`SELECTED_OVER` subject), if recorded.
fn selected_artifact(journal: &KnowledgeJournal, session_id: &str) -> Option<String> {
    journal
        .edges()
        .iter()
        .find(|e| {
            e.rel == Relationship::SelectedOver
                && journal.edges().iter().any(|b| {
                    b.rel == Relationship::BelongsToSession
                        && b.from == e.from
                        && b.to == session_id
                })
        })
        .map(|e| e.from.clone())
}

fn first_artifact_of_session(journal: &KnowledgeJournal, session_id: &str) -> Option<String> {
    journal
        .edges()
        .iter()
        .find(|e| e.rel == Relationship::BelongsToSession && e.to == session_id)
        .map(|e| e.from.clone())
}

/// Strip continuation markers, leaving the actionable directive text.
pub fn strip_directive(prompt: &str) -> String {
    let mut result = prompt.trim().to_string();
    let lower = result.to_lowercase();
    for marker in CONTINUATION_MARKERS {
        if let Some(pos) = lower.find(marker) {
            if pos == 0 || lower.as_bytes().get(pos.wrapping_sub(1)) == Some(&b' ') {
                result = result[pos + marker.len()..].trim_start().to_string();
                break;
            }
        }
    }
    let lower = result.to_lowercase();
    for verb in LOWERCASE_VERBS {
        let verb_space = format!("{verb} ");
        if lower.starts_with(&verb_space) {
            result = result[verb_space.len()..].trim_start().to_string();
            break;
        }
        if lower == *verb {
            result = String::new();
            break;
        }
    }
    result.trim_start_matches([',', '.']).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{KnowledgeEdge, KnowledgeNode, Origin};

    fn seeded_journal(path: &std::path::Path) -> KnowledgeJournal {
        let mut j = KnowledgeJournal::open(path).unwrap();
        let node = |id: &str, kind: NodeKind| KnowledgeNode {
            id: id.into(),
            kind,
            label: id.into(),
        };
        j.append_node(node("session_tap_OLD", NodeKind::Session))
            .unwrap();
        j.append_node(node("artefact_art_C", NodeKind::Artefact))
            .unwrap();
        j.append_node(node("artefact_art_A", NodeKind::Artefact))
            .unwrap();
        j.append_node(node("decision_D1", NodeKind::Decision))
            .unwrap();
        let edge = |from: &str, rel: Relationship, to: &str, origin: Origin| KnowledgeEdge {
            from: from.into(),
            rel,
            to: to.into(),
            confidence: 1.0,
            origin,
        };
        j.append_edge(edge(
            "artefact_art_C",
            Relationship::BelongsToSession,
            "session_tap_OLD",
            Origin::AgentJudgement,
        ))
        .unwrap();
        j.append_edge(edge(
            "artefact_art_A",
            Relationship::BelongsToSession,
            "session_tap_OLD",
            Origin::AgentJudgement,
        ))
        .unwrap();
        j.append_edge(edge(
            "decision_D1",
            Relationship::BelongsToSession,
            "session_tap_OLD",
            Origin::ExplicitHumanJudgement,
        ))
        .unwrap();
        // The human selected C over A (§26 example shape).
        j.append_edge(edge(
            "artefact_art_C",
            Relationship::SelectedOver,
            "artefact_art_A",
            Origin::ExplicitHumanJudgement,
        ))
        .unwrap();
        j
    }

    #[test]
    fn continuation_resolves_selected_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let journal = seeded_journal(&dir.path().join("k.jsonl"));
        let cont = resolve_continuation(&journal, "Make the selected concept darker").unwrap();
        assert_eq!(cont.parent_session_raw, "session_tap_OLD");
        assert_eq!(cont.parent_artifact_raw, "artefact_art_C");
        assert!(cont.directive.contains("darker") || cont.directive.contains("concept"));
    }

    #[test]
    fn fresh_prompt_is_not_a_continuation() {
        let dir = tempfile::tempdir().unwrap();
        let journal = seeded_journal(&dir.path().join("k.jsonl"));
        assert!(resolve_continuation(
            &journal,
            "Generate four visual concepts for the home screen"
        )
        .is_none());
    }

    #[test]
    fn empty_journal_yields_none() {
        let dir = tempfile::tempdir().unwrap();
        let journal = KnowledgeJournal::open(&dir.path().join("k.jsonl")).unwrap();
        assert!(resolve_continuation(&journal, "make it darker").is_none());
    }

    #[test]
    fn continuation_without_selection_falls_back_to_first_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let mut j = KnowledgeJournal::open(&dir.path().join("k.jsonl")).unwrap();
        let node = |id: &str, kind: NodeKind| KnowledgeNode {
            id: id.into(),
            kind,
            label: id.into(),
        };
        j.append_node(node("session_tap_S1", NodeKind::Session))
            .unwrap();
        j.append_node(node("artefact_art_X", NodeKind::Artefact))
            .unwrap();
        j.append_edge(KnowledgeEdge {
            from: "artefact_art_X".into(),
            rel: Relationship::BelongsToSession,
            to: "session_tap_S1".into(),
            confidence: 1.0,
            origin: Origin::AgentJudgement,
        })
        .unwrap();
        let cont = resolve_continuation(&j, "now make it lighter").unwrap();
        assert_eq!(cont.parent_artifact_raw, "artefact_art_X");
    }

    #[test]
    fn strip_directive_removes_leading_markers() {
        assert_eq!(strip_directive("Make it darker"), "darker");
        assert_eq!(strip_directive("darken the hero"), "the hero");
        assert_eq!(
            strip_directive("Generate four concepts"),
            "Generate four concepts"
        );
    }

    #[test]
    fn lowercase_verb_prompt_is_continuation() {
        let dir = tempfile::tempdir().unwrap();
        let journal = seeded_journal(&dir.path().join("k.jsonl"));
        assert!(resolve_continuation(&journal, "adjust the selected concept").is_some());
    }
}
