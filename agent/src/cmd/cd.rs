use std::error::Error;
use std::path::Path;
use std::env::set_current_dir;
use litcrypt::lc;
use crate::cmd::{CommandArgs, command_out};

/// Changes the directory using system tools
/// Rather than the shell
pub fn handle(cmd_args: &mut CommandArgs) -> Result<String, Box<dyn Error>> {
    let path_arg = cmd_args.nth(0).unwrap_or_else(|| ".".to_string());
    let new_path = Path::new(&path_arg);
    match set_current_dir(new_path) {
        Ok(_) => command_out!("Changed to ", &path_arg),
        Err(e) => Ok(format!("{e}"))
    }
}

