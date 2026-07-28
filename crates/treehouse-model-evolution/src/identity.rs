//! Identity matching for stable entity identification across model versions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use treehouse_application_model::Entity;

/// Stable identity for an entity across model versions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityIdentity {
    /// The canonical name of the entity.
    pub canonical_name: String,
    /// Known aliases for this entity.
    pub aliases: Vec<String>,
    /// Structural signature based on fields.
    pub structural_signature: String,
    /// Namespace (if any).
    pub namespace: Option<String>,
}

impl EntityIdentity {
    /// Creates a new EntityIdentity from an entity.
    pub fn from_entity(entity: &Entity) -> Self {
        Self {
            canonical_name: entity.name.clone(),
            aliases: vec![],
            structural_signature: compute_structural_signature(entity),
            namespace: None,
        }
    }

    /// Adds an alias to this identity.
    pub fn add_alias(&mut self, alias: impl Into<String>) {
        let alias = alias.into();
        if !self.aliases.contains(&alias) && alias != self.canonical_name {
            self.aliases.push(alias);
        }
    }

    /// Returns true if the given name matches this identity.
    pub fn matches_name(&self, name: &str) -> bool {
        self.canonical_name.eq_ignore_ascii_case(name)
            || self.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
    }
}

/// Computes a structural signature for an entity based on its fields.
fn compute_structural_signature(entity: &Entity) -> String {
    let mut field_sigs: Vec<String> = entity
        .fields
        .iter()
        .map(|f| format!("{}:{}", f.name.to_lowercase(), f.field_type.to_lowercase()))
        .collect();
    field_sigs.sort();
    field_sigs.join("|")
}

/// Matcher for identifying entities across model versions.
#[derive(Debug, Clone, Default)]
pub struct IdentityMatcher {
    /// Known entity identities.
    identities: HashMap<String, EntityIdentity>,
    /// Rename records mapping old names to new names.
    renames: HashMap<String, String>,
}

impl IdentityMatcher {
    /// Creates a new IdentityMatcher.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an entity identity.
    pub fn register(&mut self, identity: EntityIdentity) {
        self.identities
            .insert(identity.canonical_name.to_lowercase(), identity);
    }

    /// Records a rename from one name to another.
    pub fn record_rename(&mut self, from: impl Into<String>, to: impl Into<String>) {
        let from = from.into();
        let to = to.into();

        // Update the identity if it exists
        if let Some(mut identity) = self.identities.remove(&from.to_lowercase()) {
            identity.add_alias(from.clone());
            identity.canonical_name = to.clone();
            self.identities.insert(to.to_lowercase(), identity);
        }

        self.renames.insert(from.to_lowercase(), to);
    }

    /// Gets the canonical name for an entity.
    pub fn canonical_name(&self, name: &str) -> String {
        let lower = name.to_lowercase();

        // Check for renames
        if let Some(renamed_to) = self.renames.get(&lower) {
            return renamed_to.clone();
        }

        // Check identities
        for identity in self.identities.values() {
            if identity.matches_name(name) {
                return identity.canonical_name.clone();
            }
        }

        // Return the original name if no match
        name.to_string()
    }

    /// Attempts to match an entity to a known identity.
    pub fn find_identity(&self, entity: &Entity) -> Option<&EntityIdentity> {
        let name_lower = entity.name.to_lowercase();

        // First, try exact name match
        if let Some(identity) = self.identities.get(&name_lower) {
            return Some(identity);
        }

        // Try alias match
        for identity in self.identities.values() {
            if identity.matches_name(&entity.name) {
                return Some(identity);
            }
        }

        // Try structural signature match for entities with similar structure
        let entity_sig = compute_structural_signature(entity);
        for identity in self.identities.values() {
            if structural_similarity(&identity.structural_signature, &entity_sig) > 0.8 {
                return Some(identity);
            }
        }

        None
    }

    /// Determines if two entities might be the same entity (possibly renamed).
    pub fn might_be_same_entity(&self, old: &Entity, new: &Entity) -> MatchResult {
        // Exact name match
        if old.name.eq_ignore_ascii_case(&new.name) {
            return MatchResult::ExactMatch;
        }

        // Check for recorded renames
        if let Some(renamed_to) = self.renames.get(&old.name.to_lowercase()) {
            if renamed_to.eq_ignore_ascii_case(&new.name) {
                return MatchResult::KnownRename;
            }
        }

        // Structural similarity check
        let old_sig = compute_structural_signature(old);
        let new_sig = compute_structural_signature(new);
        let similarity = structural_similarity(&old_sig, &new_sig);

        if similarity > 0.9 {
            return MatchResult::HighStructuralSimilarity { similarity };
        }

        if similarity > 0.7 {
            return MatchResult::ModerateStructuralSimilarity { similarity };
        }

        // Name similarity heuristics
        let name_sim = name_similarity(&old.name, &new.name);
        if name_sim > 0.8 {
            return MatchResult::NameSimilarity { similarity: name_sim };
        }

        MatchResult::NoMatch
    }
}

/// Result of attempting to match two entities.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchResult {
    /// Exact name match.
    ExactMatch,
    /// Known rename from identity records.
    KnownRename,
    /// High structural similarity (>90%).
    HighStructuralSimilarity { similarity: f64 },
    /// Moderate structural similarity (70-90%).
    ModerateStructuralSimilarity { similarity: f64 },
    /// Name similarity match.
    NameSimilarity { similarity: f64 },
    /// No match found.
    NoMatch,
}

impl MatchResult {
    /// Returns true if this indicates a likely match.
    pub fn is_likely_match(&self) -> bool {
        matches!(
            self,
            MatchResult::ExactMatch
                | MatchResult::KnownRename
                | MatchResult::HighStructuralSimilarity { .. }
        )
    }

    /// Returns true if this indicates a possible match.
    pub fn is_possible_match(&self) -> bool {
        matches!(
            self,
            MatchResult::ExactMatch
                | MatchResult::KnownRename
                | MatchResult::HighStructuralSimilarity { .. }
                | MatchResult::ModerateStructuralSimilarity { .. }
                | MatchResult::NameSimilarity { .. }
        )
    }
}

/// Computes structural similarity between two signatures.
fn structural_similarity(sig1: &str, sig2: &str) -> f64 {
    if sig1.is_empty() && sig2.is_empty() {
        return 1.0;
    }
    if sig1.is_empty() || sig2.is_empty() {
        return 0.0;
    }

    let fields1: std::collections::HashSet<&str> = sig1.split('|').collect();
    let fields2: std::collections::HashSet<&str> = sig2.split('|').collect();

    let intersection = fields1.intersection(&fields2).count();
    let union = fields1.union(&fields2).count();

    if union == 0 {
        return 1.0;
    }

    intersection as f64 / union as f64
}

/// Computes name similarity using simple heuristics.
fn name_similarity(name1: &str, name2: &str) -> f64 {
    let n1 = name1.to_lowercase();
    let n2 = name2.to_lowercase();

    if n1 == n2 {
        return 1.0;
    }

    // Check if one is a substring of the other
    if n1.contains(&n2) || n2.contains(&n1) {
        return 0.8;
    }

    // Check for common prefixes
    let common_prefix_len = n1
        .chars()
        .zip(n2.chars())
        .take_while(|(c1, c2)| c1 == c2)
        .count();

    let max_len = n1.len().max(n2.len());
    if max_len == 0 {
        return 1.0;
    }

    common_prefix_len as f64 / max_len as f64
}

#[cfg(test)]
mod tests {
    use treehouse_application_model::Field;

    use super::*;

    fn make_entity(name: &str, fields: &[(&str, &str)]) -> Entity {
        Entity {
            name: name.to_string(),
            confidence: 0.9,
            fields: fields
                .iter()
                .map(|(n, t)| Field {
                    name: n.to_string(),
                    field_type: t.to_string(),
                    required: true,
                    primary: false,
                    unique: false,
                    confidence: 0.9,
                })
                .collect(),
            relationships: vec![],
            constraints: vec![],
        }
    }

    #[test]
    fn entity_identity_from_entity() {
        let entity = make_entity("User", &[("id", "uuid"), ("email", "string")]);
        let identity = EntityIdentity::from_entity(&entity);

        assert_eq!(identity.canonical_name, "User");
        assert!(!identity.structural_signature.is_empty());
    }

    #[test]
    fn entity_identity_matches_name() {
        let mut identity = EntityIdentity::from_entity(&make_entity("User", &[]));
        identity.add_alias("Account");

        assert!(identity.matches_name("User"));
        assert!(identity.matches_name("user"));
        assert!(identity.matches_name("Account"));
        assert!(!identity.matches_name("Order"));
    }

    #[test]
    fn identity_matcher_records_rename() {
        let mut matcher = IdentityMatcher::new();
        matcher.register(EntityIdentity::from_entity(&make_entity("Customer", &[])));
        matcher.record_rename("Customer", "Client");

        assert_eq!(matcher.canonical_name("Customer"), "Client");
        assert_eq!(matcher.canonical_name("Client"), "Client");
    }

    #[test]
    fn identity_matcher_exact_match() {
        let matcher = IdentityMatcher::new();
        let e1 = make_entity("User", &[("id", "uuid"), ("email", "string")]);
        let e2 = make_entity("User", &[("id", "uuid"), ("email", "string")]);

        let result = matcher.might_be_same_entity(&e1, &e2);
        assert_eq!(result, MatchResult::ExactMatch);
    }

    #[test]
    fn identity_matcher_high_structural_similarity() {
        let matcher = IdentityMatcher::new();
        let e1 = make_entity("Customer", &[("id", "uuid"), ("email", "string"), ("name", "string")]);
        let e2 = make_entity("Client", &[("id", "uuid"), ("email", "string"), ("name", "string")]);

        let result = matcher.might_be_same_entity(&e1, &e2);
        assert!(matches!(result, MatchResult::HighStructuralSimilarity { .. }));
        assert!(result.is_likely_match());
    }

    #[test]
    fn identity_matcher_no_match() {
        let matcher = IdentityMatcher::new();
        let e1 = make_entity("User", &[("id", "uuid")]);
        let e2 = make_entity("Invoice", &[("total", "money"), ("date", "timestamp")]);

        let result = matcher.might_be_same_entity(&e1, &e2);
        assert_eq!(result, MatchResult::NoMatch);
        assert!(!result.is_possible_match());
    }

    #[test]
    fn structural_similarity_identical() {
        let sig = "email:string|id:uuid";
        assert_eq!(structural_similarity(sig, sig), 1.0);
    }

    #[test]
    fn structural_similarity_partial() {
        let sig1 = "email:string|id:uuid|name:string";
        let sig2 = "email:string|id:uuid";
        let sim = structural_similarity(sig1, sig2);
        assert!(sim > 0.5);
        assert!(sim < 1.0);
    }
}
