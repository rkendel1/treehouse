#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTransition {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessWorkflow {
    pub name: String,
    pub states: Vec<String>,
    pub transitions: Vec<ProcessTransition>,
}
