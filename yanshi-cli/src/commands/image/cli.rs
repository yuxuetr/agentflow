use clap::{Args, Subcommand};

use super::{generate, understand};

#[derive(Args)]
pub struct ImageArgs {
  #[command(subcommand)]
  command: ImageCommands,
}

#[derive(Subcommand)]
enum ImageCommands {
  Generate {
    prompt: String,
    #[arg(short, long)]
    model: Option<String>,
    #[arg(short, long, default_value = "1024x1024")]
    size: String,
    #[arg(short, long)]
    output: String,
    #[arg(short, long, default_value = "b64_json")]
    format: String,
    #[arg(long, default_value_t = 20)]
    steps: u32,
    #[arg(long, default_value_t = 7.5)]
    cfg_scale: f32,
    #[arg(long)]
    seed: Option<u64>,
    #[arg(long)]
    strength: Option<f32>,
    #[arg(long)]
    input_image: Option<String>,
  },
  Understand {
    image_path: String,
    prompt: String,
    #[arg(short, long)]
    model: Option<String>,
    #[arg(short, long)]
    temperature: Option<f32>,
    #[arg(long)]
    max_tokens: Option<u32>,
    #[arg(short, long)]
    output: Option<String>,
  },
}

pub async fn dispatch(args: ImageArgs) -> anyhow::Result<()> {
  match args.command {
    ImageCommands::Generate {
      prompt,
      model,
      size,
      output,
      format,
      steps,
      cfg_scale,
      seed,
      strength,
      input_image,
    } => {
      generate::execute(
        prompt,
        model,
        size,
        output,
        format,
        steps,
        cfg_scale,
        seed,
        strength,
        input_image,
      )
      .await
    }
    ImageCommands::Understand {
      image_path,
      prompt,
      model,
      temperature,
      max_tokens,
      output,
    } => understand::execute(image_path, prompt, model, temperature, max_tokens, output).await,
  }
}
