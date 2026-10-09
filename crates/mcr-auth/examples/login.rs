use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use mcr_auth::msa::ClientId;
use mcr_auth::{Authenticator, UreqClient};

fn main() -> Result<()> {
    let client_id = ClientId::from_env()?;
    let cache = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "account.json".into());
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let auth = Authenticator::new(UreqClient::new(), client_id).with_cache(cache);
    let account = auth.ensure_account(
        now,
        |device| {
            println!(
                "Open {} and enter code {}",
                device.verification_uri, device.user_code
            )
        },
        std::thread::sleep,
    )?;

    let profile = &account.profile;
    println!("Logged in as {} ({})", profile.name, profile.uuid()?);
    println!(
        "skins: {}, capes: {}",
        profile.skins.len(),
        profile.capes.len()
    );
    if let Some(keys) = &account.keys {
        println!("chat key expires at {} ms", keys.expires_at_ms);
    }
    Ok(())
}
