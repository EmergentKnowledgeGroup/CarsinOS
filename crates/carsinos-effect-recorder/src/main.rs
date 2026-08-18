use carsinos_effect_recorder::{run_recorder_service, RecorderServiceLaunch};
use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "carsinos-effect-recorder")]
struct Arguments {
    #[arg(long)]
    state_root: PathBuf,
    #[arg(long)]
    database: PathBuf,
    /// Exit when the supervising runtime host closes this inherited stdin.
    #[arg(long)]
    parent_stdin: bool,
    #[cfg(feature = "test-support")]
    #[arg(long)]
    test_fake_provider: bool,
    #[cfg(feature = "test-support")]
    #[arg(
        long,
        requires = "test_fake_provider",
        requires = "test_coordination_root"
    )]
    test_failpoint: Option<carsinos_effect_recorder::TestFailpoint>,
    #[cfg(feature = "test-support")]
    #[arg(long, requires = "test_failpoint")]
    test_coordination_root: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let arguments = Arguments::parse();
    let service = run_recorder_service(RecorderServiceLaunch {
        state_root: arguments.state_root,
        database: arguments.database,
        #[cfg(feature = "test-support")]
        test_fake_provider: arguments.test_fake_provider,
        #[cfg(feature = "test-support")]
        test_failpoint: arguments.test_failpoint,
        #[cfg(feature = "test-support")]
        test_coordination_root: arguments.test_coordination_root,
    });
    if !arguments.parent_stdin {
        return service.await;
    }
    tokio::pin!(service);
    let mut parent = tokio::io::stdin();
    let mut byte = [0u8; 1];
    loop {
        tokio::select! {
            result = &mut service => return result,
            read = tokio::io::AsyncReadExt::read(&mut parent, &mut byte) => {
                match read? {
                    0 => return Ok(()),
                    _ => continue,
                }
            }
        }
    }
}
