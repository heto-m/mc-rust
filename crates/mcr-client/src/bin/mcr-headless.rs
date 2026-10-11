use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use mcr_client::script::{Runner, parse};
use mcr_client::{Client, ClientConfig, ClientError, ClientEvent, sanitize_for_log};

const TICK: Duration = Duration::from_millis(50);

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "localhost:25565")]
    addr: String,
    #[arg(long, default_value = "Bot")]
    name: String,
    #[arg(long, default_value_t = 30)]
    seconds: u64,
    #[arg(long)]
    script: Option<PathBuf>,
    #[arg(long, default_value_t = 10)]
    view_distance: i8,
    /// Reply with an empty known-packs list so the server sends all registry data.
    #[arg(long)]
    no_known_packs: bool,
}

fn config(args: &Args) -> ClientConfig {
    let mut cfg = ClientConfig::default().with_addr(&args.addr);
    cfg.name = args.name.clone();
    cfg.view_distance = args.view_distance;
    cfg.known_packs = !args.no_known_packs;
    cfg
}

fn main() -> Result<()> {
    let args = Args::parse();
    let steps = match &args.script {
        Some(path) => parse(&std::fs::read_to_string(path).context("reading script")?)
            .map_err(anyhow::Error::msg)?,
        None => Vec::new(),
    };
    let mut runner = Runner::new(steps);
    let mut client = Client::connect(config(&args)).context("connecting")?;
    eprintln!("joined as {}", args.name);

    let end = Instant::now() + Duration::from_secs(args.seconds);
    let mut next_tick = Instant::now();
    let mut last_report = Instant::now();
    while Instant::now() < end {
        let input = runner.tick(&mut client)?;
        let result = client.tick(&input);
        for line in runner.log.drain(..) {
            println!("{line}");
        }
        for event in client.drain_events() {
            match event {
                ClientEvent::Chat { message, .. } => {
                    println!("chat: {}", sanitize_for_log(&message.to_plain_string()))
                }
                ClientEvent::Died { .. } => println!("died"),
                ClientEvent::PlayerTeleported { id } => println!("teleport {id}"),
                ClientEvent::Disconnected(reason) => {
                    println!("disconnected: {}", reason.to_log_string())
                }
                _ => {}
            }
        }
        if let Err(e) = result {
            if let ClientError::Transferred(target) = &e {
                client = Client::connect(config(&args).for_transfer(target))
                    .context("following transfer")?;
                continue;
            }
            return match e {
                ClientError::Disconnected(_) => Ok(()),
                other => Err(other.into()),
            };
        }
        if last_report.elapsed() >= Duration::from_secs(1) {
            last_report = Instant::now();
            let p = &client.state.player;
            println!(
                "pos {:.2} {:.2} {:.2} ground {} health {} chunks {} entities {} vd {} recv {}",
                p.physics.pos[0],
                p.physics.pos[1],
                p.physics.pos[2],
                p.physics.on_ground,
                p.health,
                client.state.chunks.len(),
                client.state.entities.len(),
                client.state.world.view_distance,
                client.state.chunks_received,
            );
        }
        next_tick += TICK;
        let now = Instant::now();
        if next_tick > now {
            std::thread::sleep(next_tick - now);
        } else if now - next_tick > Duration::from_secs(1) {
            next_tick = now;
        }
    }
    client.disconnect();
    Ok(())
}
