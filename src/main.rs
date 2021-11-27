use std::{
    collections::hash_map::DefaultHasher,
    env,
    hash::{Hash, Hasher},
    path::PathBuf,
    process::Command,
    sync::mpsc::channel,
    time::Duration,
};

use clap::Parser;
use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};

#[derive(Parser)]
#[clap(author, about, version)]
struct Flags {
    #[clap(short, long, default_value = "./.rstrtr")]
    /// Change control file path.
    rstrtr: PathBuf,

    #[clap(short, long)]
    /// Use a control file in a tmp dir.
    tmp_dir: bool,

    #[clap(subcommand)]
    /// Subcommand.
    subcommand: Subcommand,
}

#[derive(Parser)]
enum Subcommand {
    /// Run & restart command upon it exiting.
    Run {
        #[clap(required = true)]
        /// The command to run. Separate with -- if required.
        command: Vec<String>,
    },
    /// Instruct rstrtr ill and restart command.
    Restart,
    /// Instruct rstrtr to kill the command and quit.
    Quit,
}

fn main() -> anyhow::Result<()> {
    let mut flags = Flags::parse();

    if flags.tmp_dir {
        flags.rstrtr = env::temp_dir();
        flags
            .rstrtr
            .push(format!("rstrtr.{}", calculate_hash(env::current_dir()?)));
    }

    match &flags.subcommand {
        Subcommand::Run { command } => {
            run(command, &flags)?;
        }
        Subcommand::Restart => {
            std::fs::write(&flags.rstrtr, "\n")?;
        }
        Subcommand::Quit => {
            std::fs::remove_file(&flags.rstrtr)?;
        }
    }

    Ok(())
}

fn calculate_hash(t: PathBuf) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

fn run(command: &[String], flags: &Flags) -> anyhow::Result<()> {
    std::fs::write(&flags.rstrtr, "\n")?;

    let (tx, rx) = channel();
    let mut watcher = watcher(tx, Duration::from_millis(100))?;
    watcher.watch(&flags.rstrtr, RecursiveMode::NonRecursive)?;

    let mut keep_going = true;
    while keep_going {
        let mut proc = {
            let res = Command::new(&command[0]).args(&command[1..]).spawn();
            match res {
                Err(e) => {
                    println!("Error {:?} executing command", e);
                    break;
                }
                Ok(proc) => proc,
            }
        };

        loop {
            let mut restart = false;
            if let Ok(msg) = rx.recv_timeout(Duration::from_millis(50)) {
                match msg {
                    DebouncedEvent::Write(_) => {
                        restart = true;
                    }
                    DebouncedEvent::Remove(_) => {
                        keep_going = false;
                    }
                    _ => {}
                };
            }
            if restart || !keep_going {
                let _ = proc.kill();
            }
            if !keep_going {
                break;
            }
            match proc.try_wait() {
                Ok(Some(exit)) => {
                    println!("Exit {}", exit);
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    println!("Error {:?} try_wait on process", e);
                }
            }
        }
        if keep_going {
            println!("Restarting...");
        }
    }
    println!("Quitting...");

    Ok(())
}
