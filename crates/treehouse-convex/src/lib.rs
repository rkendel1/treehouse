use treehouse_application_model::{pluralize, ApplicationModel};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub relative_path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvexArtifacts {
    pub files: Vec<GeneratedFile>,
}

pub fn compile_convex(model: &ApplicationModel) -> ConvexArtifacts {
    let mut files = vec![GeneratedFile {
        relative_path: "schema.ts".to_string(),
        content: generate_schema(model),
    }];

    for entity in &model.entities {
        files.push(GeneratedFile {
            relative_path: format!("{}.ts", entity.name.to_lowercase()),
            content: generate_entity_functions(entity.name.as_str()),
        });
    }

    ConvexArtifacts { files }
}

fn generate_schema(model: &ApplicationModel) -> String {
    let mut out = String::from("import { defineSchema, defineTable } from \"convex/server\";\n");
    out.push_str("import { v } from \"convex/values\";\n\n");
    out.push_str("export default defineSchema({\n");
    for entity in &model.entities {
        out.push_str(&format!("  {}: defineTable({{\n", pluralize(&entity.name)));
        for field in &entity.fields {
            if field.primary && field.name.eq_ignore_ascii_case("id") {
                continue;
            }
            out.push_str(&format!(
                "    {}: {},\n",
                camel_case(&field.name),
                convex_type(&field.field_type)
            ));
        }
        out.push_str("  }),\n");
    }
    out.push_str("});\n");
    out
}

fn generate_entity_functions(entity: &str) -> String {
    let route = pluralize(entity);
    format!(
        "import {{ mutation, query }} from \"./_generated/server\";\n\
         import {{ v }} from \"convex/values\";\n\n\
         export const list{entity} = query({{\n\
           args: {{}},\n\
           handler: async (ctx) => ctx.db.query(\"{route}\").collect(),\n\
         }});\n\n\
         export const get{entity} = query({{\n\
           args: {{ id: v.id(\"{route}\") }},\n\
           handler: async (ctx, args) => ctx.db.get(args.id),\n\
         }});\n\n\
         export const create{entity} = mutation({{\n\
           args: {{}},\n\
           handler: async (ctx, args) => ctx.db.insert(\"{route}\", args),\n\
         }});\n\n\
         export const update{entity} = mutation({{\n\
           args: {{ id: v.id(\"{route}\"), patch: v.any() }},\n\
           handler: async (ctx, args) => ctx.db.patch(args.id, args.patch),\n\
         }});\n",
    )
}

fn camel_case(name: &str) -> String {
    let mut out = String::new();
    let mut capitalize = false;
    for ch in name.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            capitalize = true;
            continue;
        }
        if capitalize {
            out.push(ch.to_ascii_uppercase());
            capitalize = false;
        } else {
            out.push(ch.to_ascii_lowercase());
        }
    }
    out
}

fn convex_type(field_type: &str) -> &'static str {
    match field_type {
        "number" | "money" => "v.number()",
        "boolean" => "v.boolean()",
        "timestamp" => "v.number()",
        "array" => "v.array(v.any())",
        "object" => "v.any()",
        _ => "v.string()",
    }
}

#[cfg(test)]
mod tests {
    use treehouse_application_model::{
        ApplicationInfo, ApplicationModel, Entity, Field, GenerationMetadata, PermissionPolicy,
    };

    use super::*;

    #[test]
    fn compiles_convex_schema_and_entity_functions() {
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

        let artifacts = compile_convex(&model);
        assert!(artifacts
            .files
            .iter()
            .any(|file| file.relative_path == "schema.ts" && file.content.contains("customers")));
        assert!(artifacts
            .files
            .iter()
            .any(|file| file.relative_path == "customer.ts"
                && file.content.contains("createCustomer")));
    }
}
