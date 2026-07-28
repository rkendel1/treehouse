use crate::simulation::SystemTwin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactAnalysis {
    pub affected_database: Vec<String>,
    pub affected_apis: Vec<String>,
    pub affected_processes: Vec<String>,
    pub affected_ui: Vec<String>,
    pub risk: String,
}

pub fn analyze_status_field_removal(entity: &str, twin: &SystemTwin) -> ImpactAnalysis {
    let entity_lc = entity.to_ascii_lowercase();
    ImpactAnalysis {
        affected_database: twin
            .entities
            .iter()
            .filter(|candidate| candidate.to_ascii_lowercase() == entity_lc)
            .cloned()
            .collect(),
        affected_apis: twin
            .apis
            .iter()
            .filter(|api| api.to_ascii_lowercase().contains(&entity_lc))
            .cloned()
            .collect(),
        affected_processes: twin
            .processes
            .iter()
            .filter(|process| process.to_ascii_lowercase().contains(&entity_lc))
            .cloned()
            .collect(),
        affected_ui: vec![format!("{} workflow screen", entity)],
        risk: "high".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_high_impact_for_status_removal() {
        let twin = SystemTwin {
            entities: vec!["orders".to_string()],
            processes: vec!["order lifecycle".to_string()],
            apis: vec!["PATCH /orders/:id".to_string()],
            permissions: Vec::new(),
            events: Vec::new(),
        };

        let analysis = analyze_status_field_removal("orders", &twin);
        assert_eq!(analysis.risk, "high");
        assert!(analysis
            .affected_apis
            .iter()
            .any(|api| api == "PATCH /orders/:id"));
    }
}
