// MIT/Apache2 License

use breadglx::GlDisplay;
use breadx::{DisplayConnection, Result};
use std::env;

fn main() -> Result<()> {
    env::set_var("RUST_LOG", "breadx=warn,breadglx=info");
    env_logger::init();

    let conn = DisplayConnection::create(None, None)?;
    let mut conn = GlDisplay::new(conn).expect("GlDisplay");
    let mut screen = conn
        .create_screen(conn.display().default_screen_index())
        .expect("GlScreen");

    Ok(())
}
