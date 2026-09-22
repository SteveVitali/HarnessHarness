//! The `default` context policy with six reserved slots (R-2.4.1 C0 slice; §5c.1).
//! **Throwaway Stage-0 subset.**
//!
//! §9.1 R-2.4.1 slice: "the `default` context policy with six reserved slots;
//! `context.assembled` with `context_label`; `ContextWindowExceeded → stop`; layer A memory
//! reached only through hand-authored read tools (no `R-2.4.3⁰` content — that lands at S1)".
//! The compaction family, occupancy gauge and retrieval land at S1.19/S2.8; this is the fixed
//! six-slot assembler.
//!
//! Layer-A memory is **not** injected here — it is reached only through the `read_file` tool
//! (see [`crate::tools::ToolCall::ReadFile`]), which keeps memory content at `external`
//! authority until the model chooses to read it.

use crate::provenance::AuthorityClass;
use crate::tools::ToolResult;

/// The six reserved context slots (fixed at Stage 0). A slot's content carries the authority
/// of its source, so the assembled `context_label` is the join over the slots present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// The harness system preamble (definition authority).
    System,
    /// The principal's task (principal authority).
    Task,
    /// The react scratch/plan (definition authority).
    Scratch,
    /// Prior assistant turns (external — lifted model text).
    History,
    /// The latest tool observation (external — lifted tool output).
    Observation,
    /// The tool catalogue description (definition authority).
    Tools,
}

impl Slot {
    pub fn all() -> [Slot; 6] {
        [
            Slot::System,
            Slot::Task,
            Slot::Scratch,
            Slot::History,
            Slot::Observation,
            Slot::Tools,
        ]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Slot::System => "system",
            Slot::Task => "task",
            Slot::Scratch => "scratch",
            Slot::History => "history",
            Slot::Observation => "observation",
            Slot::Tools => "tools",
        }
    }

    /// The authority class of content placed in this slot.
    pub fn authority(self) -> AuthorityClass {
        match self {
            Slot::System | Slot::Scratch | Slot::Tools => AuthorityClass::Definition,
            Slot::Task => AuthorityClass::Principal,
            Slot::History | Slot::Observation => AuthorityClass::External,
        }
    }
}

/// The result of assembling a context: the rendered prompt and its `context_label` (the join
/// over the authority classes of the slots present — `external` when any lifted content is in).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assembled {
    pub prompt: String,
    pub context_label: AuthorityClass,
    pub char_len: usize,
}

/// A stop condition: the assembled context exceeded the window ceiling
/// (`ContextWindowExceeded → stop`, R-2.4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextWindowExceeded {
    pub char_len: usize,
    pub ceiling: usize,
}

/// The fixed `default` policy: a character-count window ceiling standing in for the token
/// window (the `TokenVector`-based occupancy gauge lands at S1.14/S1.19).
#[derive(Debug, Clone, Copy)]
pub struct ContextPolicy {
    pub window_chars: usize,
}

impl ContextPolicy {
    pub fn default_policy(window_chars: usize) -> Self {
        Self { window_chars }
    }

    /// Assemble the six slots into one prompt, stamping the `context_label`. `history` and the
    /// latest `observation` are lifted content and force the label to `external`.
    pub fn assemble(
        &self,
        system: &str,
        task: &str,
        scratch: &str,
        history: &[String],
        observation: Option<&ToolResult>,
        tools_desc: &str,
    ) -> Result<Assembled, ContextWindowExceeded> {
        let mut prompt = String::new();
        // Meet over the slots present; the fold identity is the canonical top (`kernel`).
        let mut label = AuthorityClass::Kernel;
        let push = |slot: Slot, text: &str, prompt: &mut String, label: &mut AuthorityClass| {
            if text.is_empty() {
                return;
            }
            prompt.push_str(&format!("[{}]\n{}\n", slot.as_str(), text));
            if slot.authority() < *label {
                *label = slot.authority();
            }
        };
        push(Slot::System, system, &mut prompt, &mut label);
        push(Slot::Task, task, &mut prompt, &mut label);
        push(Slot::Scratch, scratch, &mut prompt, &mut label);
        push(Slot::History, &history.join("\n"), &mut prompt, &mut label);
        if let Some(obs) = observation {
            push(Slot::Observation, &obs.output, &mut prompt, &mut label);
        }
        push(Slot::Tools, tools_desc, &mut prompt, &mut label);

        let char_len = prompt.chars().count();
        if char_len > self.window_chars {
            return Err(ContextWindowExceeded {
                char_len,
                ceiling: self.window_chars,
            });
        }
        Ok(Assembled {
            prompt,
            context_label: label,
            char_len,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_reserved_slots_exist() {
        assert_eq!(Slot::all().len(), 6);
    }

    #[test]
    fn label_is_external_when_lifted_content_present() {
        let p = ContextPolicy::default_policy(10_000);
        let a = p
            .assemble("sys", "task", "", &["prior turn".into()], None, "tools")
            .unwrap();
        assert_eq!(a.context_label, AuthorityClass::External);
        assert!(a.prompt.contains("[history]"));
    }

    #[test]
    fn label_is_principal_join_without_lifted_content() {
        // Only system(def), task(principal), tools(def): under the **canonical** order
        // `principal < definition` (ADR-0033 D2), so the meet floor is `principal` — the
        // principal's task text caps the assembled context. (The retired Stage-0 subset
        // ordered `definition < principal` and yielded `definition` here — the semantic
        // correction is recorded in ADR-0230.)
        let p = ContextPolicy::default_policy(10_000);
        let a = p.assemble("sys", "task", "", &[], None, "tools").unwrap();
        assert_eq!(a.context_label, AuthorityClass::Principal);
    }

    #[test]
    fn over_window_yields_context_window_exceeded() {
        let p = ContextPolicy::default_policy(5);
        let e = p
            .assemble("a very long system preamble", "task", "", &[], None, "")
            .unwrap_err();
        assert!(e.char_len > e.ceiling);
    }
}
