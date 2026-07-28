#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub name: String,
    pub route: String,
    pub entities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenMap {
    pub domain: String,
    pub screens: Vec<Screen>,
}
