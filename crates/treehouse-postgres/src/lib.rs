use treehouse_application_model::{pluralize, to_snake_case, ApplicationModel, RelationshipType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub relative_path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresArtifacts {
    pub files: Vec<GeneratedFile>,
}

pub fn compile_postgres(model: &ApplicationModel) -> PostgresArtifacts {
    let schema = generate_schema(model);
    let migration_schema = schema.clone();
    let seed = generate_seed(model);
    let api = serde_json::to_string_pretty(&model.api).unwrap_or_else(|_| "[]".to_string());
    let docs = render_docs(model);
    PostgresArtifacts {
        files: vec![
            GeneratedFile {
                relative_path: "schema.sql".to_string(),
                content: schema.to_string(),
            },
            GeneratedFile {
                relative_path: "migrations/001_initial.sql".to_string(),
                content: migration_schema,
            },
            GeneratedFile {
                relative_path: "seed.sql".to_string(),
                content: seed,
            },
            GeneratedFile {
                relative_path: "api/endpoints.json".to_string(),
                content: api,
            },
            GeneratedFile {
                relative_path: "documentation.md".to_string(),
                content: docs,
            },
        ],
    }
}

fn generate_schema(model: &ApplicationModel) -> String {
    let mut out = String::new();
    for entity in &model.entities {
        let table = pluralize(&entity.name);
        out.push_str(&format!("CREATE TABLE {table} (\n"));

        let mut column_lines = Vec::new();
        for field in &entity.fields {
            let mut line = format!(
                "  {} {}",
                to_snake_case(&field.name),
                sql_type(&field.field_type)
            );
            if field.required {
                line.push_str(" NOT NULL");
            }
            if field.primary {
                line.push_str(" PRIMARY KEY");
            }
            if field.unique {
                line.push_str(" UNIQUE");
            }
            column_lines.push(line);
        }

        for relationship in &entity.relationships {
            if relationship.relationship_type != RelationshipType::ManyToOne {
                continue;
            }
            let target_table = pluralize(&relationship.target);
            let fk_column = format!("{}_id", to_snake_case(&relationship.target));
            if !column_lines
                .iter()
                .any(|line| line.starts_with(&format!("  {fk_column} ")))
            {
                column_lines.push(format!("  {fk_column} UUID"));
            }
            column_lines.push(format!(
                "  CONSTRAINT fk_{}_{} FOREIGN KEY ({fk_column}) REFERENCES {target_table}(id)",
                to_snake_case(&table),
                to_snake_case(&target_table)
            ));
        }

        out.push_str(&column_lines.join(",\n"));
        out.push_str("\n);\n\n");
    }
    out
}

fn generate_seed(model: &ApplicationModel) -> String {
    let mut out = String::new();
    for entity in &model.entities {
        let table = pluralize(&entity.name);
        out.push_str(&format!("-- seed template for {table}\n"));
        out.push_str(&format!("INSERT INTO {table} DEFAULT VALUES;\n\n"));
    }
    out
}

fn render_docs(model: &ApplicationModel) -> String {
    let mut out = format!(
        "# {}\n\nVersion: {}\n\n",
        model.application.name, model.application.version
    );
    out.push_str("## Entities\n\n");
    for entity in &model.entities {
        out.push_str(&format!(
            "- **{}** ({:.0}% confidence)\n",
            entity.name,
            entity.confidence * 100.0
        ));
    }
    out.push_str("\n## API\n\n");
    for endpoint in &model.api {
        out.push_str(&format!("- `{}` `{}`\n", endpoint.method, endpoint.path));
    }
    out
}

fn sql_type(model_type: &str) -> &'static str {
    match model_type {
        "uuid" => "UUID",
        "email" | "phone" | "url" | "string" | "status_enum" => "TEXT",
        "money" => "DECIMAL(12, 2)",
        "number" => "NUMERIC",
        "boolean" => "BOOLEAN",
        "timestamp" => "TIMESTAMP",
        "array" | "object" => "JSONB",
        _ => "TEXT",
    }
}

#[cfg(test)]
mod tests {
    use treehouse_application_model::{
        ApplicationInfo, ApplicationModel, Entity, Field, GenerationMetadata, PermissionPolicy,
    };

    use super::*;

    #[test]
    fn compiles_schema_and_migration_files() {
        let model = ApplicationModel {
            application: ApplicationInfo {
                name: "Commerce".to_string(),
                version: "1.0".to_string(),
            },
            entities: vec![Entity {
                name: "Customer".to_string(),
                confidence: 0.9,
                fields: vec![
                    Field {
                        name: "id".to_string(),
                        field_type: "uuid".to_string(),
                        required: true,
                        primary: true,
                        unique: false,
                        confidence: 0.99,
                    },
                    Field {
                        name: "email".to_string(),
                        field_type: "email".to_string(),
                        required: true,
                        primary: false,
                        unique: true,
                        confidence: 0.98,
                    },
                ],
                relationships: Vec::new(),
                constraints: Vec::new(),
            }],
            workflows: Vec::new(),
            permissions: vec![PermissionPolicy {
                entity: "Customer".to_string(),
                list: true,
                get: true,
                create: true,
                update: true,
            }],
            api: Vec::new(),
            experiences: Vec::new(),
            integrations: Vec::new(),
            metadata: GenerationMetadata {
                generated_by: "test".to_string(),
                generated_at_unix: 0,
                source_count: 1,
            },
        };

        let artifacts = compile_postgres(&model);
        assert!(artifacts
            .files
            .iter()
            .any(|file| file.relative_path == "schema.sql"
                && file.content.contains("CREATE TABLE customers")));
        assert!(artifacts
            .files
            .iter()
            .any(|file| file.relative_path == "migrations/001_initial.sql"));
    }
}
