use anyhow::Context as _;
use std::process::Command;

use crate::CommandOutput;

pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> anyhow::Result<CommandOutput>;
}

#[derive(Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> anyhow::Result<CommandOutput> {
        let output = Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("failed to run `{program}`"))?;

        Ok(CommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}
