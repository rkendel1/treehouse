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
        let id = classify_domain(entity);
        let entry = grouped.entry(id.clone()).or_insert_with(|| Subsystem {
            id: id.clone(),
            owner: default_owner(&id),
            ..Subsystem::default()
        });
        entry.entities.push(entity.clone());
    }

    for api in &signals.apis {
        let id = classify_domain(api);
        let entry = grouped.entry(id.clone()).or_insert_with(|| Subsystem {
            id: id.clone(),
            owner: default_owner(&id),
            ..Subsystem::default()
        });
        entry.apis.push(api.clone());
    }

    for workflow in &signals.workflows {
        let id = classify_domain(workflow);
        let entry = grouped.entry(id.clone()).or_insert_with(|| Subsystem {
            id: id.clone(),
            owner: default_owner(&id),
            ..Subsystem::default()
        });
        entry.workflows.push(workflow.clone());
    }

    for event in &signals.events {
        let id = classify_domain(event);
        let entry = grouped.entry(id.clone()).or_insert_with(|| Subsystem {
            id: id.clone(),
            owner: default_owner(&id),
            ..Subsystem::default()
        });
        entry.events.push(event.clone());
    }

    for subsystem in grouped.values_mut() {
        subsystem.entities.sort();
        subsystem.entities.dedup();
        subsystem.apis.sort();
        subsystem.apis.dedup();
        subsystem.workflows.sort();
        subsystem.workflows.dedup();
        subsystem.events.sort();
        subsystem.events.dedup();

        let evidence = subsystem.entities.len()
            + subsystem.apis.len()
            + subsystem.workflows.len()
            + subsystem.events.len();
        let symbol_hits = signals
            .code_symbols
            .iter()
            .filter(|symbol| classify_domain(symbol) == subsystem.id)
            .count();
        let db_hits = signals
            .db_signals
            .iter()
            .filter(|signal| classify_domain(signal) == subsystem.id)
            .count();
        subsystem.confidence = ((evidence + symbol_hits + db_hits) as f32 / 20.0).clamp(0.45, 0.99);
    }

    grouped.into_values().collect()
}

fn default_owner(domain: &str) -> Option<String> {
    let owner = match domain {
        "Billing" => "billing-service",
        "Identity" => "identity-service",
        "Notifications" => "notification-service",
        "Orders" => "order-service",
        "Analytics" => "analytics-service",
        "Workflow" => "workflow-service",
        _ => "platform-core",
    };
    Some(owner.to_string())
}

fn classify_domain(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("invoice")
        || lower.contains("payment")
        || lower.contains("subscription")
        || lower.contains("billing")
        || lower.contains("refund")
    {
        return "Billing".to_string();
    }
    if lower.contains("user")
        || lower.contains("identity")
        || lower.contains("auth")
        || lower.contains("tenant")
        || lower.contains("session")
    {
        return "Identity".to_string();
    }
    if lower.contains("notification")
        || lower.contains("message")
        || lower.contains("template")
        || lower.contains("delivery")
        || lower.contains("recipient")
    {
        return "Notifications".to_string();
    }
    if lower.contains("order") || lower.contains("cart") || lower.contains("checkout") {
        return "Orders".to_string();
    }
    if lower.contains("event") || lower.contains("trace") || lower.contains("runtime") {
        return "Runtime".to_string();
    }
    if lower.contains("workflow") || lower.contains("state") || lower.contains("process") {
        return "Workflow".to_string();
    }
    if lower.contains("analytics") || lower.contains("metric") || lower.contains("insight") {
        return "Analytics".to_string();
    }
    "Core".to_string()
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
    }
}
