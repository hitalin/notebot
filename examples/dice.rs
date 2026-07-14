//! コマンドルーターのデモ。
//!
//! - `@bot dice` → 1d6
//! - `@bot dice 20` → 1d20
//! - `@bot dice 6 3` → 3d6
//! - `@bot ping` → pong
//! - その他のメンション → 使い方を返信

use notebot::{Bot, Ctx};
use rand::Rng;

fn roll(ctx: &Ctx) -> String {
    let sides: u32 = ctx.args().first().and_then(|s| s.parse().ok()).unwrap_or(6);
    let count: u32 = ctx.args().get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let (sides, count) = (sides.clamp(2, 1000), count.clamp(1, 100));
    let mut rng = rand::rng();
    let rolls: Vec<u32> = (0..count).map(|_| rng.random_range(1..=sides)).collect();
    let total: u32 = rolls.iter().sum();
    if count == 1 {
        format!("🎲 {total}")
    } else {
        let detail = rolls
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(" + ");
        format!("🎲 {detail} = {total}")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "notebot=info,notecli=warn".into()),
        )
        .init();

    Bot::builder()
        .command("ping", |ctx| async move {
            ctx.reply("pong").await?;
            Ok(())
        })
        .command("dice", |ctx| async move {
            let result = roll(&ctx);
            ctx.reply(&result).await?;
            Ok(())
        })
        .on_mention(|ctx| async move {
            ctx.reply("使い方: `dice [面数] [個数]` / `ping`").await?;
            Ok(())
        })
        .build()?
        .run()
        .await?;
    Ok(())
}
