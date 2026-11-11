use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Choice {
    pub id: String,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_effort: Option<String>,
    pub efforts: Vec<Choice>,
}

impl Model {
    pub fn parse(value: &Value) -> Option<Self> {
        if value["hidden"] == true {
            return None;
        }
        let id = value["model"].as_str()?.to_owned();
        Some(Self {
            name: value["displayName"].as_str().unwrap_or(&id).to_owned(),
            id,
            description: value["description"].as_str().unwrap_or_default().into(),
            default_effort: value["defaultReasoningEffort"].as_str().map(str::to_owned),
            efforts: value["supportedReasoningEfforts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| {
                    Some(Choice {
                        id: e["reasoningEffort"].as_str()?.into(),
                        description: e["description"].as_str().unwrap_or_default().into(),
                    })
                })
                .collect(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct Skill {
    pub name: String,
    pub path: String,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct Quota {
    pub bucket_id: String,
    pub name: String,
    pub used_percent: u64,
    pub minutes: Option<u64>,
    pub resets_at: Option<u64>,
}

pub fn quota_windows(result: &Value) -> Vec<Quota> {
    let buckets: Vec<(&str, &Value)> = match result["rateLimitsByLimitId"].as_object() {
        Some(buckets) if !buckets.is_empty() => {
            buckets.iter().map(|(k, v)| (k.as_str(), v)).collect()
        }
        _ => vec![("Codex", &result["rateLimits"])],
    };
    let mut windows = Vec::new();
    for (id, bucket) in buckets {
        for key in ["primary", "secondary"] {
            if let Some(used) = bucket[key]["usedPercent"].as_u64() {
                windows.push(Quota {
                    bucket_id: bucket["limitId"]
                        .as_str()
                        .unwrap_or(if id == "Codex" { "codex" } else { id })
                        .into(),
                    name: bucket["limitName"].as_str().unwrap_or(id).into(),
                    used_percent: used,
                    minutes: bucket[key]["windowDurationMins"].as_u64(),
                    resets_at: bucket[key]["resetsAt"].as_u64(),
                });
            }
        }
    }
    windows
}
