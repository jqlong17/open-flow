use anyhow::Result;

pub async fn run(json: bool) -> Result<()> {
    let snapshot = crate::audio::list_input_devices()?;

    if json {
        println!("{}", serde_json::to_string(&snapshot)?);
        return Ok(());
    }

    println!("Open Flow Audio Input Devices");
    println!();
    if let Some(default_device_name) = snapshot.default_device_name.as_deref() {
        println!("Default: {}", default_device_name);
    } else {
        println!("Default: <none>");
    }
    println!();

    for device in snapshot.devices {
        println!(
            "{} {}",
            if device.is_default { "*" } else { "-" },
            device.name
        );
    }

    Ok(())
}
