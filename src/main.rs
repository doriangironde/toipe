use anyhow::Result;
use clap::StructOpt;

use toipe::config::ToipeConfig;
use toipe::Toipe;

fn main() -> Result<()> {
    let config = ToipeConfig::parse();

    if config.stats {
        let records = toipe::stats::load();
        toipe::stats::print_stats(&records);
        return Ok(());
    }

    let mut toipe = Toipe::new(config)?;

    loop {
        let (keep_going, _) = toipe.test()?;
        if !keep_going {
            break;
        }
        toipe.restart()?;
    }
    Ok(())
}
