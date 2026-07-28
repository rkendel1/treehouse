use std::collections::BTreeMap;

use treehouse_system_graph::Subsystem;

#[derive(Debug, Clone, Default)]
pub struct SubsystemSignals {
    pub entities: Vec<String>,
    pub apis: Vec<String>,
    pub workflows: Vec<String>,
    pub events: Vec<String>,
    pub code_symbols: Vec<String>,
    pub db_signals: Vec<String>,
}

pub fn discover_subsystems(signals: &SubsystemSignals) -> Vec<Subsystem> {
    let mut grouped: BTreeMap<String, Subsystem> = BTreeMap::new();

    for entity in &signals.entities {
        add_signal(&mut grouped, classify_domain(entity), |subsystem| {
            subsystem.entities.push(entity.clone())
        });
    }
    for api in &signals.apis {
        add_signal(&mut grouped, classify_domain(api), |subsystem| {
            subsystem.apis.push(api.clone())
        });
    }
    for workflow in &signals.workflows {
        add_signal(&mut grouped, classify_domain(workflow), |subsystem| {
            subsystem.workflows.push(workflow.clone())
        });
    }
    for event in &signals.events {
        add_signal(&mut grouped, classify_domain(event), |subsystem| {
            subsystem.events.push(event.clone())
        });
    }

    for subsystem in grouped.values_mut() {
        dedupe_sort(&mut subsystem.entities);
        dedupe_sort(&mut subsystem.apis);
        dedupe_sort(&mut subsystem.workflows);
        dedupe_sort(&mut subsystem.events);

        let evidence = subsystem.entities.len() as f32 * 2.0
            + subsystem.apis.len() as f32
            + subsystem.workflows.len() as f32
            + subsystem.events.len() as f32;

        let symbol_hits = signals
            .code_symbols
            .iter()
            .filter(|symbol| classify_domain(symbol) == subsystem.id)
            .count() as f32;

        let db_hits = signals
            .db_signals
            .iter()
            .filter(|signal| classify_domain(signal) == subsystem.id)
            .count() as f32;

        let weighted = evidence + (symbol_hits * 0.75) + (db_hits * 1.25);
        subsystem.confidence = (0.35 + (weighted / 20.0)).clamp(0.45, 0.99);
    }

    grouped.into_values().collect()
}

fn add_signal<F>(grouped: &mut BTreeMap<String, Subsystem>, id: String, update: F)
where
    F: FnOnce(&mut Subsystem),
{
    let entry = grouped.entry(id.clone()).or_insert_with(|| Subsystem {
        id: id.clone(),
        owner: default_owner(&id),
        ..Subsystem::default()
    });
    update(entry);
}

fn dedupe_sort(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

fn default_owner(domain: &str) -> Option<String> {
    let owner = match domain {
        "Billing" => "billing-service",
        "Identity" => "identity-service",
        "Notifications" => "notification-service",
        "Orders" => "order-service",
        "Analytics" => "analytics-service",
        "Workflow" => "workflow-service",
        "Runtime" => "platform-runtime",
        _ => "platform-core",
    };
    Some(owner.to_string())
}

fn classify_domain(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if contains_any(
        &lower,
        &["invoice", "payment", "subscription", "billing", "refund", "transaction"],
    ) {
        return "Billing".to_string();
    }
    if contains_any(
        &lower,
        &[
            "user", "identity", "auth", "tenant", "session", "account", "profile",
        ],
    ) {
        return "Identity".to_string();
    }
    if contains_any(
        &lower,
        &[
            "notification",
            "message",
            "template",
            "delivery",
            "recipient",
            "email",
            "sms",
        ],
    ) {
        return "Notifications".to_string();
    }
    if contains_any(&lower, &["order", "cart", "checkout", "tax", "receipt"]) {
        return "Orders".to_string();
    }
    if contains_any(&lower, &["event", "trace", "runtime", "log"]) {
        return "Runtime".to_string();
    }
    if contains_any(&lower, &["workflow", "state", "process", "orchestration"]) {
        return "Workflow".to_string();
    }
    if contains_any(&lower, &["analytics", "metric", "insight", "reporting"]) {
        return "Analytics".to_string();
    }
    "Core".to_string()
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_multiple_subsystems_from_signals() {
        let subsystems = discover_subsystems(&SubsystemSignals {
            entities: vec![
                "Invoice".to_string(),
                "User".to_string(),
                "Template".to_string(),
            ],
            apis: vec!["POST /invoices".to_string(), "POST /messages".to_string()],
            workflows: vec!["Invoice:draft->paid".to_string()],
            events: vec!["PaymentCompleted".to_string()],
            code_symbols: vec!["src/billing.rs::create_invoice".to_string()],
            db_signals: vec!["table:invoices".to_string()],
        });

        assert!(subsystems.iter().any(|s| s.id == "Billing"));
        assert!(subsystems.iter().any(|s| s.id == "Identity"));
        assert!(subsystems.iter().any(|s| s.id == "Notifications"));
        assert!(subsystems
            .iter()
            .filter(|s| s.id == "Billing")
            .all(|s| s.confidence > 0.6));
    }
}
