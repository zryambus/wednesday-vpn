use std::{process::Command};
use anyhow::{Result, Context};
use std::{io::{Write, Read}, process::Stdio};

pub fn gen_keys() -> Result<(String, String)> {
    let private_key = String::from_utf8(
        Command::new("wg").arg("genkey").output()?.stdout
    )
        .context("Failed to get private key from `wg genkey` output")?
        .trim()
        .to_owned();

    let cmd = Command::new("wg").arg("pubkey")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to spawn `wg pubkey` process")?;
    cmd.stdin.unwrap()
        .write_all(private_key.as_bytes())
        .context("Failed to pass input to `wg pubkey` process")?;

    let mut public_key = String::new();
    cmd.stdout.unwrap()
        .read_to_string(&mut public_key)
        .context("Failed to read output from `wg pubkey` to string")?;
    public_key = public_key.trim().to_owned();

    Ok((private_key, public_key))
}

#[test]
fn gen_keys_test() {
    let keys = gen_keys().expect("Could not generate keys");
    println!("{}\n{}", keys.0, keys.1);
}