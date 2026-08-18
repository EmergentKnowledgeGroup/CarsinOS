use anyhow::Context;
use carsinos_effect_recorder::{RecorderEndpoint, TestRecorderFixture};
use carsinos_protocol::execass_recorder::{RecorderReplyV1, RecorderRequestV1};
use clap::Parser;
use std::path::PathBuf;

/// A deliberately separate runtime process used by the EA-213 crash matrix.
/// It owns no provider access and can only send the closed recorder protocol.
#[derive(Debug, Parser)]
#[command(name = "ea213-runtime-harness")]
struct Arguments {
    #[arg(long)]
    state_root: PathBuf,
    #[arg(long)]
    request: PathBuf,
    #[arg(long)]
    reply: PathBuf,
    #[arg(long)]
    die_before_connect: bool,
    #[arg(long)]
    wrong_channel_key: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    let request: RecorderRequestV1 = serde_json::from_slice(
        &std::fs::read(&arguments.request).context("reading runtime-harness request")?,
    )
    .context("decoding runtime-harness request")?;
    if arguments.die_before_connect {
        return Ok(());
    }
    let binding = request.binding().clone();
    let fixture = TestRecorderFixture::for_root(&binding.canonical_root_identity);
    let endpoint = RecorderEndpoint::for_binding(
        &arguments.state_root,
        &binding.installation_id,
        binding.state_root_generation,
    );
    let client = if arguments.wrong_channel_key {
        fixture.client_with_channel_key(endpoint, [0xabu8; 32])
    } else {
        fixture.client(endpoint)
    };
    let reply: RecorderReplyV1 = client.send(request).await?;
    std::fs::write(&arguments.reply, serde_json::to_vec(&reply)?)
        .context("writing runtime-harness reply")?;
    Ok(())
}
