use std::path::Path;

use anyhow::{bail, Result};
use treehouse_mock::run_mock_server;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(model_path) = args.next() else {
        bail!("usage: treehouse-mock <model-file>");
    };

    run_mock_server(Path::new(&model_path))
}
