pub enum StatusExpectation {
    Exact(u16),
}

pub fn assert_status(expectation: StatusExpectation, actual: u16) -> bool {
    match expectation {
        StatusExpectation::Exact(expected) => expected == actual,
    }
}
