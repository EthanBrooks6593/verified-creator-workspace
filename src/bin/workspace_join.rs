use std::{env, process};
use verified_creator_workspace::{
    infrai_client::InfraiClient,
    workspace_join::{verify_and_join, JoinRequest},
};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("workspace join failed: {error}");
        process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let request = JoinRequest {
        company_domain: required_arg(&mut args, "company domain")?,
        employee_email: required_arg(&mut args, "employee email")?,
        employee_name: required_arg(&mut args, "employee name")?,
        txt_name: required_arg(&mut args, "TXT record name")?,
        txt_content: required_arg(&mut args, "TXT record content")?,
    };

    let infrai = InfraiClient::from_env()?;
    let decision = verify_and_join(&infrai, request).await?;
    println!("{}", serde_json::to_string_pretty(&decision)?);
    Ok(())
}

fn required_arg(
    args: &mut impl Iterator<Item = String>,
    label: &'static str,
) -> Result<String, Box<dyn std::error::Error>> {
    args.next().ok_or_else(|| format!("missing {label}").into())
}
