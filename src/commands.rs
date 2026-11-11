use melly::session::Session;

pub const COMMANDS: &[(&str, &str)] = &[
    ("agents", "Jump to the lead or a Luna worker"),
    (
        "spawn",
        "Start Luna: /spawn task · /spawn --edit paths -- task",
    ),
    ("model", "Choose a model for the next turn"),
    ("effort", "Set reasoning strength"),
    ("skills", "Attach a Codex skill to your next message"),
    ("compact", "Compact the lead conversation's context"),
    ("context", "Inspect context and capacity"),
    ("usage", "Account limits and session tokens"),
    ("status", "Session details and response timing"),
    ("thinking", "Expand or collapse thinking summaries"),
    ("activity", "Focus live commands and file changes"),
    ("tools", "Expand or collapse full tool output"),
    (
        "collect",
        "Return to the lead; results join its next message",
    ),
    ("help", "Keyboard shortcuts and commands"),
    ("stop", "Interrupt the active turn"),
    ("quit", "Close Melly"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Commands,
    Models,
    Effort,
    Skills,
}

pub struct Row {
    pub value: String,
    pub label: String,
    pub detail: String,
    pub selected: bool,
    pub skill_index: Option<usize>,
}

pub struct Menu {
    pub kind: Kind,
    pub rows: Vec<Row>,
}

impl Menu {
    pub fn from_draft(draft: &str, session: &Session) -> Option<Self> {
        let text = draft.strip_prefix('/')?;
        if text.contains('\n') {
            return None;
        }
        let (command, query) = text.split_once(' ').unwrap_or((text, ""));
        let has_args = text.contains(' ');
        let kind = if has_args {
            match command {
                "model" => Kind::Models,
                "effort" | "strength" => Kind::Effort,
                "skills" | "skill" => Kind::Skills,
                _ => Kind::Commands,
            }
        } else {
            Kind::Commands
        };
        let rows: Vec<Row> = match kind {
            Kind::Commands => COMMANDS
                .iter()
                .map(|(name, detail)| Row {
                    value: (*name).into(),
                    label: format!("/{name}"),
                    detail: (*detail).into(),
                    selected: false,
                    skill_index: None,
                })
                .collect(),
            Kind::Models => session
                .models
                .iter()
                .map(|m| Row {
                    value: m.id.clone(),
                    label: m.name.clone(),
                    detail: format!("{} · {}", m.id, m.description),
                    selected: session.model.as_ref() == Some(&m.id),
                    skill_index: None,
                })
                .collect(),
            Kind::Effort => session
                .current_model()
                .into_iter()
                .flat_map(|m| &m.efforts)
                .map(|e| Row {
                    value: e.id.clone(),
                    label: e.id.clone(),
                    detail: e.description.clone(),
                    selected: session.effort.as_ref() == Some(&e.id),
                    skill_index: None,
                })
                .collect(),
            Kind::Skills => session
                .skills
                .iter()
                .enumerate()
                .map(|(i, s)| Row {
                    value: s.path.clone(),
                    label: s.name.clone(),
                    detail: format!("{} · {}", s.path, s.description),
                    selected: session.selected_skills.iter().any(|a| a.path == s.path),
                    skill_index: Some(i),
                })
                .collect(),
        };
        let query = if kind == Kind::Commands {
            command
        } else {
            query
        }
        .trim()
        .to_lowercase();
        let query = if query == "strength" && kind == Kind::Commands {
            "effort"
        } else {
            &query
        };
        let mut rows: Vec<_> = rows
            .into_iter()
            .filter(|r| {
                r.label.to_lowercase().contains(query)
                    || r.value.to_lowercase().contains(query)
                    || (kind != Kind::Commands && r.detail.to_lowercase().contains(query))
            })
            .collect();
        rows.sort_by_key(|r| r.value.to_lowercase() != query);
        Some(Self { kind, rows })
    }

    pub fn title(&self) -> &'static str {
        match self.kind {
            Kind::Commands => "Commands",
            Kind::Models => "Choose model",
            Kind::Effort => "Reasoning strength",
            Kind::Skills => "Attach skill",
        }
    }

    pub fn empty(&self, session: &Session) -> &'static str {
        match self.kind {
            Kind::Models if session.models.is_empty() => {
                "Model catalog unavailable. Waiting for Codex."
            }
            Kind::Effort if session.current_model().is_none_or(|m| m.efforts.is_empty()) => {
                "This model has no advertised effort options."
            }
            Kind::Skills if !session.skills_loaded => "Loading skills from Codex…",
            Kind::Skills if session.skills.is_empty() => "No enabled skills found in this project.",
            _ => "No matches. Edit the filter, or Esc then Enter to send literally.",
        }
    }
}
